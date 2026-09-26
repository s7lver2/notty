use std::os::windows::ffi::OsStrExt;
use std::ffi::OsStr;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::Debug::{DebugActiveProcessStop, DebugSetProcessKillOnExit};
use windows::Win32::System::Threading::{
    CreateProcessW, DEBUG_ONLY_THIS_PROCESS, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows::core::PWSTR;

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn launch_real_notepad(args: &[String]) -> windows::core::Result<()> {
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let exe_path = format!("{system_root}\\System32\\notepad.exe");

    let mut cmdline = format!("\"{exe_path}\"");
    for a in args {
        cmdline.push(' ');
        cmdline.push('"');
        cmdline.push_str(&a.replace('"', "\\\""));
        cmdline.push('"');
    }
    let mut cmdline_wide = wide(&cmdline);

    let mut si = STARTUPINFOW::default();
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut pi = PROCESS_INFORMATION::default();

    unsafe {
        CreateProcessW(
            None,
            Some(PWSTR(cmdline_wide.as_mut_ptr())),
            None,
            None,
            false,
            DEBUG_ONLY_THIS_PROCESS,
            None,
            None,
            &si,
            &mut pi,
        )?;

        // Detach the debugger immediately so notepad.exe runs free, unaffected by IFEO
        // (Windows never applies IFEO to a process it is already debugging).
        DebugSetProcessKillOnExit(false)?;
        DebugActiveProcessStop(pi.dwProcessId)?;

        CloseHandle(pi.hProcess).ok();
        CloseHandle(pi.hThread).ok();
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = launch_real_notepad(&args) {
        eprintln!("notepad_legacy: fallo al lanzar notepad.exe real: {e:?}");
        // Fallback: Store Notepad, documented as losing file args.
        let _ = std::process::Command::new("explorer.exe")
            .arg("shell:AppsFolder\\Microsoft.WindowsNotepad_8wekyb3d8bbwe!App")
            .spawn();
    }
}
