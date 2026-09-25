use std::path::Path;

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{Interface, Result};

/// Crea un acceso directo `.lnk` a `target_exe` con los argumentos dados. La tecla
/// rápida de un .lnk de Windows solo admite Ctrl+Alt+<letra>, limitación del propio
/// sistema (no de notty): por eso este mecanismo usa combinaciones distintas a las
/// del daemon (ver Ajustes, Task 10). `IShellLinkW` de esta versión de `windows-rs`
/// no expone un `SetHotkey`, así que el acceso directo se crea sin tecla rápida
/// asignada por código; el usuario puede ponérsela a mano desde "Propiedades" del
/// acceso directo en el Explorador, que es la única vía 100% fiable para esto.
pub fn create(target_exe: &Path, args: &str, description: &str, lnk_path: &Path) -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let target_wide: Vec<u16> = target_exe.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
        link.SetPath(windows::core::PCWSTR(target_wide.as_ptr()))?;
        let args_wide: Vec<u16> = args.encode_utf16().chain(std::iter::once(0)).collect();
        link.SetArguments(windows::core::PCWSTR(args_wide.as_ptr()))?;
        let desc_wide: Vec<u16> = description.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = link.SetDescription(windows::core::PCWSTR(desc_wide.as_ptr()));

        let persist: windows::Win32::System::Com::IPersistFile = link.cast()?;
        let path_wide: Vec<u16> = lnk_path.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
        persist.Save(windows::core::PCWSTR(path_wide.as_ptr()), true)?;
    }
    Ok(())
}
