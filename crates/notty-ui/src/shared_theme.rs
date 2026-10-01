//! Tema compartido con essentials (`%APPDATA%\essentials\appearance.toml`, ver
//! `notty_config::read_appearance`): si existe con `shared = true` y `[ui]
//! follow_essentials` está activo, su tema y su acento mandan sobre los de notty.
//! Lo que hay en el archivo vive aquí (un proceso, una copia); `effective` es lo que
//! se pinta y la ventana lo lee donde antes leía `cfg.ui.theme`/`cfg.ui.accent`.

use std::sync::{Mutex, MutexGuard};

use notty_config::{AccentColor, Theme, UiConfig};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadDirectoryChangesW,
};
use windows::core::HSTRING;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shared {
    /// El archivo existe (aunque no se entienda): enseña la fila de Ajustes.
    exists: bool,
    /// Sus valores, si se entiende y `shared = true`.
    values: Option<(Theme, AccentColor)>,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared { exists: false, values: None });

fn lock() -> MutexGuard<'static, Shared> {
    SHARED.lock().unwrap_or_else(|e| e.into_inner())
}

/// Relee el archivo. `true` si cambió algo (que exista o sus valores).
pub(crate) fn reload() -> bool {
    let new = match notty_config::appearance_path() {
        Some(p) => Shared { exists: p.exists(), values: notty_config::read_appearance(&p) },
        None => Shared { exists: false, values: None },
    };
    let mut s = lock();
    let changed = *s != new;
    *s = new;
    changed
}

/// Si existe `appearance.toml` (la fila «Seguir el tema de essentials» solo sale entonces).
pub(crate) fn file_exists() -> bool {
    lock().exists
}

/// Tema y acento que se pintan: los de essentials si se siguen y están, si no los de `ui`.
pub(crate) fn effective(ui: &UiConfig) -> (Theme, AccentColor) {
    match lock().values {
        Some(v) if ui.follow_essentials => v,
        _ => (ui.theme, ui.accent),
    }
}

/// El usuario eligió tema o acento en notty: si se sigue a essentials, ese componente
/// se escribe en `appearance.toml` (así cambia en todas sus apps).
pub(crate) fn push(ui: &UiConfig, theme: Option<Theme>, accent: Option<AccentColor>) {
    if !ui.follow_essentials {
        return;
    }
    let mut s = lock();
    let Some((t0, a0)) = s.values else { return };
    let new = (theme.unwrap_or(t0), accent.unwrap_or(a0));
    let Some(path) = notty_config::appearance_path() else { return };
    if notty_config::write_appearance(&path, new.0, new.1).is_ok() {
        s.values = Some(new);
    }
}

/// Vigila `%APPDATA%\essentials\` en un hilo (`ReadDirectoryChangesW`) y llama a
/// `on_change` tras cada cambio; quien lo recibe relee con `reload`. Solo si la carpeta
/// existe al arrancar (sin essentials no hay nada que vigilar).
pub(crate) fn watch(on_change: fn()) {
    let Some(dir) = notty_config::appearance_path().and_then(|p| p.parent().map(|d| d.to_path_buf())) else { return };
    if !dir.is_dir() {
        return;
    }
    std::thread::spawn(move || unsafe {
        let Ok(h) = CreateFileW(
            &HSTRING::from(dir.as_os_str()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        ) else {
            return;
        };
        let mut buf = [0u32; 1024];
        loop {
            let mut got = 0u32;
            if ReadDirectoryChangesW(
                h,
                buf.as_mut_ptr().cast(),
                std::mem::size_of_val(&buf) as u32,
                false,
                FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME,
                Some(&mut got),
                None,
                None,
            )
            .is_err()
            {
                break;
            }
            // Un guardado son varios avisos seguidos (temporal, rename…): se deja acabar.
            std::thread::sleep(std::time::Duration::from_millis(60));
            on_change();
        }
        let _ = CloseHandle(h);
    });
}
