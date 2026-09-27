//! Diálogo nativo de Windows para elegir/crear un archivo (`^O` en la línea de ruta),
//! como alternativa a escribir la ruta a mano. `IFileOpenDialog`/`IFileSaveDialog` vía COM.

use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, CLSCTX_INPROC_SERVER, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FileOpenDialog, FileSaveDialog, IFileOpenDialog,
    IFileSaveDialog, IShellItem, KNOWN_FOLDER_FLAG, SHCreateItemFromParsingName, SHGetKnownFolderPath,
    SIGDN_FILESYSPATH,
};

use crate::Purpose;

/// Carpetas conocidas que "Guardar por defecto en" (Ajustes → Archivos) ofrece como
/// atajo, resueltas de verdad (`SHGetKnownFolderPath`) en vez de adivinar el nombre de
/// la carpeta en el idioma de Windows del usuario (que no siempre es "Desktop").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownFolder {
    Desktop,
    Documents,
    Downloads,
}

impl KnownFolder {
    fn id(self) -> windows::core::GUID {
        match self {
            KnownFolder::Desktop => FOLDERID_Desktop,
            KnownFolder::Documents => FOLDERID_Documents,
            KnownFolder::Downloads => FOLDERID_Downloads,
        }
    }
}

pub fn known_folder_path(folder: KnownFolder) -> Option<PathBuf> {
    unsafe {
        let wide = SHGetKnownFolderPath(&folder.id(), KNOWN_FOLDER_FLAG(0), None).ok()?;
        let path = wide.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(wide.0 as *const core::ffi::c_void));
        path
    }
}

/// Muestra el diálogo nativo correspondiente (Abrir/Guardar como). `None` si el
/// usuario cancela, si COM falla al crearlo, o si la ruta resultante no es UTF-8.
/// `initial_dir` (solo se usa al guardar) es la carpeta configurada en Ajustes →
/// Archivos → "Guardar por defecto en", si hay una.
pub fn pick_path(hwnd: HWND, purpose: Purpose, initial_dir: Option<&std::path::Path>) -> Option<PathBuf> {
    unsafe {
        // CoInitializeEx puede llamarse más de una vez en el mismo hilo (se lleva la
        // cuenta por referencias): cada llamada aquí se empareja con su CoUninitialize.
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = match purpose {
            Purpose::Open => pick_open(hwnd),
            Purpose::Save => pick_save(hwnd, initial_dir),
        };
        if init.is_ok() {
            CoUninitialize();
        }
        result
    }
}

unsafe fn pick_open(hwnd: HWND) -> Option<PathBuf> {
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        dialog.Show(Some(hwnd)).ok()?;
        let item = dialog.GetResult().ok()?;
        let wide = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        wide.to_string().ok().map(PathBuf::from)
    }
}

unsafe fn pick_save(hwnd: HWND, initial_dir: Option<&std::path::Path>) -> Option<PathBuf> {
    unsafe {
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        if let Some(dir) = initial_dir {
            let wide = to_wide(&dir.display().to_string());
            if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(windows::core::PCWSTR(wide.as_ptr()), None) {
                let _ = dialog.SetFolder(&item);
            }
        }
        dialog.Show(Some(hwnd)).ok()?;
        let item = dialog.GetResult().ok()?;
        let wide = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        wide.to_string().ok().map(PathBuf::from)
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
