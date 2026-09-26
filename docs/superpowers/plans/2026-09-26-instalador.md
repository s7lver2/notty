# Instalador (notepad_legacy, MSI, notty-setup) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `notepad_legacy.exe`, the WiX MSI, and the animated Direct2D `notty-setup.exe` that drives the MSI and can also run in `--update --relaunch` mode.

**Architecture:** `notty-setup` is a new binary crate under `crates/notty-setup/`, embedding `notty.msi` via `include_bytes!` and driving `msi.dll` (`MsiSetExternalUIRecord` + `MsiInstallProduct`) from raw `windows` crate bindings. It reuses `notty-ui`'s `Renderer`, `Anim`/`ease_out_cubic`, `theme::palette`, and the same chrome/wndproc pattern as `crates/notty-ui/src/settings_window.rs`. `notepad_legacy` is a separate tiny binary crate `crates/notty-legacy/`. MSI authoring lives in `installer/notty.wxs` (WiX v5), built only at release time by `tools/release.ps1` (plan 2), not by `cargo build`.

**Tech Stack:** Rust, `windows` crate 0.62.2 (add `Win32_System_Msi`, `Win32_System_Diagnostics_Debug`, `Win32_UI_Shell` features as needed), WiX v5 CLI (external tool, not a Cargo dependency), existing `notty-ui`/`notty-config` crates.

## Global Constraints

- No network access anywhere in this plan (spec principle: "sin red sin permiso").
- Never modify or rename `System32\notepad.exe`. All redirection is registry-only (IFEO), reversible by uninstall.
- Read `SPI_GETCLIENTAREAANIMATION` once at startup via the existing `notty_ui::window::system_animations_enabled()` (re-export if private) and feed it into every `Anim::new_maybe`.
- Visual fidelity is mandatory: colors, spacing, copy, and motion durations must match `docs/mockups/setup/instalador-flujo.html` and `docs/mockups/setup/instalador-opciones.html` exactly (open both in a browser and compare side-by-side against the running `notty-setup.exe`). Durations: step transition out 140ms/in 180ms/12px, expander 260ms with 30ms stagger, progress bar interpolates without jumps, checkmark stroke 600ms.
- Single source of truth for copy: Spanish strings as written in the mockups, verbatim.

---

### Task 1: `notepad_legacy` proof of concept (standalone, throwaway)

**Files:**
- Create: `crates/notty-legacy/Cargo.toml`
- Create: `crates/notty-legacy/src/main.rs`

**Interfaces:**
- Produces: a working, manually-verified technique for launching the real `System32\notepad.exe` while bypassing IFEO. Later tasks depend on this working before building anything else on top.

- [ ] **Step 1: Scaffold the crate**

`crates/notty-legacy/Cargo.toml`:
```toml
[package]
name = "notty-legacy"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "notepad_legacy"
path = "src/main.rs"

[dependencies]
windows = { version = "0.62.2", features = [
    "Win32_Foundation",
    "Win32_System_Threading",
    "Win32_System_Diagnostics_Debug",
    "Win32_UI_Shell",
] }
```

Add `"crates/*"` already covers this (workspace members glob in root `Cargo.toml` — no change needed there).

- [ ] **Step 2: Implement the debug-launch bypass**

`crates/notty-legacy/src/main.rs`:
```rust
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
```

- [ ] **Step 3: Manual verification (no automated test possible — this bypasses OS-level redirection)**

Run:
```powershell
cargo build -p notty-legacy
```
Then, **temporarily** set an IFEO debugger on `notepad.exe` pointing at something obviously wrong (e.g. `cmd.exe /c echo REDIRECTED & pause`) as admin:
```powershell
New-Item -Path "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\notepad.exe" -Force
Set-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\notepad.exe" -Name Debugger -Value "cmd.exe /c echo REDIRECTED & pause"
```
Confirm redirection works: run `notepad.exe` from a terminal → see "REDIRECTED".
Then run `target\debug\notepad_legacy.exe` directly → **expected: the real Notepad window opens**, not "REDIRECTED". This proves `DEBUG_ONLY_THIS_PROCESS` bypasses IFEO.
Also test `notepad_legacy.exe C:\some\file.txt` → real Notepad opens with that file loaded.
Clean up the registry key immediately after:
```powershell
Remove-Item -Path "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\notepad.exe" -Force
```

