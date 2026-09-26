//! Envoltura sobre `MsiInstallProductW` + `MsiSetExternalUIRecord` (msi.dll) para
//! conducir la instalación real desde la UI Direct2D de `notty-setup` (Task 6),
//! sin usar los diálogos propios de Windows Installer (`INSTALLUILEVEL_NONE`).
//!
//! Nota de implementación: la variante "Record" de `MsiSetExternalUIRecord` entrega
//! el mensaje como un `MSIHANDLE` (un *record* MSI), no como texto ya formateado
//! (`PCWSTR`) -- eso es lo que hace la variante más antigua `MsiSetExternalUI`. El
//! texto se obtiene con `MsiFormatRecordW` sobre ese handle.
use windows::Win32::System::ApplicationInstallationAndServicing::{
    INSTALLLOGMODE, INSTALLMESSAGE_ACTIONSTART, INSTALLMESSAGE_ERROR, INSTALLMESSAGE_PROGRESS,
    INSTALLUILEVEL_NONE, MSIHANDLE, MsiEnableLogW, MsiFormatRecordW, MsiInstallProductW,
    MsiRecordGetInteger, MsiSetExternalUIRecord, MsiSetInternalUI,
};
use windows::core::PCWSTR;
use std::path::Path;

#[derive(Debug, Clone)]
pub enum InstallEvent {
    Progress(u8),
    ActionText(String),
    Error(String),
    Done,
}

thread_local! {
    static CALLBACK: std::cell::RefCell<Option<Box<dyn FnMut(InstallEvent)>>> = std::cell::RefCell::new(None);
    // Estado de MsiMessage(INSTALLMESSAGE_PROGRESS) necesario para traducir sus campos
    // (que son deltas/tipo de paso, no un porcentaje) en un 0..=100 utilizable. Ver
    // "Progress messages sent by internal UI" en la documentación de MSI.
    static PROGRESS: std::cell::Cell<ProgressState> = std::cell::Cell::new(ProgressState::new());
}

#[derive(Clone, Copy)]
struct ProgressState {
    total: i32,
    current: i32,
    forward: bool,
}

impl ProgressState {
    const fn new() -> Self {
        Self { total: 0, current: 0, forward: true }
    }

    fn percent(&self) -> u8 {
        if self.total <= 0 {
            return 0;
        }
        let done = if self.forward { self.current } else { self.total - self.current };
        ((done.max(0) as i64 * 100 / self.total as i64).clamp(0, 100)) as u8
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn format_record(hrecord: MSIHANDLE) -> String {
    unsafe {
        let mut len: u32 = 0;
        // Primera llamada con buffer nulo: devuelve en `len` el tamaño (sin el nul)
        // necesario, tal y como documenta MsiFormatRecordW.
        let _ = MsiFormatRecordW(MSIHANDLE(0), hrecord, None, Some(&mut len));
        if len == 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let mut cap = buf.len() as u32;
        let result = MsiFormatRecordW(
            MSIHANDLE(0),
            hrecord,
            Some(windows::core::PWSTR(buf.as_mut_ptr())),
            Some(&mut cap),
        );
        if result != 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..cap as usize])
    }
}

pub fn install(
    msi_path: &Path,
    addlocal: &str,
    install_folder: &Path,
    on_event: impl FnMut(InstallEvent) + Send + 'static,
) -> Result<(), i32> {
    CALLBACK.with(|c| *c.borrow_mut() = Some(Box::new(on_event)));
    PROGRESS.with(|p| p.set(ProgressState::new()));

    let log_path = std::env::temp_dir().join("notty-install.log");
    let log_wide = to_wide(&log_path.display().to_string());

    unsafe {
        // Todos los tipos de mensaje (ver INSTALLLOGMODE_*): necesitamos progreso,
        // texto de acción y errores como mínimo.
        let _ = MsiEnableLogW(INSTALLLOGMODE(0x03FF_FFFF), PCWSTR(log_wide.as_ptr()), 0);

        MsiSetInternalUI(INSTALLUILEVEL_NONE, None);
        let ok = MsiSetExternalUIRecord(Some(external_ui_handler), 0x03FF_FFFF, None, None);
        if ok != 0 {
            return Err(ok as i32);
        }

        let props = format!(
            "ADDLOCAL={addlocal} INSTALLFOLDER=\"{}\"",
            install_folder.display()
        );
        let msi_wide = to_wide(&msi_path.display().to_string());
        let props_wide = to_wide(&props);

        let result = MsiInstallProductW(PCWSTR(msi_wide.as_ptr()), PCWSTR(props_wide.as_ptr()));
        if result != 0 {
            return Err(result as i32);
        }
    }
    CALLBACK.with(|c| {
        if let Some(cb) = c.borrow_mut().as_mut() {
            cb(InstallEvent::Done);
        }
    });
    Ok(())
}

unsafe extern "system" fn external_ui_handler(
    _context: *mut core::ffi::c_void,
    message_type: u32,
    hrecord: MSIHANDLE,
) -> i32 {
    // El byte alto de `message_type` es el INSTALLMESSAGE_*; el resto son banderas de
    // estilo de caja de diálogo, que no aplican aquí (INSTALLUILEVEL_NONE).
    let category = (message_type & 0xFF00_0000) as i32;

    let event = if category == INSTALLMESSAGE_PROGRESS.0 {
        // Campos del record de progreso (1-indexados): [1]=subtipo, [2..]=valores.
        // Subtipo 0: reset (campo 2=total de "ticks", campo 3=dirección: 0=adelante,
        // 1=atrás, campo 4=ignorar). Subtipo 1: increment por acción. Subtipo 2:
        // progreso real (campo 2=cantidad completada en esta llamada).
        let subtype = unsafe { MsiRecordGetInteger(hrecord, 1) };
        match subtype {
            0 => {
                let total = unsafe { MsiRecordGetInteger(hrecord, 2) };
                let forward = unsafe { MsiRecordGetInteger(hrecord, 3) } == 0;
                PROGRESS.with(|p| p.set(ProgressState { total, current: 0, forward }));
                None
            }
            1 | 2 => {
                let amount = unsafe { MsiRecordGetInteger(hrecord, 2) };
                let pct = PROGRESS.with(|p| {
                    let mut s = p.get();
                    s.current += amount;
                    p.set(s);
                    s.percent()
                });
                Some(InstallEvent::Progress(pct))
            }
            _ => None,
        }
    } else if category == INSTALLMESSAGE_ACTIONSTART.0 {
        let text = format_record(hrecord);
        if text.is_empty() { None } else { Some(InstallEvent::ActionText(text)) }
    } else if category == INSTALLMESSAGE_ERROR.0 {
        let text = format_record(hrecord);
        Some(InstallEvent::Error(text))
    } else {
        None
    };

    if let Some(event) = event {
        CALLBACK.with(|c| {
            if let Some(cb) = c.borrow_mut().as_mut() {
                cb(event);
            }
        });
    }
    1 // IDOK: dejar que MSI continúe
}
