//! Diálogo nativo de Windows para elegir/crear un archivo (`^O` en la línea de ruta),
//! como alternativa a escribir la ruta a mano. `IFileOpenDialog`/`IFileSaveDialog` vía COM.

use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, CLSCTX_INPROC_SERVER, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::Shell::{FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, SIGDN_FILESYSPATH};

use crate::Purpose;

/// Muestra el diálogo nativo correspondiente (Abrir/Guardar como). `None` si el
/// usuario cancela, si COM falla al crearlo, o si la ruta resultante no es UTF-8.
pub fn pick_path(hwnd: HWND, purpose: Purpose) -> Option<PathBuf> {
    unsafe {
        // CoInitializeEx puede llamarse más de una vez en el mismo hilo (se lleva la
        // cuenta por referencias): cada llamada aquí se empareja con su CoUninitialize.
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = match purpose {
            Purpose::Open => pick_open(hwnd),
            Purpose::Save => pick_save(hwnd),
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

unsafe fn pick_save(hwnd: HWND) -> Option<PathBuf> {
    unsafe {
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        dialog.Show(Some(hwnd)).ok()?;
        let item = dialog.GetResult().ok()?;
        let wide = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        wide.to_string().ok().map(PathBuf::from)
    }
}