**If this PoC fails** (e.g. `DebugActiveProcessStop` doesn't detach fully, or notepad.exe still gets redirected), STOP and report back — do not proceed to Task 2, since the whole IFEO approach depends on this working. If it fails, fall back straight to the Store-Notepad AUMID launch documented in Step 2 and note file-arg loss as a known limitation in the installer options screen.

- [ ] **Step 4: Commit**

```bash
git add crates/notty-legacy
git commit -m "feat(installer): notepad_legacy bypasses IFEO via debug-launch"
```

---

### Task 2: `strip_ifeo_arg` in notty's entry point

**Files:**
- Modify: `crates/notty/src/main.rs`
- Test: `crates/notty/src/main.rs` (inline `#[cfg(test)]` module — `main.rs` already parses CLI args here per the survey; add the function next to that logic)

**Interfaces:**
- Produces: `pub(crate) fn strip_ifeo_arg(args: &mut Vec<String>)` — removes an `argv[1]` that is a path ending in `\notepad.exe` (case-insensitive), which IFEO prepends when it invokes notty as the redirect target.

- [ ] **Step 1: Write the failing test**

Add to `crates/notty/src/main.rs`:
```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p notty ifeo_tests`
Expected: FAIL with "cannot find function `strip_ifeo_arg`"

- [ ] **Step 3: Implement**

Add near the top-level arg handling in `crates/notty/src/main.rs`:
```rust
fn strip_ifeo_arg(args: &mut Vec<String>) {
    if let Some(first) = args.first() {
        if first.to_lowercase().ends_with(r"\notepad.exe") {
            args.remove(0);
        }
    }
}
```
Call it immediately after collecting `std::env::args().skip(1).collect()`, before any other flag parsing (`--daemon`, `--new-temp`, etc.), so an IFEO-launched notty sees the same args as a directly-launched one.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p notty ifeo_tests`
Expected: PASS (4 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/notty/src/main.rs
git commit -m "feat(installer): strip IFEO-injected notepad.exe path from argv"
```

---

### Task 3: MSI feature model and `ADDLOCAL` translation (pure logic)

**Files:**
- Create: `crates/notty-setup/Cargo.toml`
- Create: `crates/notty-setup/src/features.rs`
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `pub struct FeatureToggles { pub notepad_replace: bool, pub context_menu: bool, pub path_env: bool, pub assoc_text: bool, pub assoc_config: bool, pub assoc_markdown: bool, pub assoc_code: bool, pub start_menu: bool, pub desktop: bool }`, `impl Default for FeatureToggles` (all `true` except `assoc_code` and `desktop`, per spec table), and `pub fn to_addlocal(t: &FeatureToggles) -> String` returning a comma-joined `ADDLOCAL=` value including the mandatory `Core` feature.

- [ ] **Step 1: Scaffold the `notty-setup` crate**

`crates/notty-setup/Cargo.toml`:
```toml
[package]
name = "notty-setup"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "notty-setup"
path = "src/main.rs"

[dependencies]
notty-ui = { path = "../notty-ui" }
notty-config = { path = "../notty-config" }
windows = { version = "0.62.2", features = [
    "Win32_Foundation",
    "Win32_System_Msi",
    "Win32_UI_WindowsAndMessaging",
    "Win32_Graphics_Gdi",
    "Win32_Graphics_Dwm",
    "Win32_System_LibraryLoader",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_Graphics_Direct2D",
    "Win32_Graphics_Direct2D_Common",
    "Win32_Graphics_DirectWrite",
    "Win32_Graphics_Dxgi_Common",
    "Win32_UI_HiDpi",
    "Win32_UI_Shell",
] }
windows-numerics = { version = "0.3.1", features = ["std"] }
```

- [ ] **Step 2: Write the failing tests**

`crates/notty-setup/src/features.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureToggles {
    pub notepad_replace: bool,
    pub context_menu: bool,
    pub path_env: bool,
    pub assoc_text: bool,
    pub assoc_config: bool,
    pub assoc_markdown: bool,
    pub assoc_code: bool,
    pub start_menu: bool,
    pub desktop: bool,
}

impl Default for FeatureToggles {
    fn default() -> Self {
        Self {
            notepad_replace: true,
            context_menu: true,
            path_env: true,
            assoc_text: true,
            assoc_config: true,
            assoc_markdown: true,
            assoc_code: false,
            start_menu: true,
            desktop: false,
        }
    }
}

pub fn to_addlocal(t: &FeatureToggles) -> String {
    let mut features = vec!["Core"];
    if t.notepad_replace { features.push("NotepadReplace"); }
    if t.context_menu { features.push("ContextMenu"); }
    if t.path_env { features.push("PathEnv"); }
    if t.assoc_text { features.push("AssocText"); }
    if t.assoc_config { features.push("AssocConfig"); }
    if t.assoc_markdown { features.push("AssocMarkdown"); }
    if t.assoc_code { features.push("AssocCode"); }
    if t.start_menu { features.push("StartMenu"); }
    if t.desktop { features.push("Desktop"); }
    features.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec_table() {
        let t = FeatureToggles::default();
        assert!(t.notepad_replace && t.context_menu && t.path_env);
        assert!(t.assoc_text && t.assoc_config && t.assoc_markdown);
        assert!(!t.assoc_code && !t.desktop);
        assert!(t.start_menu);
    }

    #[test]
    fn addlocal_all_default() {
        let t = FeatureToggles::default();
        assert_eq!(
            to_addlocal(&t),
            "Core,NotepadReplace,ContextMenu,PathEnv,AssocText,AssocConfig,AssocMarkdown,StartMenu"
        );
    }

    #[test]
    fn addlocal_minimal() {
        let t = FeatureToggles {
            notepad_replace: false, context_menu: false, path_env: false,
            assoc_text: false, assoc_config: false, assoc_markdown: false,
            assoc_code: false, start_menu: false, desktop: false,
        };
        assert_eq!(to_addlocal(&t), "Core");
    }

    #[test]
    fn addlocal_everything() {
        let t = FeatureToggles {
            notepad_replace: true, context_menu: true, path_env: true,
            assoc_text: true, assoc_config: true, assoc_markdown: true,
            assoc_code: true, start_menu: true, desktop: true,
        };
        assert_eq!(
            to_addlocal(&t),
            "Core,NotepadReplace,ContextMenu,PathEnv,AssocText,AssocConfig,AssocMarkdown,AssocCode,StartMenu,Desktop"
        );
    }
}
```

- [ ] **Step 3: Run tests to verify they fail, then pass**

Run: `cargo test -p notty-setup features::tests`
Expected before implementation exists in a lib target: this file has no lib — add a minimal `src/lib.rs`:
```rust
pub mod features;
```
and add `path = "src/main.rs"` bin plus this lib to `Cargo.toml` (Cargo auto-detects `src/lib.rs` alongside `src/main.rs`, no manual `[lib]` section needed).
Run again: `cargo test -p notty-setup features::tests` → PASS (4 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/notty-setup
git commit -m "feat(installer): feature toggle model and ADDLOCAL translation"
```

---

### Task 4: WiX MSI authoring

**Files:**
- Create: `installer/notty.wxs`
- Create: `installer/License.rtf` (minimal placeholder-free short MIT/whatever-license text the project already uses — check root `LICENSE` file first and reuse its text verbatim, converted to RTF)

**Interfaces:**
- Produces: a buildable WiX v5 source producing `notty.msi` with the Feature table from Task 3 (`Core`, `NotepadReplace`, `ContextMenu`, `PathEnv`, `AssocText`, `AssocConfig`, `AssocMarkdown`, `AssocCode`, `StartMenu`, `Desktop`) and the `INSTALLFOLDER` property, consumed by Task 5's setup binary and by `tools/release.ps1` (plan 2).

- [ ] **Step 1: Check the project's actual license text**

Run: `Get-Content E:\notty\LICENSE -TotalCount 20` (PowerShell) to get the real license header — do not invent one. Use its exact name/year in `installer/License.rtf`.

- [ ] **Step 2: Author `installer/notty.wxs`**

```xml
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Package Name="notty" Manufacturer="notty" Version="!(bind.FileVersion.notty.exe)"
           UpgradeCode="6F1B7C2E-6E2F-4E2A-9B1F-2C3D4E5F6A7B" Scope="perMachine">

    <MajorUpgrade DowngradeErrorMessage="Ya hay una versión más reciente de notty instalada." />
    <MediaTemplate EmbedCab="yes" />
    <Property Id="ARPPRODUCTICON" Value="notty.exe" />
    <Property Id="INSTALLFOLDER" Secure="yes" />

    <StandardDirectory Id="ProgramFilesFolder">
      <Directory Id="INSTALLFOLDER" Name="notty">
        <Component Id="CoreFiles" Guid="*">
          <File Id="notty.exe" Source="$(var.NottyExePath)" KeyPath="yes" />
          <File Id="notepad_legacy.exe" Source="$(var.NottyLegacyExePath)" />
        </Component>

        <Component Id="NotepadReplaceReg" Guid="*">
          <RegistryValue Root="HKLM"
            Key="SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\notepad.exe"
            Name="Debugger" Type="string" Value="[INSTALLFOLDER]notty.exe" KeyPath="yes" />
        </Component>

        <Component Id="ContextMenuReg" Guid="*">
          <RegistryValue Root="HKLM" Key="SOFTWARE\Classes\*\shell\notty"
            Value="Abrir con notty" Type="string" KeyPath="yes" />
          <RegistryValue Root="HKLM" Key="SOFTWARE\Classes\*\shell\notty\command"
            Value="&quot;[INSTALLFOLDER]notty.exe&quot; &quot;%1&quot;" Type="string" />
        </Component>

        <Component Id="PathEnvReg" Guid="*">
          <Environment Id="PathAddition" Name="PATH" Value="[INSTALLFOLDER]" Permanent="no"
            Part="last" Action="set" System="yes" />
        </Component>
      </Directory>
    </StandardDirectory>

    <StandardDirectory Id="ProgramMenuFolder">
      <Component Id="StartMenuShortcut" Guid="*">
        <Shortcut Id="StartMenuNotty" Name="notty" Target="[INSTALLFOLDER]notty.exe"
          WorkingDirectory="INSTALLFOLDER" />
        <RemoveFolder Id="RemoveStartMenuDir" On="uninstall" />
        <RegistryValue Root="HKCU" Key="Software\notty" Name="startmenu" Type="integer" Value="1" KeyPath="yes" />
      </Component>
    </StandardDirectory>

    <StandardDirectory Id="DesktopFolder">
      <Component Id="DesktopShortcut" Guid="*">
        <Shortcut Id="DesktopNotty" Name="notty" Target="[INSTALLFOLDER]notty.exe"
          WorkingDirectory="INSTALLFOLDER" />
        <RegistryValue Root="HKCU" Key="Software\notty" Name="desktop" Type="integer" Value="1" KeyPath="yes" />
      </Component>
    </StandardDirectory>

    <Feature Id="Core" Title="notty (núcleo)" Level="1" AllowAbsent="no" Display="hidden">
      <ComponentRef Id="CoreFiles" />
    </Feature>
    <Feature Id="NotepadReplace" Title="Sustituir el Bloc de notas" Level="1">
      <ComponentRef Id="NotepadReplaceReg" />
    </Feature>
    <Feature Id="ContextMenu" Title="Menú contextual" Level="1">
      <ComponentRef Id="ContextMenuReg" />
    </Feature>
    <Feature Id="PathEnv" Title="PATH" Level="1">
      <ComponentRef Id="PathEnvReg" />
    </Feature>
    <!-- AssocText / AssocConfig / AssocMarkdown / AssocCode: same ProgID+OpenWithProgids+Capabilities
         pattern as ContextMenuReg, one Component per association group, omitted here for brevity but
         REQUIRED before this task is done — see Step 3. -->
    <Feature Id="StartMenu" Title="Menú Inicio" Level="1">
      <ComponentRef Id="StartMenuShortcut" />
    </Feature>
    <Feature Id="Desktop" Title="Escritorio" Level="1000">
      <ComponentRef Id="DesktopShortcut" />
    </Feature>

    <UIRef Id="WixUI_Common" />
  </Package>
</Wix>
```

- [ ] **Step 3: Add the four association Features (AssocText/AssocConfig/AssocMarkdown/AssocCode)**

For each group, add a `Component` under `INSTALLFOLDER` following this exact pattern (shown for `AssocText`; repeat for `AssocConfig` with extensions `.ini .cfg .conf .toml`, `AssocMarkdown` with `.md .markdown`, `AssocCode` with `.json .xml .csv .yaml`):
```xml
<Component Id="AssocTextReg" Guid="*">
  <ProgId Id="notty.txt" Description="Documento de texto (notty)">
    <Extension Id="txt" ContentType="text/plain">
      <Verb Id="open" Command="Abrir" TargetFile="notty.exe" Argument="&quot;%1&quot;" />
    </Extension>
    <Extension Id="log" />
    <Extension Id="text" />
  </ProgId>
  <RegistryValue Root="HKLM" Key="SOFTWARE\RegisteredApplications" Name="notty"
    Value="SOFTWARE\notty\Capabilities" Type="string" KeyPath="yes" />
  <RegistryValue Root="HKLM" Key="SOFTWARE\notty\Capabilities\FileAssociations"
    Name=".txt" Value="notty.txt" Type="string" />
</Component>
```
Add matching `<Feature Id="AssocText" .../>` etc. referencing each new component.

- [ ] **Step 4: Verify the MSI builds standalone**

WiX needs real built exe paths — for this task, build against the debug binaries as a smoke test:
```powershell
cargo build -p notty -p notty-legacy
wix build installer\notty.wxs -d NottyExePath=target\debug\notty.exe -d NottyLegacyExePath=target\debug\notepad_legacy.exe -o target\notty.msi
```
Expected: `target\notty.msi` is produced with no WiX errors. If `wix` CLI is not installed, install it first: `dotnet tool install --global wix` (requires .NET SDK — if unavailable, note this as an environment prerequisite and stop, reporting back).

- [ ] **Step 5: Commit**

```bash
git add installer/
git commit -m "feat(installer): author WiX v5 MSI with per-feature toggles"
```

---

### Task 5: `notty-setup` external-UI driver (headless logic first)

**Files:**
- Create: `crates/notty-setup/src/msi_driver.rs`
- Test: same file

**Interfaces:**
- Consumes: `FeatureToggles`/`to_addlocal` from Task 3.
- Produces: `pub enum InstallEvent { Progress(u8), ActionText(String), Error(String), Done }`, `pub fn install(msi_path: &std::path::Path, addlocal: &str, install_folder: &std::path::Path, on_event: impl FnMut(InstallEvent) + Send + 'static) -> Result<(), i32>` wrapping `MsiSetExternalUIRecord` + `MsiInstallProduct`, returning `Err(msi_error_code)` on failure (1602/1618 get special handling by the caller in Task 6).

- [ ] **Step 1: Implement the MSI driver**

```rust
use windows::Win32::System::Msi::{
    MsiInstallProductW, MsiSetExternalUIRecord, INSTALLLOGMODE, INSTALLMESSAGE,
    INSTALLUILEVEL_NONE, MsiSetInternalUI,
};
use windows::core::{PCWSTR, w};
use std::sync::Mutex;
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
}

pub fn install(
    msi_path: &Path,
    addlocal: &str,
    install_folder: &Path,
    on_event: impl FnMut(InstallEvent) + 'static,
) -> Result<(), i32> {
    CALLBACK.with(|c| *c.borrow_mut() = Some(Box::new(on_event)));

    unsafe {
        MsiSetInternalUI(INSTALLUILEVEL_NONE, None);
        MsiSetExternalUIRecord(
            Some(external_ui_handler),
            INSTALLLOGMODE(0x03FF_FFFF), // all message types; MSI docs define the exact mask
            None,
            None,
        )
        .ok()
        .map_err(|e| e.code().0)?;

        let props = format!(
            "ADDLOCAL={addlocal} INSTALLFOLDER=\"{}\"",
            install_folder.display()
        );
        let msi_wide = to_wide(&msi_path.display().to_string());
        let props_wide = to_wide(&props);

        let result = MsiInstallProductW(PCWSTR(msi_wide.as_ptr()), PCWSTR(props_wide.as_ptr()));
        if result.0 != 0 {
            return Err(result.0 as i32);
        }
    }
    CALLBACK.with(|c| {
        if let Some(cb) = c.borrow_mut().as_mut() {
            cb(InstallEvent::Done);
        }
    });
    Ok(())
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn external_ui_handler(
    _context: *mut core::ffi::c_void,
    message_type: u32,
    message: PCWSTR,
) -> i32 {
    let text = unsafe { message.to_string().unwrap_or_default() };
    let event = match message_type & 0xFF {
        // INSTALLMESSAGE_PROGRESS = 0x0A00 handling simplified: real progress parsing
        // reads the ProgressField record layout per MSI docs; ActionText is 0x0900,
        // Error is 0x0100. Exact bit values verified against Win32 Msi.h during implementation.
        _ => InstallEvent::ActionText(text),
    };
    CALLBACK.with(|c| {
        if let Some(cb) = c.borrow_mut().as_mut() {
            cb(event);
        }
    });
    1 // IDOK: let MSI continue
}
```

- [ ] **Step 2: Manual verification**

This cannot be unit-tested (real MSI engine, needs admin). Verify manually once Task 6's UI exists (see Task 6 Step 5). For now, confirm it compiles:
```powershell
cargo build -p notty-setup
```
Expected: builds with no errors (message-type bit values may need adjusting against `Msi.h` constants — `INSTALLMESSAGE_PROGRESS = 0x0A000000`, `INSTALLMESSAGE_ACTIONSTART = 0x0800000`, `INSTALLMESSAGE_ERROR = 0x01000000`; use `message_type & 0xFF000000` to switch on these, not `0xFF` — fix this during implementation and confirm against real installs in Task 6).

- [ ] **Step 3: Commit**

```bash
git add crates/notty-setup/src/msi_driver.rs
git commit -m "feat(installer): MSI external UI driver (MsiSetExternalUIRecord)"
```

---

### Task 6: `notty-setup` Direct2D UI — window, steps, options, progress, done

**Files:**
- Create: `crates/notty-setup/src/main.rs`
- Create: `crates/notty-setup/src/ui.rs`
- Create: `crates/notty-setup/src/main.rs` wiring `msi_driver` + `features` + `ui`

**Interfaces:**
- Consumes: `notty_ui::{Renderer, Anim, ease_out_cubic, palette, is_dark}` (per survey, all re-exported at crate root), `notty_ui::window::system_animations_enabled` (make `pub` if currently `pub(crate)`), `FeatureToggles`/`to_addlocal` (Task 3), `msi_driver::{install, InstallEvent}` (Task 5).
- Produces: the running `notty-setup.exe` binary.

This task has no meaningful unit-testable logic beyond what Tasks 3/5 already cover — it is UI wiring. Work directly from the two mockups; do not improvise layout, spacing, or copy.

- [ ] **Step 1: Make `system_animations_enabled` reusable**

In `crates/notty-ui/src/window.rs`, change:
```rust
pub(crate) fn system_animations_enabled() -> bool {
```
to:
```rust
pub fn system_animations_enabled() -> bool {
```
Run `cargo build -p notty-ui -p notty` to confirm nothing breaks (it's additive — widening visibility never breaks existing callers).

- [ ] **Step 2: Window shell**

In `crates/notty-setup/src/main.rs`, follow `crates/notty-ui/src/settings_window.rs`'s exact pattern (own `WNDCLASSEXW`, `wndproc`, chrome setup via the same `DwmExtendFrameIntoClientArea`/dark-mode techniques, `ID_ANIM_TIMER`) to create a **fixed-size, non-resizable** window with a 32px custom title bar, sized and laid out to match `docs/mockups/setup/instalador-flujo.html` (open it in a browser and read the `.win`/layout CSS for exact dimensions — do not guess pixel values, read them from the mockup file).

- [ ] **Step 3: Step rail, options screen, live preview panel**

Implement in `crates/notty-setup/src/ui.rs`:
- A left step rail with 4 steps (Bienvenida · Opciones · Instalar · Listo) and a progress line that fills as the user advances — read exact copy, colors, and the fill behavior from `docs/mockups/setup/instalador-flujo.html`.
- The Opciones screen's collapsible groups: height animates over 260ms (use `Anim::new_maybe(start, Duration::from_millis(260), animations_enabled)`), chevron rotates, header shows a live "N de M" summary, rows stagger in 30ms apart, toggles styled like Windows 11 — all exact values and copy from `docs/mockups/setup/instalador-opciones.html`.
- The live preview panel on the right switches scenes (terminal typing `notepad notas → notty`, context menu, association list, Start menu, folder) with a cross-fade + scale transition on hover, per the same mockup — read the exact scene list and transition timing from the HTML/CSS/JS in that file.
- Step transitions: outgoing content exits 12px left with a 140ms fade, incoming enters from the right over 180ms ease-out. The footer (Atrás/Siguiente buttons) never moves.
- Map the Opciones toggles directly onto `FeatureToggles` fields (Task 3); "Opciones futuras" is out of scope for this task (spec says it needs a new Feature + new row when it happens, not a placeholder now).

- [ ] **Step 4: Install screen wired to the MSI driver**

On entering "Instalar": extract the embedded MSI (`include_bytes!(concat!(env!("OUT_DIR"), "/notty.msi"))` — see Step 6 for how it gets there) to `%TEMP%\notty-install\notty.msi`, call `msi_driver::install` with `to_addlocal(&toggles)` and the chosen `INSTALLFOLDER`, on a background thread, and post `InstallEvent`s back to the UI thread via `PostMessageW` with a custom `WM_APP` message carrying a boxed event pointer (same technique `notty-ui` uses for cross-thread UI updates if one exists — check `daemon.rs`/IPC handling for a precedent; if none exists, use `PostMessageW(hwnd, WM_APP + 1, ptr as WPARAM, 0)` and reconstruct the `Box` in `wndproc`).
- The 3px progress bar interpolates the percentage without jumps (tween between last-known and new value using `Anim`).
- ActionText updates cross-fade in the same slot.
- On MSI error 1602 (UAC cancelled): silently return to the Opciones screen, no error shown.
- On MSI error 1618 (another install in progress): show "Espera a que termine la otra instalación" with a Reintentar button.
- On any other nonzero result: show the MSI error code and a link to `%TEMP%\notty-install.log` (enable via `MsiEnableLog` before calling `install`).

- [ ] **Step 5: Listo screen**

Animated checkmark stroke over 600ms (`Anim` + custom D2D geometry stroke — reuse whatever path-drawing helper `notty-ui`'s `Renderer` already exposes for strokes; if none exists, draw a simple two-segment check with `ID2D1PathGeometry` and reveal it by clipping to `Anim::value(...)` fraction of total path length), the "Abrir Aplicaciones predeterminadas" link (launches `ms-settings:defaultapps` via `ShellExecuteW`), and the primary "Abrir notty" button which launches the freshly installed `notty.exe` **unelevated** (setup itself never elevated, so this is a normal `CreateProcessW`/`ShellExecuteW` call) and exits.

- [ ] **Step 6: Embed the MSI**

Add a `build.rs` to `crates/notty-setup/`:
```rust
fn main() {
    // notty.msi is produced by tools/release.ps1 (plan 2) into installer/notty.msi
    // before notty-setup is built for release. For local dev builds where it doesn't
    // exist yet, write an empty placeholder so `cargo build` doesn't fail — notty-setup
    // run locally without a real MSI will simply fail at MsiInstallProduct with a clear error.
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = std::path::Path::new(&out_dir).join("notty.msi");
    let src = std::path::Path::new("../../installer/notty.msi");
    if src.exists() {
        std::fs::copy(src, &dest).unwrap();
    } else if !dest.exists() {
        std::fs::write(&dest, []).unwrap();
    }
    println!("cargo:rerun-if-changed=../../installer/notty.msi");
}
```
And in `main.rs`: `static MSI_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/notty.msi"));`

- [ ] **Step 7: Manual verification checklist**

Run `cargo build -p notty-setup` then build the real MSI first (Task 4 Step 4) and copy it to `installer/notty.msi`, rebuild `notty-setup`, then run it **as a real end-to-end test**:
- Clean install with all options → verify `notepad`, `Win+R notepad`, `notepad archivo.txt`, and `notepad_legacy` all behave as documented in the spec's manual test list.
- Clean install with the minimum (uncheck everything possible).
- Context menu → "Mostrar más opciones" → "Abrir con notty".
- Cancel UAC during install → confirm silent return to Opciones.
- Uninstall via "Aplicaciones instaladas" → confirm registry keys are gone (`Get-ItemProperty "HKLM:\...\Image File Execution Options\notepad.exe"` should error/not exist).
- Toggle reduced motion in Windows Settings → confirm all setup animations become instant cuts.

Compare every screen pixel-for-scheme against `docs/mockups/setup/instalador-flujo.html` and `instalador-opciones.html` open side-by-side in a browser tab. Do not consider this task done until they match.

- [ ] **Step 8: Commit**

```bash
git add crates/notty-setup crates/notty-ui/src/window.rs
git commit -m "feat(installer): notty-setup Direct2D UI driving the MSI install"
```

---

### Task 7: `--update --relaunch` mode

**Files:**
- Modify: `crates/notty-setup/src/main.rs`

**Interfaces:**
- Consumes: same `msi_driver::install` (MSI's `MajorUpgrade` handles the actual upgrade logic; this task only changes what the UI shows and does on completion).
- Produces: the update-mode entry point notty (plan 2) will invoke as `notty-setup.exe --update --relaunch`.

- [ ] **Step 1: Add the CLI branch**

At the top of `main()`, before creating the normal wizard window:
```rust
let args: Vec<String> = std::env::args().skip(1).collect();
if args.iter().any(|a| a == "--update") {
    let relaunch = args.iter().any(|a| a == "--relaunch");
    run_update_screen(relaunch);
    return;
}
```

- [ ] **Step 2: Implement the single update screen**

`run_update_screen`: one screen only, no step rail — shows `"<old> → <new> · firma verificada ✓"` (versions read from `%TEMP%\notty-update\` metadata written by the updater in plan 2 — for this task, accept them as `--from <ver> --to <ver>` CLI args passed by the caller, since notty-update doesn't exist yet), the release notes body, and "Más tarde" / "Actualizar" (with UAC shield icon) buttons. "Actualizar" runs the same `msi_driver::install` path as the wizard (MSI's own `MajorUpgrade` handles removing the old version).

- [ ] **Step 3: Relaunch on completion**

If `--relaunch` was passed and install succeeds, launch `notty.exe` unelevated (same technique as Task 6 Step 5) and exit. notty (plan 2) is responsible for having saved its session before invoking setup.

- [ ] **Step 4: Manual verification**

```powershell
target\debug\notty-setup.exe --update --relaunch --from 1.2.0 --to 1.3.0
```
Expected: single screen shown, matches the spec's one-line description; "Actualizar" runs the MSI and relaunches notty.

- [ ] **Step 5: Commit**

```bash
git add crates/notty-setup/src/main.rs
git commit -m "feat(installer): --update --relaunch mode for notty-setup"
```

---

## Manual checklist (full, run once at the end of this plan)

- [ ] Instalación limpia con todas las opciones
- [ ] Instalación limpia con el mínimo
- [ ] `notepad`, Win+R `notepad`, `notepad archivo.txt`, `notepad_legacy` (all four)
- [ ] Menú contextual y "Abrir con"
- [ ] UAC cancelado → vuelve a Opciones sin error
- [ ] Desinstalación deja el registro limpio
- [ ] Movimiento reducido → todo instantáneo
