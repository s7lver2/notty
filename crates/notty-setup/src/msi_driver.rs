//! Envoltura sobre `MsiInstallProductW` + `MsiSetExternalUIRecord` (msi.dll) para
//! conducir la instalación real desde la UI Direct2D de `notty-setup`, sin usar los
//! diálogos propios de Windows Installer (`INSTALLUILEVEL_NONE`).
//!
//! La variante "Record" entrega cada mensaje como un `MSIHANDLE` (un *record* MSI),
//! no como texto ya formateado: el texto sale de `MsiFormatRecordW`/`MsiRecordGetStringW`.
use windows::Win32::System::ApplicationInstallationAndServicing::{
    INSTALLLOGMODE, INSTALLMESSAGE_ACTIONSTART, INSTALLMESSAGE_ERROR, INSTALLMESSAGE_PROGRESS, INSTALLUILEVEL,
    INSTALLUILEVEL_NONE, INSTALLUILEVEL_UACONLY,
    MSIHANDLE, MsiEnableLogW, MsiFormatRecordW, MsiInstallProductW, MsiRecordGetInteger, MsiRecordGetStringW,
    MsiSetExternalUIRecord, MsiSetInternalUI,
};
use windows::core::PCWSTR;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum InstallEvent {
    Progress(u8),
    ActionText(String),
    /// Texto de un error de MSI. No termina la instalación por sí solo: el código final
    /// llega aparte, cuando `install` devuelve `Err`.
    Error(String),
    Done,
}

thread_local! {
    static CALLBACK: std::cell::RefCell<Option<Box<dyn FnMut(InstallEvent)>>> = std::cell::RefCell::new(None);
    static PROGRESS: std::cell::Cell<ProgressState> = const { std::cell::Cell::new(ProgressState::new()) };
}

/// MSI manda dos tandas de progreso: la generación del script (rápida) y su ejecución
/// (la de verdad). Cada una reinicia sus contadores, así que se reparten el 0..100 de
/// la barra en vez de hacerla retroceder: 0..20 el script, 20..100 la ejecución.
#[derive(Clone, Copy)]
struct ProgressState {
    total: i32,
    current: i32,
    forward: bool,
    scripting: bool,
    shown: u8,
}

impl ProgressState {
    const fn new() -> Self {
        Self { total: 0, current: 0, forward: true, scripting: true, shown: 0 }
    }

    fn percent(&self) -> u8 {
        if self.total <= 0 {
            return self.shown;
        }
        let done = if self.forward { self.current } else { self.total - self.current };
        let frac = (done.max(0) as f64 / self.total as f64).clamp(0.0, 1.0);
        let (lo, hi) = if self.scripting { (0.0, 20.0) } else { (20.0, 100.0) };
        ((lo + (hi - lo) * frac) as u8).max(self.shown)
    }
}

pub fn log_path() -> PathBuf {
    std::env::temp_dir().join("notty-install.log")
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn format_record(hrecord: MSIHANDLE) -> String {
    unsafe {
        let mut len: u32 = 0;
        let _ = MsiFormatRecordW(MSIHANDLE(0), hrecord, None, Some(&mut len));
        if len == 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let mut cap = buf.len() as u32;
        if MsiFormatRecordW(MSIHANDLE(0), hrecord, Some(windows::core::PWSTR(buf.as_mut_ptr())), Some(&mut cap)) != 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..cap as usize])
    }
}

fn record_string(hrecord: MSIHANDLE, field: u32) -> String {
    unsafe {
        let mut buf = vec![0u16; 256];
        let mut cap = buf.len() as u32;
        if MsiRecordGetStringW(hrecord, field, Some(windows::core::PWSTR(buf.as_mut_ptr())), Some(&mut cap)) != 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..cap as usize])
    }
}

/// Traduce el nombre interno de una acción estándar de MSI a lo que se enseña bajo la
/// barra de progreso. Las acciones que no aparecen aquí no cambian el texto.
pub fn action_label(action: &str) -> Option<&'static str> {
    Some(match action {
        "InstallValidate" | "InstallInitialize" | "CostInitialize" | "FileCost" | "CostFinalize"
        | "LaunchConditions" | "FindRelatedProducts" | "AppSearch" | "MigrateFeatureStates" | "ValidateProductID" => {
            "Preparando la instalación"
        }
        "RemoveExistingProducts" => "Quitando la versión anterior",
        "InstallFiles" | "RemoveFiles" | "MoveFiles" | "DuplicateFiles" => "Copiando archivos",
        "WriteRegistryValues" | "RemoveRegistryValues" => "Escribiendo en el registro",
        "RegisterClassInfo" | "RegisterExtensionInfo" | "RegisterProgIdInfo" | "RegisterMIMEInfo"
        | "UnregisterClassInfo" | "UnregisterExtensionInfo" | "UnregisterProgIdInfo" | "UnregisterMIMEInfo" => {
            "Registrando asociaciones de archivo"
        }
        "CreateShortcuts" | "RemoveShortcuts" => "Creando accesos directos",
        "WriteEnvironmentStrings" | "RemoveEnvironmentStrings" => "Actualizando el PATH",
        "RegisterUser" | "RegisterProduct" | "PublishFeatures" | "PublishProduct" | "UnpublishFeatures" => {
            "Registrando notty en Windows"
        }
        "InstallFinalize" => "Terminando",
        _ => return None,
    })
}

