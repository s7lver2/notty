//! Punto de entrada de notty: interpreta argv, carga la configuración y abre la ventana.
#![windows_subsystem = "windows"]

pub(crate) mod daemon;
pub(crate) mod shortcut;

use notty_update::{PUBKEY, REPO};

/// Intenta reenviar `msg` a una instancia de notty ya en marcha (ventana normal o
/// `--daemon`) a través del *named pipe* de instancia única. Devuelve `true` si había
/// alguien escuchando (y por tanto el mensaje se envió): en ese caso el proceso actual
/// no debe abrir ventana propia.
fn try_forward_to_existing_instance(msg: &notty_ipc::Message) -> bool {
    use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_GENERIC_WRITE, OPEN_EXISTING};
    use windows::core::PCWSTR;
    let wide: Vec<u16> = notty_ipc::PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let Ok(handle) = CreateFileW(PCWSTR(wide.as_ptr()), FILE_GENERIC_WRITE.0, Default::default(), None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None) else {
            return false;
        };
        let bytes = notty_ipc::encode(msg);
        let mut written = 0u32;
        let _ = windows::Win32::Storage::FileSystem::WriteFile(handle, Some(&bytes), Some(&mut written), None);
        let _ = windows::Win32::Foundation::CloseHandle(handle);
        true
    }
}

/// Lanza el hilo servidor del pipe de instancia única: escucha en bucle, decodifica
/// cada mensaje recibido y lo manda por `sender`. Usado tanto por la instancia normal
/// con ventana (Task 8) como por `notty --daemon` (Task 9).
pub(crate) fn spawn_pipe_server(sender: std::sync::mpsc::Sender<notty_ipc::Message>) {
    std::thread::spawn(move || loop {
        use windows::Win32::Storage::FileSystem::PIPE_ACCESS_INBOUND;
        use windows::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_TYPE_BYTE, PIPE_WAIT};
        let wide: Vec<u16> = notty_ipc::PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe {
            CreateNamedPipeW(
                windows::core::PCWSTR(wide.as_ptr()),
                PIPE_ACCESS_INBOUND,
                PIPE_TYPE_BYTE | PIPE_WAIT,
                windows::Win32::System::Pipes::PIPE_UNLIMITED_INSTANCES,
                0,
                4096,
                0,
                None,
            )
        };
        if handle == windows::Win32::Foundation::INVALID_HANDLE_VALUE || handle.is_invalid() {
            break;
        }
        if unsafe { ConnectNamedPipe(handle, None) }.is_err() {
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(handle) };
            continue;
        }
        let mut buf = [0u8; 4096];
        let mut read = 0u32;
        if unsafe { windows::Win32::Storage::FileSystem::ReadFile(handle, Some(&mut buf), Some(&mut read), None) }.is_ok() {
            if let Some(msg) = notty_ipc::decode(&buf[..read as usize]) {
                let _ = sender.send(msg);
            }
        }
        unsafe {
            let _ = DisconnectNamedPipe(handle);
            let _ = windows::Win32::Foundation::CloseHandle(handle);
        }
    });
}

/// Elimina un `argv[1]` que sea la ruta a `notepad.exe` (sin distinguir mayúsculas),
/// que es lo que IFEO antepone cuando invoca a notty como sustituto del Bloc de
/// notas real (`... Image File Execution Options\notepad.exe` con `Debugger` = notty).
/// Sin esto, notty intentaría abrir esa ruta como si fuera un documento.
pub(crate) fn strip_ifeo_arg(args: &mut Vec<String>) {
    if let Some(first) = args.first() {
        if first.to_lowercase().ends_with(r"\notepad.exe") {
            args.remove(0);
        }
    }
}

