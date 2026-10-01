//! Punto de entrada de notty: interpreta argv, carga la configuración y abre la ventana.
#![windows_subsystem = "windows"]

pub(crate) mod daemon;

use notty_update::{PUBKEY, REPO};

/// Intenta reenviar `msg` a una instancia de notty ya en marcha (ventana normal o
/// `--daemon`) a través del *named pipe* de instancia única. Devuelve `true` si había
/// alguien escuchando (y por tanto el mensaje se envió): en ese caso el proceso actual
/// no debe abrir ventana propia.
fn try_forward_to_existing_instance(msg: &notty_ipc::Message) -> bool {
    use windows::Win32::Foundation::ERROR_PIPE_BUSY;
    use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_GENERIC_WRITE, OPEN_EXISTING};
    use windows::Win32::System::Pipes::WaitNamedPipeW;
    use windows::core::{HRESULT, PCWSTR};
    let wide: Vec<u16> = notty_ipc::PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    for _ in 0..5 {
        let opened = unsafe {
            CreateFileW(PCWSTR(wide.as_ptr()), FILE_GENERIC_WRITE.0, Default::default(), None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None)
        };
        match opened {
            Ok(handle) => {
                // Para que la ventana que lo recibe pueda ponerse delante.
                let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(u32::MAX) };
                let bytes = notty_ipc::encode(msg);
                let mut written = 0u32;
                let ok = unsafe { windows::Win32::Storage::FileSystem::WriteFile(handle, Some(&bytes), Some(&mut written), None) }.is_ok()
                    && written as usize == bytes.len();
                let _ = unsafe { windows::Win32::Foundation::CloseHandle(handle) };
                return ok;
            }
            // Todas las instancias del pipe ocupadas: se espera a una libre en vez de
            // abrir otra ventana.
            Err(e) if e.code() == HRESULT::from_win32(ERROR_PIPE_BUSY.0) => {
                if !unsafe { WaitNamedPipeW(PCWSTR(wide.as_ptr()), 2000) }.as_bool() {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
    false
}

/// La otra instancia tiene su propio directorio actual: una ruta relativa se resuelve
/// aquí, antes de mandarla.
fn absolute_path_arg(p: &str) -> String {
    std::path::absolute(p).map(|a| a.to_string_lossy().into_owned()).unwrap_or_else(|_| p.to_string())
}

/// Lanza el hilo servidor del pipe de instancia única: escucha en bucle, decodifica
/// cada mensaje recibido y lo manda por `sender`. Usado tanto por la instancia normal
/// con ventana (Task 8) como por `notty --daemon` (Task 9).
pub(crate) fn spawn_pipe_server(sender: std::sync::mpsc::Sender<notty_ipc::Message>) {
    use windows::Win32::Foundation::{ERROR_PIPE_CONNECTED, HANDLE};
    use windows::Win32::Storage::FileSystem::PIPE_ACCESS_INBOUND;
    use windows::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_TYPE_BYTE, PIPE_WAIT};
    fn create() -> Option<HANDLE> {
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
        (handle != windows::Win32::Foundation::INVALID_HANDLE_VALUE && !handle.is_invalid()).then_some(handle)
    }
    std::thread::spawn(move || {
        let Some(mut listening) = create() else { return };
        loop {
            let connected = match unsafe { ConnectNamedPipe(listening, None) } {
                Ok(()) => true,
                // El cliente llegó entre `CreateNamedPipeW` y `ConnectNamedPipe`: también vale.
                Err(e) => e.code() == windows::core::HRESULT::from_win32(ERROR_PIPE_CONNECTED.0),
            };
            let current = listening;
            // La siguiente instancia se crea antes de atender esta: así siempre hay una
            // esperando y un cliente nunca se encuentra sin pipe (abriría otra ventana).
            let next = create();
            if connected {
                let mut buf = [0u8; 4096];
                let mut read = 0u32;
                if unsafe { windows::Win32::Storage::FileSystem::ReadFile(current, Some(&mut buf), Some(&mut read), None) }.is_ok() {
                    if let Some(msg) = notty_ipc::decode(&buf[..read as usize]) {
                        let _ = sender.send(msg);
                        notty_ui::window::wake_for_ipc();
                    }
                }
            }
            unsafe {
                let _ = DisconnectNamedPipe(current);
                let _ = windows::Win32::Foundation::CloseHandle(current);
            }
            match next {
                Some(h) => listening = h,
                None => break,
            }
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
    notty_ui::bench_log::mark("main");
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
    let load = notty_config::load(&notty_config::default_path());
    notty_ui::bench_log::mark("config_loaded");
    let cfg_for_check = match &load {
        notty_config::LoadResult::Loaded(c) | notty_config::LoadResult::Missing(c) | notty_config::LoadResult::Defaulted(c, _) => c.clone(),
    };
    // Perfil de pruebas (`tools/shot-settings.ps1`): ni registro, ni pipe de instancia
    // única, ni atajo global, para no pisar el notty de verdad del usuario.
    let pruebas = std::env::var_os("NOTTY_PRUEBAS").is_some();
    if let Some(p) = &path {
        if cfg_for_check.files.open_in_existing_window && !pruebas {
            let msg = notty_ipc::Message::OpenPath(absolute_path_arg(p));
            if try_forward_to_existing_instance(&msg) {
                return Ok(());
            }
        }
    }

    // Cada usuario tiene su propio HKCU y el instalador solo corrió como uno de ellos.
    if !pruebas {
        notty_update::notepad::sync();
    }
    notty_ui::bench_log::mark("notepad_sync");

    // Esta instancia también escucha en el pipe mientras viva, además de abrir su
    // propia ventana con normalidad.
    let (tx, rx) = std::sync::mpsc::channel();
    if !pruebas {
        spawn_pipe_server(tx);
    }

    let hotkey = cfg_for_check.hotkey;
    std::thread::spawn(move || notty_ui::global_hotkey::sync(&hotkey));

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
    fn relative_paths_are_made_absolute_before_forwarding() {
        let p = super::absolute_path_arg("nuevo.txt");
        assert!(std::path::Path::new(&p).is_absolute());
        assert!(p.ends_with("nuevo.txt"));
        assert_eq!(super::absolute_path_arg(r"C:\a\b.txt"), r"C:\a\b.txt");
    }

    #[test]
    fn leaves_empty_args_untouched() {
        let mut args: Vec<String> = vec![];
        strip_ifeo_arg(&mut args);
        assert!(args.is_empty());
    }
}