pub fn install(
    msi_path: &Path,
    addlocal: &str,
    install_folder: &Path,
    on_event: impl FnMut(InstallEvent) + Send + 'static,
) -> Result<(), i32> {
    CALLBACK.with(|c| *c.borrow_mut() = Some(Box::new(on_event)));
    PROGRESS.with(|p| p.set(ProgressState::new()));

    let log_wide = to_wide(&log_path().display().to_string());

    unsafe {
        let _ = MsiEnableLogW(INSTALLLOGMODE(0x03FF_FFFF), PCWSTR(log_wide.as_ptr()), 0);

        // Sin interfaz de MSI (la pone notty-setup) salvo el aviso de UAC: con
        // `INSTALLUILEVEL_NONE` a secas, MSI no puede pedir elevación y una instalación
        // para todo el equipo falla con 1603 por falta de permisos.
        MsiSetInternalUI(INSTALLUILEVEL(INSTALLUILEVEL_NONE.0 | INSTALLUILEVEL_UACONLY.0), None);
        let ok = MsiSetExternalUIRecord(Some(external_ui_handler), 0x03FF_FFFF, None, None);
        if ok != 0 {
            return Err(ok as i32);
        }

        let props = format!("ADDLOCAL={addlocal} INSTALLFOLDER=\"{}\"", install_folder.display());
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

fn emit(event: InstallEvent) {
    CALLBACK.with(|c| {
        if let Some(cb) = c.borrow_mut().as_mut() {
            cb(event);
        }
    });
}

unsafe extern "system" fn external_ui_handler(_context: *mut core::ffi::c_void, message_type: u32, hrecord: MSIHANDLE) -> i32 {
    let category = (message_type & 0xFF00_0000) as i32;

    if category == INSTALLMESSAGE_PROGRESS.0 {
        // [1] subtipo. 0 = reinicio ([2] total, [3] dirección, [4] 1 = generando el
        // script); 2 = avance ([2] ticks hechos). El 1 solo configura ActionData.
        let subtype = unsafe { MsiRecordGetInteger(hrecord, 1) };
        match subtype {
            0 => {
                let total = unsafe { MsiRecordGetInteger(hrecord, 2) };
                let forward = unsafe { MsiRecordGetInteger(hrecord, 3) } == 0;
                let scripting = unsafe { MsiRecordGetInteger(hrecord, 4) } == 1;
                PROGRESS.with(|p| {
                    let shown = p.get().shown;
                    p.set(ProgressState { total, current: 0, forward, scripting, shown });
                });
            }
            2 => {
                let amount = unsafe { MsiRecordGetInteger(hrecord, 2) };
                let pct = PROGRESS.with(|p| {
                    let mut s = p.get();
                    s.current += amount;
                    s.shown = s.percent();
                    p.set(s);
                    s.shown
                });
                emit(InstallEvent::Progress(pct));
            }
            _ => {}
        }
    } else if category == INSTALLMESSAGE_ACTIONSTART.0 {
        if let Some(label) = action_label(&record_string(hrecord, 1)) {
            emit(InstallEvent::ActionText(label.to_string()));
        }
    } else if category == INSTALLMESSAGE_ERROR.0 {
        let text = format_record(hrecord);
        if !text.is_empty() {
            emit(InstallEvent::Error(text));
        }
    }
    1 // IDOK: dejar que MSI continúe
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_splits_script_and_execution() {
        let script = ProgressState { total: 100, current: 50, forward: true, scripting: true, shown: 0 };
        assert_eq!(script.percent(), 10);
        let exec = ProgressState { total: 100, current: 50, forward: true, scripting: false, shown: 20 };
        assert_eq!(exec.percent(), 60);
    }

    #[test]
    fn progress_never_goes_back() {
        let s = ProgressState { total: 100, current: 0, forward: true, scripting: false, shown: 35 };
        assert_eq!(s.percent(), 35);
    }

    #[test]
    fn known_actions_are_translated() {
        assert_eq!(action_label("InstallFiles"), Some("Copiando archivos"));
        assert_eq!(action_label("RegisterExtensionInfo"), Some("Registrando asociaciones de archivo"));
        assert_eq!(action_label("SomethingCustom"), None);
    }
}
