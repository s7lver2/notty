//! notty --daemon: sin ventana de documento. Bandeja + atajos globales + pipe.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NOTIFYICONDATAW, Shell_NotifyIconW};
use windows::Win32::UI::Input::KeyboardAndMouse::RegisterHotKey;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG, RegisterClassExW, WM_DESTROY, WM_HOTKEY,
    WNDCLASSEXW,
};
use windows::core::{Result, w};

const ID_HOTKEY_TEMP: i32 = 1;
const ID_HOTKEY_PERM: i32 = 2;
const WM_APP_TRAYICON: u32 = 0x8000 + 1;

pub fn run() -> Result<()> {
    let cfg = match notty_config::load(&notty_config::default_path()) {
        notty_config::LoadResult::Loaded(c) | notty_config::LoadResult::Missing(c) | notty_config::LoadResult::Defaulted(c, _) => c,
    };
    if cfg.hotkey.mechanism != notty_config::HotkeyMechanism::Daemon {
        // El usuario eligió el mecanismo .lnk: nada residente que escuche atajos, pero
        // sí hace falta que los accesos directos existan para que pueda asignarles una
        // tecla rápida a mano (ver `shortcut::create` y Ajustes, Task 10).
        let _ = ensure_lnk_shortcuts();
        return Ok(());
    }

    unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
        let class_name = w!("NottyDaemonClass");
        let icon = notty_ui::window::app_icon(instance.into());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hIcon: icon.unwrap_or_default(),
            hIconSm: icon.unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(Default::default(), class_name, w!("notty (segundo plano)"), Default::default(), 0, 0, 0, 0, None, None, Some(instance.into()), None)?;

        let (temp_vk, temp_mods) = hotkey_or_default("Win+Alt+N");
        let (perm_vk, perm_mods) = hotkey_or_default("Win+Alt+Shift+N");
        let _ = RegisterHotKey(Some(hwnd), ID_HOTKEY_TEMP, windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS(temp_mods), temp_vk);
        let _ = RegisterHotKey(Some(hwnd), ID_HOTKEY_PERM, windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS(perm_mods), perm_vk);

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_APP_TRAYICON,
            hIcon: icon.unwrap_or_default(),
            ..Default::default()
        };
        let tip = w!("notty");
        nid.szTip[..tip.as_wide().len()].copy_from_slice(tip.as_wide());
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);

        let (tx, _rx) = std::sync::mpsc::channel();
        super::spawn_pipe_server(tx);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

/// Crea (o actualiza) en el Escritorio los dos accesos directos del mecanismo `.lnk`:
/// "notty - nuevo temporal.lnk" y "notty - nuevo permanente.lnk". Sin tecla rápida
/// asignada por código (ver nota en `shortcut::create`): el usuario la pone a mano
/// desde "Propiedades" en el Explorador, con Ctrl+Alt+<letra>.
fn ensure_lnk_shortcuts() -> Result<()> {
    let Ok(exe) = std::env::current_exe() else { return Ok(()) };
    let Some(desktop) = std::env::var_os("USERPROFILE").map(std::path::PathBuf::from) else { return Ok(()) };
    let desktop = desktop.join("Desktop");
    let _ = crate::shortcut::create(&exe, "--new-temp", "notty: nuevo temporal", &desktop.join("notty - nuevo temporal.lnk"));
    let _ = crate::shortcut::create(&exe, "--new-permanent", "notty: nuevo permanente", &desktop.join("notty - nuevo permanente.lnk"));
    Ok(())
}

/// Convierte una cadena "Win+Alt+N" en (vk, modificadores de RegisterHotKey).
/// Reutiliza `notty_input::parse_key_spec` para la letra y añade el bit MOD_WIN
/// a mano, porque `Modifiers` de `notty-input` no distingue la tecla Windows
/// (no hace falta en el resto de la app, solo aquí).
fn hotkey_or_default(spec: &str) -> (u32, u32) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};
    let has_win = spec.to_lowercase().contains("win+");
    let rest = spec.replace("Win+", "").replace("win+", "");
    let (vk, m) = notty_input::parse_key_spec(&rest).unwrap_or((0x4E, notty_input::Modifiers::default()));
    let mut mods = 0u32;
    if has_win {
        mods |= MOD_WIN.0;
    }
    if m.ctrl {
        mods |= MOD_CONTROL.0;
    }
    if m.alt {
        mods |= MOD_ALT.0;
    }
    if m.shift {
        mods |= MOD_SHIFT.0;
    }
    (vk, mods)
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_HOTKEY => {
                let arg = match wparam.0 as i32 {
                    ID_HOTKEY_TEMP => Some("--new-temp"),
                    ID_HOTKEY_PERM => Some("--new-permanent"),
                    _ => None,
                };
                if let Some(arg) = arg {
                    let _ = std::process::Command::new(std::env::current_exe().unwrap_or_default()).arg(arg).spawn();
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
