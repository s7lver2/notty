//! Deja el sistema como pide Ajustes → Atajo global (`HotkeyConfig`):
//!
//! - **Segundo plano**: `notty --daemon` corriendo (lo arranca si no está) y, con
//!   "Iniciar con Windows", en `HKCU\...\Run` para que vuelva tras reiniciar.
//! - **Acceso directo**: sin daemon (se cierra si estaba) y dos `.lnk` en el menú
//!   Inicio con la tecla rápida ya puesta (Ctrl+Alt+N y Ctrl+Alt+Shift+N). Windows
//!   solo atiende teclas rápidas de accesos directos del Escritorio o del menú Inicio.
//!
//! `sync` se llama al arrancar y cada vez que se guardan los Ajustes; solo hace algo
//! si la configuración cambió desde la última vez en este proceso.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use notty_config::{HotkeyConfig, HotkeyMechanism};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW};
use windows::Win32::UI::Shell::{IShellLinkW, SHCNE_CREATE, SHCNE_DELETE, SHCNE_ID, SHCNF_PATHW, SHChangeNotify, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
use windows::core::{HSTRING, Interface, PCWSTR, w};

/// Clase de la ventana oculta del daemon (`notty/src/daemon.rs`).
pub const DAEMON_CLASS: PCWSTR = w!("NottyDaemonClass");

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const RUN_VALUE: PCWSTR = w!("notty");

/// Teclas rápidas de los `.lnk`: VK en el byte bajo, `HOTKEYF_*` en el alto.
const LNK_TEMP: u16 = 0x4E | ((0x2 | 0x4) << 8); // Ctrl+Alt+N
const LNK_PERM: u16 = 0x4E | ((0x1 | 0x2 | 0x4) << 8); // Ctrl+Alt+Shift+N

static LAST: Mutex<Option<HotkeyConfig>> = Mutex::new(None);

/// Si el daemon de atajos está corriendo (en este usuario).
pub fn daemon_running() -> bool {
    unsafe { FindWindowW(DAEMON_CLASS, PCWSTR::null()).is_ok_and(|h| !h.is_invalid()) }
}

pub fn sync(cfg: &HotkeyConfig) {
    {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if last.as_ref() == Some(cfg) {
            return;
        }
        *last = Some(cfg.clone());
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let daemon = cfg.mechanism == HotkeyMechanism::Daemon;
    set_autostart(&exe, daemon && cfg.start_with_windows);
    if daemon {
        remove_lnks();
        if !daemon_running() {
            let _ = std::process::Command::new(&exe).arg("--daemon").spawn();
        }
    } else {
        stop_daemon();
        create_lnks(&exe);
    }
}

fn set_autostart(exe: &Path, on: bool) {
    unsafe {
        if on {
            let cmd: Vec<u16> = format!("\"{}\" --daemon", exe.display()).encode_utf16().chain(Some(0)).collect();
            let _ = RegSetKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE, REG_SZ.0, Some(cmd.as_ptr().cast()), (cmd.len() * 2) as u32);
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE);
        }
    }
}

fn stop_daemon() {
    unsafe {
        if let Ok(h) = FindWindowW(DAEMON_CLASS, PCWSTR::null()) {
            let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

/// `%APPDATA%\Microsoft\Windows\Start Menu\Programs\notty`.
fn lnk_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join(r"Microsoft\Windows\Start Menu\Programs\notty"))
}

const LNKS: [(&str, &str, &str, u16); 2] = [
    ("notty - nuevo temporal.lnk", "--new-temp", "notty: nuevo temporal", LNK_TEMP),
    ("notty - nuevo permanente.lnk", "--new-permanent", "notty: nuevo permanente", LNK_PERM),
];

fn create_lnks(exe: &Path) {
    let Some(dir) = lnk_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    for (name, args, desc, key) in LNKS {
        let path = dir.join(name);
        if create_lnk(exe, args, desc, key, &path).is_ok() {
            notify_shell(SHCNE_CREATE, &path);
        }
    }
}

fn remove_lnks() {
    let Some(dir) = lnk_dir() else { return };
    for (name, ..) in LNKS {
        let path = dir.join(name);
        if std::fs::remove_file(&path).is_ok() {
            notify_shell(SHCNE_DELETE, &path);
        }
    }
    let _ = std::fs::remove_dir(&dir); // solo si quedó vacía
}

/// Explorer solo registra (o suelta) la tecla rápida de un `.lnk` cuando se entera de
/// que apareció o desapareció; sin este aviso el atajo no funciona hasta reiniciar.
fn notify_shell(event: SHCNE_ID, path: &Path) {
    let wide = HSTRING::from(path.as_os_str());
    unsafe { SHChangeNotify(event, SHCNF_PATHW, Some(wide.as_ptr().cast()), None) };
}

fn create_lnk(exe: &Path, args: &str, desc: &str, hotkey: u16, lnk: &Path) -> windows::core::Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(exe.as_os_str()))?;
        link.SetArguments(&HSTRING::from(args))?;
        let _ = link.SetDescription(&HSTRING::from(desc));
        link.SetHotkey(hotkey)?;
        link.cast::<IPersistFile>()?.Save(&HSTRING::from(lnk.as_os_str()), true)
    }
}
