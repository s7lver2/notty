//! Punto de entrada de notty: interpreta argv, carga la configuración y abre la ventana.

pub(crate) mod daemon;
pub(crate) mod shortcut;

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

fn main() -> windows::core::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--daemon") {
        return daemon::run();
    }

    // `--new-temp`/`--new-permanent`: los lanza el daemon (Task 9) al pulsar un atajo
    // global. Cada pulsación abre su propia ventana nueva sin pasar por el pipe: no
    // tiene sentido "reenviar a la instancia existente" cuando lo que se pide es
    // justo lo contrario, un documento nuevo. "--new-permanent" es, en la práctica,
    // el mismo camino que "notty sin argumentos" (documento vacío, ruta `None`): el
    // modo real (borrador/volátil) solo aplica a "--new-temp", que se decide con
    // `cfg.files.temp_mode`.
    let new_temp = args.get(1).map(String::as_str) == Some("--new-temp");
    let is_new_permanent = args.get(1).map(String::as_str) == Some("--new-permanent");

    let path = if new_temp || is_new_permanent { None } else { args.get(1).cloned() };
    if let Some(p) = &path {
        let msg = notty_ipc::Message::OpenPath(p.clone());
        if try_forward_to_existing_instance(&msg) {
            return Ok(());
        }
    }

    // Nadie escuchaba en el pipe: esta instancia se convierte en el servidor mientras
    // viva, además de abrir su propia ventana con normalidad.
    let (tx, rx) = std::sync::mpsc::channel();
    spawn_pipe_server(tx);

    let load = notty_config::load(&notty_config::default_path());
    if new_temp {
        let cfg = match &load {
            notty_config::LoadResult::Loaded(c) | notty_config::LoadResult::Missing(c) | notty_config::LoadResult::Defaulted(c, _) => c.clone(),
        };
        notty_ui::window::run_with_temp(load, Some(rx), cfg.files.temp_mode, cfg.files.default_extension.clone())
    } else {
        notty_ui::window::run_with_ipc(path.as_deref(), load, Some(rx))
    }
}
