//! "Sustituir el Bloc de notas" en Windows 11: el Notepad de la Store registra
//! `HKCU\...\App Paths\notepad.exe` hacia su propio exe, y Win+R resuelve por App
//! Paths antes de llegar al `Debugger` de IFEO (HKLM) que escribe el MSI. Aquí se
//! alinea esa entrada del usuario actual con IFEO. Vive en `notty-update` porque es
//! el único crate que comparten `notty` (autorreparación al arrancar) y `notty-setup`.

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_SZ, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegDeleteKeyW,
    RegGetValueW, RegSetKeyValueW,
};
use windows::core::{PCWSTR, w};

const IFEO_KEY: PCWSTR = w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\notepad.exe");
const APP_PATH_KEY: PCWSTR = w!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\notepad.exe");

fn normalize(path: &str) -> String {
    path.trim().trim_matches('"').replace('/', "\\").to_lowercase()
}

pub fn points_to_notty(value: &str) -> bool {
    let v = normalize(value);
    v == "notty.exe" || v.ends_with("\\notty.exe")
}

pub fn same_path(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

fn read_string(root: HKEY, key: PCWSTR, name: PCWSTR) -> Option<String> {
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
    let mut buf = [0u16; 1024];
    let mut len = std::mem::size_of_val(&buf) as u32;
    let rc = unsafe { RegGetValueW(root, key, name, flags, None, Some(buf.as_mut_ptr().cast()), Some(&mut len)) };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let chars = (len as usize / 2).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..chars]).trim_end_matches('\0').to_string())
}

/// Pone el `(Default)` de App Paths hacia el notty de IFEO si IFEO apunta a notty, y
/// si no, quita esa entrada cuando es nuestra. Dos lecturas de registro como mucho.
pub fn sync() {
    match read_string(HKEY_LOCAL_MACHINE, IFEO_KEY, w!("Debugger")) {
        Some(exe) if points_to_notty(&exe) => {
            if !read_string(HKEY_CURRENT_USER, APP_PATH_KEY, PCWSTR::null()).is_some_and(|v| same_path(&v, &exe)) {
                let data: Vec<u16> = exe.encode_utf16().chain(std::iter::once(0)).collect();
                unsafe {
                    let _ = RegSetKeyValueW(HKEY_CURRENT_USER, APP_PATH_KEY, PCWSTR::null(), REG_SZ.0, Some(data.as_ptr().cast()), (data.len() * 2) as u32);
                }
            }
        }
        _ => forget(),
    }
}

/// Borra App Paths\notepad.exe del usuario actual solo si apunta a notty: si otra
/// herramienta la cambió después, no se toca.
pub fn forget() {
    if read_string(HKEY_CURRENT_USER, APP_PATH_KEY, PCWSTR::null()).is_some_and(|v| points_to_notty(&v)) {
        unsafe {
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, APP_PATH_KEY);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{points_to_notty, same_path};

    #[test]
    fn matches_installed_path_any_case_and_slash() {
        assert!(points_to_notty(r"C:\Program Files\notty\notty.exe"));
        assert!(points_to_notty(r"c:/program files/NOTTY/Notty.EXE"));
        assert!(points_to_notty("\"C:\\Program Files\\notty\\notty.exe\""));
    }

    #[test]
    fn rejects_other_notepads() {
        assert!(!points_to_notty(r"C:\Users\a\AppData\Local\Microsoft\WindowsApps\notepad.exe"));
        assert!(!points_to_notty(r"C:\tools\notnotty.exe"));
        assert!(!points_to_notty(""));
    }

    #[test]
    fn same_path_ignores_case_quotes_and_slashes() {
        assert!(same_path(r"C:\Program Files\notty\notty.exe", "\"c:/program files/notty/NOTTY.exe\""));
        assert!(!same_path(r"C:\Program Files\notty\notty.exe", r"D:\notty\notty.exe"));
    }
}