fn main() -> windows::core::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    strip_ifeo_arg(&mut args);
    notty_ui::set_app_version(env!("CARGO_PKG_VERSION"));

    if args.first().map(String::as_str) == Some("--daemon") {
        return daemon::run();
    }
    // Lo lanza el MSI al desinstalar (acción personalizada en installer/notty.wxs).
    if args.first().map(String::as_str) == Some("--forget-notepad") {
        notty_update::notepad::forget();
        return Ok(());
    }

    // `--new-temp`/`--new-permanent`: los lanza el daemon (Task 9) al pulsar un atajo
    // global. Cada pulsación abre su propia ventana nueva sin pasar por el pipe: no
    // tiene sentido "reenviar a la instancia existente" cuando lo que se pide es
    // justo lo contrario, un documento nuevo. "--new-permanent" es, en la práctica,
    // el mismo camino que "notty sin argumentos" (documento vacío, ruta `None`): el
    // modo real (borrador/volátil) solo aplica a "--new-temp", que se decide con
    // `cfg.files.temp_mode`.
    let new_temp = args.first().map(String::as_str) == Some("--new-temp");
    let is_new_permanent = args.first().map(String::as_str) == Some("--new-permanent");

    let path = if new_temp || is_new_permanent { None } else { args.first().cloned() };
    if let Some(p) = &path {
        let msg = notty_ipc::Message::OpenPath(p.clone());
        if try_forward_to_existing_instance(&msg) {
            return Ok(());
        }
    }

    // Cada usuario tiene su propio HKCU y el instalador solo corrió como uno de ellos.
    notty_update::notepad::sync();

    // Nadie escuchaba en el pipe: esta instancia se convierte en el servidor mientras
    // viva, además de abrir su propia ventana con normalidad.
    let (tx, rx) = std::sync::mpsc::channel();
    let update_tx = tx.clone();
    spawn_pipe_server(tx);

    let load = notty_config::load(&notty_config::default_path());
    let cfg_for_check = match &load {
        notty_config::LoadResult::Loaded(c) | notty_config::LoadResult::Missing(c) | notty_config::LoadResult::Defaulted(c, _) => c.clone(),
    };
    spawn_update_check(cfg_for_check, update_tx);

    if new_temp {
        let cfg = match &load {
            notty_config::LoadResult::Loaded(c) | notty_config::LoadResult::Missing(c) | notty_config::LoadResult::Defaulted(c, _) => c.clone(),
        };
        notty_ui::window::run_with_temp(load, Some(rx), cfg.files.temp_mode, cfg.files.default_extension.clone(), PUBKEY, REPO.to_string())
    } else {
        notty_ui::window::run_with_ipc(path.as_deref(), load, Some(rx), PUBKEY, REPO.to_string())
    }
}

#[cfg(test)]
mod ifeo_tests {
    use super::strip_ifeo_arg;

    #[test]
    fn strips_notepad_path_case_insensitive() {
        let mut args = vec![
            r"C:\Windows\System32\notepad.exe".to_string(),
            "C:\\file.txt".to_string(),
        ];
        strip_ifeo_arg(&mut args);
        assert_eq!(args, vec!["C:\\file.txt".to_string()]);
    }

    #[test]
    fn strips_uppercase_notepad_path() {
        let mut args = vec![r"C:\WINDOWS\SYSTEM32\NOTEPAD.EXE".to_string()];
        strip_ifeo_arg(&mut args);
        assert!(args.is_empty());
    }

    #[test]
    fn leaves_normal_args_untouched() {
        let mut args = vec!["C:\\file.txt".to_string()];
        strip_ifeo_arg(&mut args);
        assert_eq!(args, vec!["C:\\file.txt".to_string()]);
    }

    #[test]
    fn leaves_empty_args_untouched() {
        let mut args: Vec<String> = vec![];
        strip_ifeo_arg(&mut args);
        assert!(args.is_empty());
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Chequeo automático de actualizaciones al arrancar (Task 4, Step 1 del plan del
/// actualizador): sin red salvo que `cfg.updates.check` esté activo y ya toque
/// (`notty_update::due`). Falla en silencio (spec: "Silent failure only for the
/// automatic background check"): un error de red o un JSON roto simplemente deja
/// `last_check` puesto y no dice nada. Manda el resultado por el mismo canal
/// `mpsc` que ya existía para el pipe de instancia única, en vez de inventar uno
/// nuevo — `ipc_tick` en `notty-ui` lo recoge igual que un `OpenPath`.
fn spawn_update_check(cfg: notty_config::Config, tx: std::sync::mpsc::Sender<notty_ipc::Message>) {
    if !cfg.updates.check {
        return;
    }
    let now = unix_now();
    if !notty_update::due(cfg.updates.last_check, now) {
        return;
    }
    std::thread::spawn(move || {
        let ua = format!("notty/{}", env!("CARGO_PKG_VERSION"));
        if let Ok(release) = notty_update::http::latest_release(REPO, &ua) {
            if notty_update::is_newer(env!("CARGO_PKG_VERSION"), &release.version) {
                let msg = notty_ipc::Message::UpdateAvailable(notty_ipc::UpdateRelease {
                    tag: release.tag,
                    version: release.version,
                    body: release.body,
                    setup_url: release.setup_url,
                    sig_url: release.sig_url,
                });
                let _ = tx.send(msg);
            }
        }
        // Se persiste `last_check` pase lo que pase (éxito, sin red, JSON roto...):
        // así no se reintenta cada arranque si GitHub está caído.
        let mut fresh = match notty_config::load(&notty_config::default_path()) {
            notty_config::LoadResult::Loaded(c) | notty_config::LoadResult::Missing(c) | notty_config::LoadResult::Defaulted(c, _) => c,
        };
        fresh.updates.last_check = unix_now();
        let _ = notty_config::save(&fresh, &notty_config::default_path());
    });
}
