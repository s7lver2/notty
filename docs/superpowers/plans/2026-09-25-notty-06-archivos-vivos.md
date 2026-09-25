# notty · Plan 6: Archivos vivos Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Temporales de verdad (borrador en disco / volátil en memoria), un atajo global (segundo plano o `.lnk`) que crea uno nuevo aunque notty esté cerrado, instancia única (abrir un archivo con notty ya abierto lo manda a la ventana existente), autoguardado, conflicto al guardar si otro programa tocó el archivo, y recuperación de buffers sin guardar tras una caída. Cierra también las secciones "Archivos" y "Atajo global" de Ajustes, y conecta "modo vim siempre" (Plan 5) al crear documentos.

**Architecture:** Todo lo que toca disco de forma nueva (rutas de borradores/recuperación, marcas de tiempo para detectar cambios externos) vive en `notty-io`, testeado con `tempdir` como el resto del crate. El protocolo de instancia única es un mensaje de una sola línea (la ruta a abrir) sobre un *named pipe*; su parte de formato es pura y testeable, y solo la E/S del pipe en sí toca Win32. El daemon (`notty --daemon`) es un binario minúsculo sin ventana de documento: icono de bandeja + atajos globales (`RegisterHotKey`) que reenvían por el mismo pipe. La recuperación tras caída usa `std::panic::set_hook` para volcar los buffers sin guardar antes de que el proceso termine.

**Tech Stack:** Rust stable 1.96 (MSVC), `notty-core`/`notty-io`/`notty-ui`/`notty-config`/`notty-input` (Planes 1-5), `windows` (ya en el workspace: se añaden funcionalidades de *named pipes*, bandeja del sistema, `RegisterHotKey` y `IShellLinkW`).

**Spec:** `docs/superpowers/specs/2026-09-24-notty-design.md`
**Planes anteriores:** Plan 1 (`2026-09-24-notty-01-nucleo.md`), Plan 2 (`2026-09-25-notty-02-ventana-editor.md`), Plan 3 (`2026-09-25-notty-03-config-ventana.md`), Plan 4 (`2026-09-25-notty-04-ruta-busqueda.md`), Plan 5 (`2026-09-25-notty-05-vim-raw.md`)

## Global Constraints

- Solo Windows 10/11; toolchain `stable-x86_64-pc-windows-msvc`.
- Todo lo nuevo de `notty-io` (rutas de borradores/recuperación, mtime) se testea con `tempfile::tempdir()`, sin Win32.
- **Borrador:** vive en `%LOCALAPPDATA%\notty\drafts\`; si el usuario nunca le da una ruta propia con "Guardar como" antes de cerrarlo, se borra del disco al cerrar. **Volátil:** nunca toca el disco; se descarta sin preguntar al cerrar.
- **Autoguardado** solo se activa para documentos con ruta real (ni `None`/CLICKME ni temporal) y solo si `config.files.autosave` es `true`.
- **Conflicto al guardar:** nunca se sobrescribe en silencio un archivo que cambió en disco desde que se abrió. Se pregunta "el mío" o "el del disco"; el autoguardado se pausa mientras haya conflicto sin resolver.
- **Recuperación:** nunca se pierde un buffer sin guardar por una caída; los volcados de recuperación de documentos volátiles se borran en cuanto se recuperan (o se descartan).
- Textos visibles para el usuario en español.
- Un commit por tarea terminada.
- **Sobre el código Win32 de este plan** (Tasks 8-11: *named pipes*, daemon con bandeja, `RegisterHotKey`, acceso directo `.lnk` vía `IShellLinkW`, `panic hook` con E/S síncrona): se da la arquitectura y las llamadas por su nombre. Compílalo, y si una firma no coincide con la versión de `windows` instalada, ajústala consultando `cargo doc -p windows --open` o el error del compilador, sin cambiar el comportamiento descrito. Los pasos marcados como "lógica pura" son código exacto.

## File Structure

```
crates/notty-config/src/model.rs        (modificado) FilesConfig, HotkeyConfig, TempMode, HotkeyMechanism
crates/notty-io/src/drafts.rs           rutas de borradores + nombre único por fecha (nuevo)
crates/notty-io/src/recovery.rs         volcar/listar/borrar recuperación tras caída (nuevo)
crates/notty-io/src/watch.rs            mtime de un archivo, para detectar cambios externos (nuevo)
crates/notty-ipc/Cargo.toml             protocolo del pipe de instancia única (nuevo crate, puro)
crates/notty-ipc/src/lib.rs             encode_open_message / decode_open_message
crates/notty-ui/src/editor.rs           (modificado) TempMode, open_mtime, temporales
crates/notty-ui/src/window.rs           (modificado) instancia única (cliente), conflicto, autoguardado, recuperación
crates/notty-ui/src/settings_window.rs  (modificado) secciones Archivos y Atajo global
crates/notty/src/main.rs                (modificado) --daemon, instancia única (servidor del pipe)
crates/notty/src/daemon.rs              bandeja + RegisterHotKey + reenvío por el pipe (nuevo)
crates/notty/src/shortcut.rs            crear el .lnk con atajo (nuevo)
```

---

### Task 1: notty-config — `FilesConfig` y `HotkeyConfig` (lógica pura)

**Files:**
- Modify: `crates/notty-config/src/model.rs`

**Interfaces:**
- Consumes: nada.
- Produces:
  - `pub enum TempMode { Draft, Volatile }` (serde `rename_all = "lowercase"` con los valores `"borrador"`/`"volatil"` vía `#[serde(rename = "borrador")]`/`#[serde(rename = "volatil")]` explícitos porque no coinciden con el nombre de la variante en inglés; `Default` = `Draft`).
  - `pub enum HotkeyMechanism { Daemon, Lnk }` (serde `rename_all = "lowercase"`, `Default` = `Daemon`).
  - `pub struct FilesConfig { pub temp_mode: TempMode, pub autosave: bool, pub default_extension: String, pub large_file_mb: u32 }` (`Default`: `temp_mode=Draft, autosave=false, default_extension=".txt".to_string(), large_file_mb=50`).
  - `pub struct HotkeyConfig { pub mechanism: HotkeyMechanism, pub start_with_windows: bool }` (`Default`: `mechanism=Daemon, start_with_windows=true`).
  - `Config` gana los campos `pub files: FilesConfig` y `pub hotkey: HotkeyConfig` (ambos `#[serde(default)]`, igual que `ui`).

- [x] **Step 1: Escribir los tests que fallan**

Añadir a `mod tests` en `crates/notty-config/src/model.rs`:

```rust
    #[test]
    fn files_config_has_sensible_defaults() {
        let f = FilesConfig::default();
        assert_eq!(f.temp_mode, TempMode::Draft);
        assert!(!f.autosave);
        assert_eq!(f.default_extension, ".txt");
        assert_eq!(f.large_file_mb, 50);
    }

    #[test]
    fn hotkey_config_defaults_to_daemon_and_autostart() {
        let h = HotkeyConfig::default();
        assert_eq!(h.mechanism, HotkeyMechanism::Daemon);
        assert!(h.start_with_windows);
    }

    #[test]
    fn temp_mode_serializes_in_spanish() {
        assert_eq!(toml::to_string(&TempMode::Draft).unwrap().trim(), "\"borrador\"");
        assert_eq!(toml::to_string(&TempMode::Volatile).unwrap().trim(), "\"volatil\"");
    }

    #[test]
    fn config_round_trips_with_files_and_hotkey() {
        let mut cfg = Config::default();
        cfg.files.autosave = true;
        cfg.hotkey.mechanism = HotkeyMechanism::Lnk;
        let text = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert!(back.files.autosave);
        assert_eq!(back.hotkey.mechanism, HotkeyMechanism::Lnk);
    }

    #[test]
    fn partial_toml_still_gets_files_and_hotkey_defaults() {
        let cfg: Config = toml::from_str("[ui]\npreset = \"zen\"\n").unwrap();
        assert_eq!(cfg.files, FilesConfig::default());
        assert_eq!(cfg.hotkey, HotkeyConfig::default());
    }
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-config model`
Expected: FAIL de compilación, `cannot find type FilesConfig`.

- [x] **Step 3: Implementar**

Añadir a `crates/notty-config/src/model.rs`, junto a los `enum`/`struct` ya existentes:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TempMode {
    #[default]
    #[serde(rename = "borrador")]
    Draft,
    #[serde(rename = "volatil")]
    Volatile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HotkeyMechanism {
    #[default]
    Daemon,
    Lnk,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilesConfig {
    pub temp_mode: TempMode,
    pub autosave: bool,
    pub default_extension: String,
    pub large_file_mb: u32,
}

impl Default for FilesConfig {
    fn default() -> Self {
        Self { temp_mode: TempMode::default(), autosave: false, default_extension: ".txt".to_string(), large_file_mb: 50 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    pub mechanism: HotkeyMechanism,
    pub start_with_windows: bool,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self { mechanism: HotkeyMechanism::default(), start_with_windows: true }
    }
}
```

Y añadir los dos campos a `Config`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ui: UiConfig,
    pub files: FilesConfig,
    pub hotkey: HotkeyConfig,
}
```

`crates/notty-config/src/lib.rs` añade los nuevos tipos al `pub use`:

```rust
pub use model::{Config, Files, FilesConfig, HotkeyConfig, HotkeyMechanism, MenuBar, Preset, TabsPosition, TempMode, Theme, UiConfig};
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, +5 tests.

- [x] **Step 5: Commit**

```bash
git add crates/notty-config
git commit -m "feat(config): FilesConfig y HotkeyConfig (temporales y atajo global)"
```

---

### Task 2: notty-io — rutas de borradores y recuperación (tempdir)

**Files:**
- Create: `crates/notty-io/src/drafts.rs`
- Create: `crates/notty-io/src/recovery.rs`
- Modify: `crates/notty-io/src/lib.rs`

**Interfaces:**
- Consumes: `notty_io::atomic_write`, `notty_io::create_parent_dirs`.
- Produces:
  - `pub fn drafts_dir() -> std::path::PathBuf` (`%LOCALAPPDATA%\notty\drafts`; sin `LOCALAPPDATA`, cae a `./notty-drafts`).
  - `pub fn draft_filename(now: std::time::SystemTime, ext: &str) -> String`: `"AAAA-MM-DD_HHMM<ext>"` (por ejemplo `"2026-09-25_1432.txt"`), calculado a mano con aritmética de calendario simple (sin `chrono`: `SystemTime::duration_since(UNIX_EPOCH)` + una conversión a fecha civil; ver implementación).
  - `pub fn recovery_dir() -> std::path::PathBuf` (`%LOCALAPPDATA%\notty\recovery`).
  - `pub struct RecoveryEntry { pub name: String, pub text: String }`.
  - `pub fn dump_recovery(dir: &Path, entries: &[RecoveryEntry]) -> std::io::Result<()>`: escribe cada `entry.text` en `dir/{entry.name}.txt` (crea `dir` si falta). Pensado para llamarse en un `panic hook`, así que no debe usar nada que pueda volver a entrar en pánico (evita `.unwrap()`; ignora errores de un archivo individual y sigue con los demás).
  - `pub fn list_recovery(dir: &Path) -> Vec<RecoveryEntry>` (lee todos los `.txt` de `dir`; vacío si `dir` no existe).
  - `pub fn clear_recovery(dir: &Path) -> std::io::Result<()>` (borra `dir` entero si existe; `Ok(())` si no existía).

- [x] **Step 1: Escribir los tests que fallan**

`crates/notty-io/src/drafts.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn drafts_dir_ends_with_notty_drafts() {
        let p = drafts_dir();
        assert_eq!(p.file_name().unwrap(), "drafts");
        assert_eq!(p.parent().unwrap().file_name().unwrap(), "notty");
    }

    #[test]
    fn draft_filename_formats_date_and_time() {
        // 2024-01-15 10:30:00 UTC
        let t = UNIX_EPOCH + Duration::from_secs(1_705_314_600);
        assert_eq!(draft_filename(t, ".txt"), "2024-01-15_1030.txt");
    }

    #[test]
    fn draft_filename_uses_the_given_extension() {
        let t = UNIX_EPOCH + Duration::from_secs(0);
        assert!(draft_filename(t, ".md").ends_with(".md"));
    }
}
```

`crates/notty-io/src/recovery.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn dump_then_list_round_trips() {
        let dir = tempdir().unwrap();
        let d = dir.path().join("recovery");
        let entries = vec![
            RecoveryEntry { name: "a".into(), text: "hola".into() },
            RecoveryEntry { name: "b".into(), text: "mundo".into() },
        ];
        dump_recovery(&d, &entries).unwrap();
        let mut listed = list_recovery(&d);
        listed.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].text, "hola");
        assert_eq!(listed[1].text, "mundo");
    }

    #[test]
    fn list_recovery_of_missing_dir_is_empty() {
        let dir = tempdir().unwrap();
        assert!(list_recovery(&dir.path().join("no-existe")).is_empty());
    }

    #[test]
    fn clear_recovery_removes_the_dir() {
        let dir = tempdir().unwrap();
        let d = dir.path().join("recovery");
        dump_recovery(&d, &[RecoveryEntry { name: "a".into(), text: "x".into() }]).unwrap();
        clear_recovery(&d).unwrap();
        assert!(!d.exists());
    }

    #[test]
    fn clear_recovery_of_missing_dir_is_ok() {
        let dir = tempdir().unwrap();
        assert!(clear_recovery(&dir.path().join("no-existe")).is_ok());
    }
}
```

- [x] **Step 2: Ejecutar y ver que falla**

Añadir `mod drafts; mod recovery;` a `crates/notty-io/src/lib.rs`.

Run: `cargo test -p notty-io drafts recovery`
Expected: FAIL de compilación, `cannot find function drafts_dir`.

- [x] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-io/src/drafts.rs`:

```rust
use std::path::PathBuf;
use std::time::SystemTime;

pub fn drafts_dir() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(local) => PathBuf::from(local).join("notty").join("drafts"),
        None => PathBuf::from("notty-drafts"),
    }
}

/// "AAAA-MM-DD_HHMM<ext>", calculado a mano (sin `chrono`) a partir del reloj UTC.
/// Como es solo para nombrar archivos de borrador, no hace falta la hora local exacta.
pub fn draft_filename(now: SystemTime, ext: &str) -> String {
    let secs = now.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = secs / 86_400;
    let secs_of_day = secs % 86_400;
    let (hh, mm) = (secs_of_day / 3600, (secs_of_day % 3600) / 60);
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02}_{hh:02}{mm:02}{ext}")
}

/// Algoritmo de Howard Hinnant para convertir "días desde 1970-01-01" a (año, mes, día).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
```

Añadir **encima** del módulo de tests en `crates/notty-io/src/recovery.rs`:

```rust
use std::path::{Path, PathBuf};

pub fn recovery_dir() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(local) => PathBuf::from(local).join("notty").join("recovery"),
        None => PathBuf::from("notty-recovery"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryEntry {
    pub name: String,
    pub text: String,
}

/// Pensada para llamarse desde un `panic hook`: nunca usa `.unwrap()` y sigue
/// adelante con el resto de entradas aunque una falle.
pub fn dump_recovery(dir: &Path, entries: &[RecoveryEntry]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for e in entries {
        let safe_name: String = e.name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        let _ = std::fs::write(dir.join(format!("{safe_name}.txt")), &e.text);
    }
    Ok(())
}

pub fn list_recovery(dir: &Path) -> Vec<RecoveryEntry> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    read.flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "txt"))
        .filter_map(|e| {
            let text = std::fs::read_to_string(e.path()).ok()?;
            let name = e.path().file_stem()?.to_string_lossy().into_owned();
            Some(RecoveryEntry { name, text })
        })
        .collect()
}

pub fn clear_recovery(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, +7 tests.

- [x] **Step 5: Exportar los módulos**

`crates/notty-io/src/lib.rs` añade:

```rust
pub use drafts::{drafts_dir, draft_filename};
pub use recovery::{RecoveryEntry, clear_recovery, dump_recovery, list_recovery, recovery_dir};
```

- [x] **Step 6: Commit**

```bash
git add crates/notty-io
git commit -m "feat(io): rutas de borradores y volcado/lectura de recuperación"
```

---

### Task 3: notty-io — detectar cambios externos (mtime, tempdir)

**Files:**
- Create: `crates/notty-io/src/watch.rs`
- Modify: `crates/notty-io/src/lib.rs`

**Interfaces:**
- Consumes: `std::fs::metadata`.
- Produces: `pub fn mtime(path: &Path) -> std::io::Result<std::time::SystemTime>` y `pub fn changed_since(path: &Path, since: std::time::SystemTime) -> bool` (`true` si `mtime(path)` es estrictamente posterior a `since`, o si `mtime` falla porque el archivo ya no existe — un archivo borrado también cuenta como "cambiado").

- [x] **Step 1: Escribir los tests que fallan**

`crates/notty-io/src/watch.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn unchanged_file_reports_false() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "x").unwrap();
        let t = mtime(&p).unwrap();
        assert!(!changed_since(&p, t));
    }

    #[test]
    fn rewriting_the_file_reports_true() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "x").unwrap();
        let t = mtime(&p).unwrap();
        sleep(Duration::from_millis(20));
        std::fs::write(&p, "y").unwrap();
        assert!(changed_since(&p, t));
    }

    #[test]
    fn missing_file_counts_as_changed() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("no-existe.txt");
        assert!(changed_since(&p, std::time::SystemTime::now()));
    }
}
```

- [x] **Step 2: Ejecutar y ver que falla**

Añadir `mod watch;` a `crates/notty-io/src/lib.rs`.

Run: `cargo test -p notty-io watch`
Expected: FAIL de compilación, `cannot find function mtime`.

- [x] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-io/src/watch.rs`:

```rust
use std::path::Path;
use std::time::SystemTime;

pub fn mtime(path: &Path) -> std::io::Result<SystemTime> {
    std::fs::metadata(path)?.modified()
}

pub fn changed_since(path: &Path, since: SystemTime) -> bool {
    match mtime(path) {
        Ok(t) => t > since,
        Err(_) => true,
    }
}
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, +3 tests.

- [x] **Step 5: Exportar**

`crates/notty-io/src/lib.rs` añade:

```rust
pub use watch::{changed_since, mtime};
```

- [x] **Step 6: Commit**

```bash
git add crates/notty-io
git commit -m "feat(io): detecta si un archivo cambió en disco desde que se abrió"
```

---

### Task 4: notty-ipc — protocolo del pipe de instancia única (lógica pura, nuevo crate)

**Files:**
- Create: `crates/notty-ipc/Cargo.toml`
- Create: `crates/notty-ipc/src/lib.rs`
- Modify: `Cargo.toml` (workspace: no hace falta tocarlo, `members = ["crates/*"]` ya lo recoge).

**Interfaces:**
- Consumes: nada.
- Produces:
  - `pub const PIPE_NAME: &str = r"\\.\pipe\notty-instance"`.
  - `pub enum Message { OpenPath(String), NewTemp, NewPermanent }`.
  - `pub fn encode(msg: &Message) -> Vec<u8>`: una línea UTF-8 terminada en `\n`: `"OPEN <ruta>\n"`, `"NEW_TEMP\n"` o `"NEW_PERMANENT\n"`.
  - `pub fn decode(bytes: &[u8]) -> Option<Message>`: la operación inversa; `None` si no reconoce el formato.

- [x] **Step 1: Crear el crate y los tests que fallan**

`crates/notty-ipc/Cargo.toml`:

```toml
[package]
name = "notty-ipc"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
```

`crates/notty-ipc/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_open_path() {
        let msg = Message::OpenPath(r"C:\Users\ana\nota.txt".to_string());
        assert_eq!(decode(&encode(&msg)), Some(msg));
    }

    #[test]
    fn round_trips_new_temp_and_permanent() {
        assert_eq!(decode(&encode(&Message::NewTemp)), Some(Message::NewTemp));
        assert_eq!(decode(&encode(&Message::NewPermanent)), Some(Message::NewPermanent));
    }

    #[test]
    fn encoded_message_ends_with_newline() {
        assert!(encode(&Message::NewTemp).ends_with(b"\n"));
    }

    #[test]
    fn decode_of_garbage_is_none() {
        assert_eq!(decode(b"algo-que-no-es-un-mensaje\n"), None);
    }

    #[test]
    fn decode_of_empty_is_none() {
        assert_eq!(decode(b""), None);
    }
}
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ipc`
Expected: FAIL de compilación, `cannot find type Message`.

- [x] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-ipc/src/lib.rs`:

```rust
pub const PIPE_NAME: &str = r"\\.\pipe\notty-instance";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    OpenPath(String),
    NewTemp,
    NewPermanent,
}

pub fn encode(msg: &Message) -> Vec<u8> {
    let line = match msg {
        Message::OpenPath(p) => format!("OPEN {p}"),
        Message::NewTemp => "NEW_TEMP".to_string(),
        Message::NewPermanent => "NEW_PERMANENT".to_string(),
    };
    let mut out = line.into_bytes();
    out.push(b'\n');
    out
}

pub fn decode(bytes: &[u8]) -> Option<Message> {
    let text = std::str::from_utf8(bytes).ok()?.trim_end_matches('\n');
    if let Some(path) = text.strip_prefix("OPEN ") {
        return Some(Message::OpenPath(path.to_string()));
    }
    match text {
        "NEW_TEMP" => Some(Message::NewTemp),
        "NEW_PERMANENT" => Some(Message::NewPermanent),
        _ => None,
    }
}
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, +5 tests.

- [x] **Step 5: Commit**

```bash
git add Cargo.lock crates/notty-ipc
git commit -m "feat(ipc): protocolo del pipe de instancia única y del daemon"
```

---

### Task 5: EditorState — temporales (lógica pura + tempdir para Draft)

**Files:**
- Modify: `crates/notty-ui/src/editor.rs`

**Interfaces:**
- Consumes: `notty_io::{drafts_dir, draft_filename, atomic_write, create_parent_dirs}`.
- Produces: `EditorState` gana `pub temp: Option<notty_config::TempMode>` y `pub open_mtime: Option<std::time::SystemTime>` (`None` en `new_empty`/`from_opened` salvo que `from_opened` sí rellene `open_mtime` con `notty_io::mtime(&path)`, ignorando el error). `pub fn new_temp_at(mode: notty_config::TempMode, dir: &Path, ext: &str, now: std::time::SystemTime) -> EditorState`: en `Draft`, calcula la ruta con `dir.join(draft_filename(now, ext))`, la asigna a `path` y dirty a `false` (el archivo aún no existe en disco hasta el primer guardado/autoguardado, ver Task 7); en `Volatile`, `path = None` igual que `new_empty`. `pub fn new_temp(mode, ext) -> EditorState` (envoltorio que usa `notty_io::drafts_dir()` y `SystemTime::now()`).

- [x] **Step 1: Escribir los tests que fallan**

Añadir a `mod tests` en `crates/notty-ui/src/editor.rs`:

```rust
    #[test]
    fn draft_gets_a_real_path_inside_the_given_dir() {
        let dir = tempfile::tempdir().unwrap();
        let s = EditorState::new_temp_at(notty_config::TempMode::Draft, dir.path(), ".txt", std::time::SystemTime::UNIX_EPOCH);
        assert_eq!(s.temp, Some(notty_config::TempMode::Draft));
        assert!(s.path.as_ref().unwrap().starts_with(dir.path()));
        assert!(s.path.as_ref().unwrap().extension().is_some());
    }

    #[test]
    fn volatile_has_no_path() {
        let dir = tempfile::tempdir().unwrap();
        let s = EditorState::new_temp_at(notty_config::TempMode::Volatile, dir.path(), ".txt", std::time::SystemTime::now());
        assert_eq!(s.temp, Some(notty_config::TempMode::Volatile));
        assert!(s.path.is_none());
    }
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui editor new_temp`
Expected: FAIL de compilación, `no function or associated item named new_temp_at`.

- [x] **Step 3: Implementar**

Añadir los dos campos a `struct EditorState` (junto a `path`, `encoding`, etc.) y a `new_empty`/`from_opened`:

```rust
    pub temp: Option<notty_config::TempMode>,
    pub open_mtime: Option<std::time::SystemTime>,
```

En `new_empty()`: `temp: None, open_mtime: None`. En `from_opened(opened)`: `temp: None, open_mtime: notty_io::mtime(&opened.path).ok()`.

Añadir el nuevo constructor en el primer `impl EditorState` (junto a `new_empty`/`from_opened`):

```rust
    pub fn new_temp_at(mode: notty_config::TempMode, dir: &std::path::Path, ext: &str, now: std::time::SystemTime) -> Self {
        let mut s = Self::new_empty();
        s.temp = Some(mode);
        if mode == notty_config::TempMode::Draft {
            s.path = Some(dir.join(notty_io::draft_filename(now, ext)));
        }
        s
    }

    pub fn new_temp(mode: notty_config::TempMode, ext: &str) -> Self {
        Self::new_temp_at(mode, &notty_io::drafts_dir(), ext, std::time::SystemTime::now())
    }
```

(`TempMode` necesita `PartialEq`/`Eq`, ya los tiene desde la Task 1 de este plan.)

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, +2 tests.

- [x] **Step 5: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): EditorState soporta temporales (borrador/volátil)"
```

---

### Task 6: Prompt de conflicto (lógica pura)

**Files:**
- Modify: `crates/notty-ui/src/prompt.rs`
- Modify: `crates/notty-ui/src/workspace.rs`

**Interfaces:**
- Consumes: nada nuevo.
- Produces: `Prompt` gana la variante `Conflict` (sin datos: el propio `ws.active()` ya tiene todo lo necesario — `path`, texto en memoria, y basta con releer el disco al resolver). `Workspace` gana `pub fn open_conflict(&mut self)` (`self.prompt = Prompt::Conflict`) — trivial, pero se deja como método para que `window.rs` no construya la variante a mano en varios sitios.

- [x] **Step 1: Escribir el test que falla**

Añadir a `mod tests` en `crates/notty-ui/src/workspace.rs`:

```rust
    #[test]
    fn open_conflict_sets_the_prompt() {
        let mut w = Workspace::new();
        w.open_conflict();
        assert!(matches!(w.prompt, crate::Prompt::Conflict));
    }
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui workspace conflict`
Expected: FAIL de compilación, `no variant named Conflict`.

- [x] **Step 3: Implementar**

En `crates/notty-ui/src/prompt.rs`, añadir la variante:

```rust
#[derive(Default)]
pub enum Prompt {
    #[default]
    None,
    Path(PathPromptState),
    Find(SearchState),
    Replace(SearchState),
    Conflict,
}
```

(Si en el Plan 5 añadiste también `VimCmdline(String)`, mantenla; esta Task solo suma `Conflict` a lo que ya hubiera.)

En `crates/notty-ui/src/workspace.rs`:

```rust
    pub fn open_conflict(&mut self) {
        self.prompt = crate::Prompt::Conflict;
    }
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, +1 test.

- [x] **Step 5: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): Prompt::Conflict para el aviso de cambio externo al guardar"
```

---

### Task 7: window.rs — autoguardado, conflicto y recuperación tras caída

**Files:**
- Modify: `crates/notty-ui/src/window.rs`
- Modify: `crates/notty/src/main.rs`

**Interfaces:**
- Consumes: `notty_io::{changed_since, dump_recovery, list_recovery, clear_recovery, recovery_dir, RecoveryEntry}`, `Prompt::Conflict`, `cfg.borrow().files`.
- Produces: guardar (`Ctrl+S` o autoguardado) comprueba primero si `st.path` es `Some` y `notty_io::changed_since(path, st.open_mtime.unwrap_or(UNIX_EPOCH))`; si es así, abre `Prompt::Conflict` en vez de escribir. Un temporizador (`SetTimer`) dispara el autoguardado ~1s después de la última edición para documentos con ruta real y `cfg.files.autosave`. Un `panic hook` instalado al arrancar vuelca todos los documentos sucios a `notty_io::recovery_dir()`; al arrancar, si `list_recovery` no está vacío, se ofrece recuperarlos.

- [x] **Step 1: Guardar con comprobación de conflicto**

Modificar el punto donde `window.rs` llama a `st.save()` (el guardado normal del Plan 2/Plan 4) para, antes de guardar, comprobar el conflicto:

```rust
fn try_save(ws: &mut crate::Workspace) {
    let st = ws.active();
    if let Some(path) = &st.path {
        if let Some(since) = st.open_mtime {
            if notty_io::changed_since(path, since) {
                ws.open_conflict();
                return;
            }
        }
    }
    let _ = ws.active_mut().save();
    if let Some(path) = ws.active().path.clone() {
        ws.active_mut().open_mtime = notty_io::mtime(&path).ok();
    }
}
```

Sustituye las llamadas directas a `ws.active_mut().save()` que ya hubiera en el manejo de `EditorAction::Save`/autoguardado por `try_save(ws)`.

- [x] **Step 2: Resolver el conflicto**

Cuando `ws.prompt == Prompt::Conflict`, dos teclas: `M` (0x4D) = "el mío" → `ws.active_mut().save()` seguido de actualizar `open_mtime` y `ws.close_prompt()`; `D` (0x44) = "el del disco" → releer el archivo con `notty_ui::open_as_document` y sustituir `ws.active_mut().doc`/`encoding`/`eol` por lo leído, `open_mtime` actualizado, `ws.close_prompt()`. `Esc` cierra sin hacer nada (el usuario decide más tarde, y el autoguardado sigue pausado mientras exista el conflicto — ver Step 3).

- [x] **Step 3: Autoguardado con `SetTimer`**

Al crear la ventana (`run`), llamar a `SetTimer(hwnd, ID_AUTOSAVE_TIMER, 1000, None)` (constante `ID_AUTOSAVE_TIMER: usize = 1`). En `WM_TIMER` con `wparam.0 == ID_AUTOSAVE_TIMER`: si `cfg.borrow().files.autosave`, recorrer los documentos del `Workspace` (o, más simple para esta primera versión, solo el activo) y, para cada uno con `path: Some(..)`, `temp: None o Draft` (los volátiles nunca se autoguardan), `doc.is_dirty()` y `matches!(ws.prompt, Prompt::Conflict) == false`, llamar a `try_save`.

(El "1s tras la última edición" del enunciado se simplifica aquí a un temporizador periódico de 1s que solo actúa si hay cambios sin guardar; es equivalente en la práctica y mucho más simple que reprogramar el temporizador en cada tecla — documenta esta simplificación si decides no reimplementar el debounce exacto.)

- [x] **Step 4: `panic hook` y recuperación al arrancar**

En `crates/notty/src/main.rs`, antes de crear la ventana:

```rust
fn main() -> windows::core::Result<()> {
    // (el resto de main.rs, argv/config, sigue igual que en planes anteriores)
    notty_ui::window::run(path.as_deref(), load)
}
```

En `crates/notty-ui/src/window.rs`, dentro de `run`, **antes** de crear la ventana, instalar el hook:

```rust
fn install_recovery_hook(ws_ptr: *const std::sync::Mutex<Vec<(String, String, bool)>>) {
    // ws_ptr apunta a una lista compartida (nombre, texto, sucio) que window.rs
    // actualiza cada vez que cambia el Workspace, pensada para leerse sin bloquear
    // si es posible incluso durante un pánico (un Mutex envenenado también sirve:
    // `lock().unwrap_or_else(|e| e.into_inner())`).
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("notty: pánico: {info}");
        let snapshot = unsafe { &*ws_ptr };
        let entries: Vec<notty_io::RecoveryEntry> = {
            let guard = snapshot.lock().unwrap_or_else(|e| e.into_inner());
            guard.iter().filter(|(_, _, dirty)| *dirty).enumerate()
                .map(|(i, (name, text, _))| notty_io::RecoveryEntry { name: format!("{i}_{name}"), text: text.clone() })
                .collect()
        };
        let _ = notty_io::dump_recovery(&notty_io::recovery_dir(), &entries);
    }));
}
```

Ajusta este mecanismo a como esté organizado realmente el estado por ventana tras los planes anteriores: lo esencial es (a) que exista una forma de leer, desde dentro del `panic hook`, el texto y el nombre de cada documento sucio sin necesidad de que el hilo que ha entrado en pánico coopere activamente (de ahí el `Mutex`/snapshot en vez de leer directamente el `Workspace` que vive en el estado normal de la ventana, que podría estar en un estado a medio mutar en el momento del pánico), y (b) que el volcado se intente incluso si el propio acceso está "envenenado" por el pánico.

Al arrancar (tras cargar `cfg`, antes o después de crear la ventana, como resulte más simple de encajar):

```rust
let recovered = notty_io::list_recovery(&notty_io::recovery_dir());
if !recovered.is_empty() {
    // Se ofrecen como documentos nuevos con ruta CLICKME (ninguno es "el archivo
    // original": si tenían ruta, esa se perdió al volcar solo nombre+texto; el
    // usuario decide dónde guardarlos igual que con cualquier documento nuevo).
    for entry in &recovered {
        let mut st = crate::EditorState::new_empty();
        st.doc = notty_core::Document::new(&entry.text, "\r\n");
        ws.open(st);
    }
    let _ = notty_io::clear_recovery(&notty_io::recovery_dir());
}
```

- [x] **Step 5: Compilar y comprobar manualmente**

Run: `cargo build --workspace`
Expected: compila.

Run: `cargo run --bin notty -- /tmp/notas.txt` en segundo plano, y mientras corre, modificar `/tmp/notas.txt` con otro programa (por ejemplo `echo cambiado >> /tmp/notas.txt` desde otra terminal) y luego intentar guardar desde notty con `Ctrl+S` tras escribir algo.
Expected (manual): aparece el conflicto; `M` conserva lo escrito en notty y lo guarda; `D` recarga lo que había en disco.

Run: activar `autosave = true` en `config.toml`, editar un documento con ruta real y esperar ~1-2s sin tocar nada.
Expected (manual): el archivo en disco cambia solo, sin pulsar `Ctrl+S`.

- [x] **Step 6: Commit**

```bash
git add crates/notty crates/notty-ui
git commit -m "feat(ui): autoguardado, conflicto al guardar y recuperación tras caída"
```

---

### Task 8: Instancia única — servidor del pipe en `notty` y `--daemon`

**Files:**
- Modify: `crates/notty/src/main.rs`
- Create: `crates/notty/src/daemon.rs`
- Modify: `crates/notty/Cargo.toml`

**Interfaces:**
- Consumes: `notty_ipc::{Message, encode, decode, PIPE_NAME}`.
- Produces: al arrancar `notty <ruta>` sin `--daemon`, si ya hay un servidor de pipe escuchando (otra instancia de `notty` normal, o el daemon), se envía `Message::OpenPath(ruta)` (o `NewTemp`/`NewPermanent` si así se invocó) y el proceso termina sin abrir ventana; si no hay nadie escuchando, la instancia actual crea el pipe y se convierte en el servidor mientras viva (además de abrir su propia ventana normalmente). `notty --daemon` no abre ninguna ventana de documento: solo sirve el pipe y espera (la bandeja/atajos llegan en la Task 9).

- [x] **Step 1: Añadir las funcionalidades de *named pipes***

Run:

```bash
cargo add windows -p notty --features Win32_Storage_FileSystem,Win32_System_Pipes,Win32_Security
```

Y añadir la dependencia al `notty-ipc` recién creado:

```bash
cargo add notty-ipc -p notty --path crates/notty-ipc
```

- [x] **Step 2: Cliente del pipe (intentar reenviar antes de abrir ventana propia)**

`crates/notty/src/main.rs`. Arquitectura: intentar `CreateFileW(PIPE_NAME, ...)` como cliente; si tiene éxito, escribir el mensaje codificado y salir con éxito; si falla (no hay servidor), seguir el arranque normal y, además, lanzar un hilo servidor.

```rust
fn try_forward_to_existing_instance(msg: &notty_ipc::Message) -> bool {
    use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_GENERIC_WRITE, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES};
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
```

En `main()`, antes de crear la ventana:

```rust
let msg = notty_ipc::Message::OpenPath(path.clone().unwrap_or_default());
if path.is_some() && try_forward_to_existing_instance(&msg) {
    return Ok(());
}
```

(Si `path` es `None`, es decir "notty sin argumentos", no tiene sentido reenviar nada — se abre una ventana nueva con documento vacío como hasta ahora, aunque ya haya otra instancia corriendo; solo se evita duplicar ventana cuando se pide abrir un archivo concreto. Reenviar `NewTemp`/`NewPermanent` es responsabilidad del daemon, no de este camino de arranque normal — ver Task 9.)

- [x] **Step 3: Servidor del pipe (hilo en segundo plano dentro de la instancia con ventana)**

```rust
fn spawn_pipe_server(sender: std::sync::mpsc::Sender<notty_ipc::Message>) {
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
        let Ok(handle) = handle else { break };
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
```

Este hilo manda cada mensaje recibido por un `std::sync::mpsc::Sender`; el extremo receptor (`Receiver`) se guarda en el estado de la ventana principal y se consulta con `try_recv()` en un `WM_TIMER` corto (por ejemplo cada 100ms, otro `SetTimer` con id distinto al de autoguardado) o, más simple, se envuelve el `Sender` para que además llame a `PostMessageW(hwnd, WM_APP_IPC, ...)` y así reaccionar al instante en vez de con sondeo — usa lo que compile más simple con la organización real de `window.rs`; ambas formas son válidas.

Al recibir `Message::OpenPath(p)` (con `p` no vacío) en la ventana principal: abrir ese archivo igual que hace `Ctrl+O` al confirmar una ruta existente (reutiliza el mismo camino que la Task 8/9 del Plan 4 ya implementó para abrir por ruta), según `cfg.ui.files` como pestaña nueva o buffer nuevo.

- [x] **Step 4: Modo `--daemon`**

`crates/notty/src/main.rs`:

```rust
fn main() -> windows::core::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--daemon") {
        return notty::daemon::run();
    }
    // ... resto del arranque normal (path, config, try_forward_to_existing_instance, run) ...
}
```

`crates/notty/src/daemon.rs` (esqueleto para esta tarea; la Task 9 lo completa con bandeja y atajos):

```rust
//! notty --daemon: sin ventana de documento. Sirve el pipe y, más adelante
//! (Task 9), escucha los atajos globales y muestra el icono de bandeja.

pub fn run() -> windows::core::Result<()> {
    // Por ahora, solo sirve el pipe: si alguien reenvía NewTemp/NewPermanent,
    // se limita a lanzar `notty.exe` normal (sin argumentos especiales todavía;
    // la Task 9 completa el reenvío real de "nuevo temporal"/"nuevo permanente").
    let (tx, _rx) = std::sync::mpsc::channel();
    super::spawn_pipe_server(tx); // ajusta la visibilidad de spawn_pipe_server (pub(crate)) para poder llamarla desde aquí
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
```

- [x] **Step 5: Compilar y comprobar manualmente**

Run: `cargo build --workspace`
Expected: compila.

Run (dos terminales): primero `cargo run --bin notty -- /tmp/notas.txt &` (déjalo corriendo en segundo plano), y en la segunda `timeout 3 cargo run --bin notty -- /tmp/notas.txt` otra vez con la misma ruta.
Expected (manual): la segunda invocación termina casi al instante (no abre una segunda ventana); la primera instancia recibe el mensaje y, si vuelves a mirarla, no ha crasheado.

- [x] **Step 6: Commit**

```bash
git add crates/notty
git commit -m "feat(daemon): instancia única mediante un named pipe"
```

---

### Task 9: Daemon con bandeja y atajos globales, alternativa `.lnk`

**Files:**
- Modify: `crates/notty/src/daemon.rs`
- Create: `crates/notty/src/shortcut.rs`
- Modify: `crates/notty/src/main.rs`

**Interfaces:**
- Consumes: `notty_config::{HotkeyMechanism, load, default_path}`, `notty_input::parse_key_spec` (reutilizado para leer las combinaciones desde config), `notty_ipc::Message`.
- Produces: `daemon::run()` completo: icono en la bandeja (`Shell_NotifyIconW`) y dos atajos globales (`RegisterHotKey`) — por defecto `Win+Alt+N` (nuevo temporal) y `Win+Alt+Shift+N` (nuevo permanente) si `cfg.hotkey.mechanism == Daemon` — que, al pulsarse, lanzan `notty.exe --new-temp`/`notty.exe --new-permanent` (nuevos flags de `main.rs` que crean directamente un `EditorState::new_temp`/uno vacío con ruta `None`, sin pasar por el pipe: cada pulsación abre su propia ventana nueva, que es lo que se espera de "nuevo" temporal/permanente). `shortcut::create(target_exe: &Path, args: &str, hotkey_vk: u32, hotkey_mods: u32, lnk_path: &Path) -> windows::core::Result<()>` crea un `.lnk` con `IShellLinkW`/`IPersistFile` que incluye la tecla rápida (solo válida como `Ctrl+Alt+Letra` en un acceso directo de Windows, según limitación del propio sistema operativo — recuérdalo en el nombre de la constante de atajo por defecto para este mecanismo).

- [x] **Step 1: Añadir las funcionalidades de bandeja, atajos y COM/shell**

Run:

```bash
cargo add windows -p notty --features Win32_UI_Shell,Win32_System_Com,Win32_UI_Input_KeyboardAndMouse
```

- [x] **Step 2: Bandeja y atajos globales**

`crates/notty/src/daemon.rs`, sustituir el esqueleto de la Task 8 por la versión completa:

```rust
//! notty --daemon: sin ventana de documento. Bandeja + atajos globales + pipe.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NOTIFYICONDATAW, Shell_NotifyIconW};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, LoadIconW, MSG, RegisterClassExW,
    RegisterHotKey, WM_DESTROY, WM_HOTKEY, WNDCLASSEXW, IDI_APPLICATION,
};
use windows::core::{Result, w};

const ID_HOTKEY_TEMP: i32 = 1;
const ID_HOTKEY_PERM: i32 = 2;
const WM_APP_TRAYICON: u32 = 0x8000 + 1;

pub fn run() -> Result<()> {
    let cfg = match notty_config::load(&notty_config::default_path()) {
        notty_config::LoadResult::Loaded(c) | notty_config::LoadResult::Missing(c) | notty_config::LoadResult::Defaulted(c, _) => c,
    };
    if cfg.hotkey.mechanism != notty_config::HotkeyMechanism::Daemon {
        return Ok(()); // el usuario eligió el mecanismo .lnk: el daemon no tiene nada que hacer.
    }

    unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
        let class_name = w!("NottyDaemonClass");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(Default::default(), class_name, w!("notty (segundo plano)"), Default::default(), 0, 0, 0, 0, None, None, instance, None)?;

        let (temp_vk, temp_mods) = hotkey_or_default("Win+Alt+N");
        let (perm_vk, perm_mods) = hotkey_or_default("Win+Alt+Shift+N");
        let _ = RegisterHotKey(Some(hwnd), ID_HOTKEY_TEMP, windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS(temp_mods), temp_vk);
        let _ = RegisterHotKey(Some(hwnd), ID_HOTKEY_PERM, windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS(perm_mods), perm_vk);

        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_APP_TRAYICON;
        nid.hIcon = LoadIconW(None, IDI_APPLICATION).unwrap_or_default();
        let tip = w!("notty");
        nid.szTip[..tip.as_wide().len()].copy_from_slice(tip.as_wide());
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);

        let (tx, _rx) = std::sync::mpsc::channel();
        super::spawn_pipe_server(tx);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

/// Convierte una cadena "Win+Alt+N" en (vk, modificadores de RegisterHotKey).
/// Reutiliza `notty_input::parse_key_spec` para la letra y añade el bit MOD_WIN
/// a mano, porque `Modifiers` de `notty-input` no distingue la tecla Windows
/// (no hace falta en el resto de la app, solo aquí).
fn hotkey_or_default(spec: &str) -> (u32, u32) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};
    let has_win = spec.to_lowercase().contains("win+");
    let rest = spec.replace("Win+", "").replace("win+", "");
    let (vk, m) = notty_input::parse_key_spec(&rest).unwrap_or((0x4E, notty_input::Modifiers::default()));
    let mut mods = 0u32;
    if has_win { mods |= MOD_WIN.0; }
    if m.ctrl { mods |= MOD_CONTROL.0; }
    if m.alt { mods |= MOD_ALT.0; }
    if m.shift { mods |= MOD_SHIFT.0; }
    (vk, mods)
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_HOTKEY => {
                let arg = match wparam.0 as i32 {
                    ID_HOTKEY_TEMP => Some("--new-temp"),
                    ID_HOTKEY_PERM => Some("--new-permanent"),
                    _ => None,
                };
                if let Some(arg) = arg {
                    let _ = std::process::Command::new(std::env::current_exe().unwrap_or_default()).arg(arg).spawn();
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
```

Ajusta `notty_config::LoadResult` en el `match` inicial de `run()` si su forma exacta difiere (ya se usa igual en `main.rs` desde el Plan 3); ajusta cualquier otra firma de `windows` al compilar.

- [x] **Step 3: Flags `--new-temp`/`--new-permanent` en `main.rs`**

En `crates/notty/src/main.rs`, junto al `if args.get(1) == Some("--daemon")` de la Task 8:

```rust
    let new_kind = match args.get(1).map(String::as_str) {
        Some("--new-temp") => Some(notty_config::TempMode::Draft), // el modo real (borrador/volátil) se decide con cfg.files.temp_mode, ver más abajo
        Some("--new-permanent") => None,
        _ => None,
    };
```

Y, tras cargar `cfg`, si `args.get(1)` era `"--new-temp"`, crear la ventana con `EditorState::new_temp(cfg.files.temp_mode, &cfg.files.default_extension)` como documento inicial en vez del vacío de siempre (ajusta `notty_ui::window::run` para aceptar este caso, o añade una función hermana `run_with_temp(cfg, mode)` si resulta más simple que enredar la firma existente).

- [x] **Step 4: Alternativa `.lnk`**

`crates/notty/src/shortcut.rs`:

```rust
use std::path::Path;

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{Result, w};

/// Crea un acceso directo `.lnk` a `target_exe` con los argumentos dados. La tecla
/// rápida de un .lnk de Windows solo admite Ctrl+Alt+<letra>, limitación del propio
/// sistema (no de notty): por eso este mecanismo usa combinaciones distintas a las
/// del daemon (ver Ajustes, Task 10).
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
```

(Un `.lnk` no permite fijar una tecla rápida por COM tan directamente como el resto de propiedades: `IShellLinkW` no tiene un `SetHotkey` estándar accesible desde `windows-rs` en todas las versiones; si al compilar no aparece, deja la creación del acceso directo sin tecla rápida asignada por código y añade una nota en el propio texto de Ajustes —Task 10— explicando que el usuario puede asignarla a mano desde "Propiedades" del acceso directo en el Explorador, que es la única vía 100% fiable multiplataforma-de-Windows para esto.)

`crates/notty/src/main.rs` añade `mod shortcut;` y `mod daemon;` junto al resto de `mod`.

- [x] **Step 5: Compilar**

Run: `cargo build --workspace`
Expected: compila.

- [x] **Step 6: Commit**

```bash
git add crates/notty
git commit -m "feat(daemon): bandeja, atajos globales y acceso directo .lnk alternativo"
```

---

### Task 10: Ajustes — secciones Archivos y Atajo global, y `vim_always` al crear documentos

> **Nota de contexto (Plan 5b ya hecho):** cuando se escribió esta tarea, la ventana de
> Ajustes todavía se dibujaba con controles nativos de Win32 (`radio`/`checkbox` +
> `apply_control`, un patrón por `ID_*`). El Plan 5b (ajuste visual) la reescribió por
> completo: ahora es una ventana propia dibujada con Direct2D (como el resto de la app),
> cuyo contenido sale de un modelo puro y testeable en `crates/notty-ui/src/settings_model.rs`
> (`SECTIONS`/`Row`/`SettingKey`/`SettingValue`/`apply`) — `settings_window.rs` solo dibuja
> lo que ese modelo describe y hace hit-testing genérico por tipo de `Row`, sin nada
> específico de cada ajuste. Esta versión de la Task 10 está adaptada a esa arquitectura
> real; ignora cualquier mención a `ID_*`/`radio`/`checkbox`/`apply_control` de la
> descripción original, que ya no existen.

**Files:**
- Modify: `crates/notty-ui/src/settings_model.rs`
- Modify: `crates/notty-ui/src/window.rs`

**Interfaces:**
- Consumes: `cfg.files`, `cfg.hotkey`.
- Produces: `SECTIONS` gana dos secciones más, **Archivos** y **Atajo global**, con las mismas filas que ya describía el Plan 5b como diferidas a este plan: `temp_mode` (Borrador/Volátil) y `autosave` en Archivos; `hotkey.mechanism` (Segundo plano/Acceso directo) y `start_with_windows` en Atajo global. `default_extension` y `large_file_mb` (campos de texto libre en la maqueta) **no** se añaden aquí — `Row` no tiene todavía una variante de campo de texto editable, y merece su propio ciclo TDD en el Plan 7 en vez de improvisarse. Quedan con sus valores por defecto (`.txt`, `50`), editables solo a mano en `config.toml`. Además, `window.rs` pasa a leer `cfg.borrow().ui.vim_always` al crear cualquier `EditorState` nuevo (pestaña nueva, documento abierto, mensaje del pipe, flags `--new-temp`/`--new-permanent`) para arrancarlo ya con `vim: Some(VimState::default())` si está activado.

- [x] **Step 1: Escribir los tests que fallan**

Añadir a `mod tests` en `crates/notty-ui/src/settings_model.rs`:

```rust
    #[test]
    fn archivos_and_atajo_global_sections_exist() {
        let names: Vec<&str> = SECTIONS.iter().map(|s| s.name).collect();
        assert_eq!(names, ["Apariencia", "Ventana", "Teclado", "Archivos", "Atajo global"]);
    }

    #[test]
    fn applying_temp_mode_and_autosave_touches_files_not_ui_preset() {
        let mut cfg = Config::default();
        let preset_before = cfg.ui.preset;
        apply(&mut cfg, SettingKey::TempMode, SettingValue::TempMode(notty_config::TempMode::Volatile));
        apply(&mut cfg, SettingKey::Autosave, SettingValue::Bool(true));
        assert_eq!(cfg.files.temp_mode, notty_config::TempMode::Volatile);
        assert!(cfg.files.autosave);
        // Archivos/Atajo global no son piezas de un preset de Apariencia: no lo tocan.
        assert_eq!(cfg.ui.preset, preset_before);
    }

    #[test]
    fn applying_hotkey_settings_touches_hotkey_not_ui() {
        let mut cfg = Config::default();
        apply(&mut cfg, SettingKey::HotkeyMechanism, SettingValue::HotkeyMechanism(notty_config::HotkeyMechanism::Lnk));
        apply(&mut cfg, SettingKey::StartWithWindows, SettingValue::Bool(false));
        assert_eq!(cfg.hotkey.mechanism, notty_config::HotkeyMechanism::Lnk);
        assert!(!cfg.hotkey.start_with_windows);
    }
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui settings_model`
Expected: FAIL de compilación, `no variant named TempMode` (o similar).

- [x] **Step 3: Implementar**

En `crates/notty-ui/src/settings_model.rs`:

1. Importar `notty_config::{FilesConfig, HotkeyConfig, HotkeyMechanism, TempMode}` (los que hagan falta) junto al resto de `use`.
2. Añadir a `SettingKey`: `TempMode, Autosave, HotkeyMechanism, StartWithWindows`.
3. Añadir a `SettingValue`: `TempMode(TempMode), HotkeyMechanism(HotkeyMechanism)` (`Bool` ya sirve para `Autosave`/`StartWithWindows`).
4. Dos constantes de filas nuevas, con los mismos textos que ya traía la versión original de esta tarea:

```rust
const ARCHIVOS: &[Row] = &[
    Row::Seg {
        title: "Archivos temporales",
        desc: "Borrador: se guarda solo y se borra al cerrar si no le das ruta. Volátil: nunca toca el disco.",
        key: SettingKey::TempMode,
        options: &[
            ("Borrador", SettingValue::TempMode(TempMode::Draft)),
            ("Volátil", SettingValue::TempMode(TempMode::Volatile)),
        ],
    },
    Row::Toggle {
        title: "Autoguardado",
        desc: "Guarda sola tras dejar de escribir. Solo si el archivo ya tiene ruta.",
        key: SettingKey::Autosave,
    },
];

const ATAJO_GLOBAL: &[Row] = &[
    Row::Seg {
        title: "Cómo se escucha el atajo",
        desc: "Segundo plano: ~1 MB de RAM, cualquier combinación, instantáneo. Acceso directo: nada residente, solo Ctrl+Alt+letra.",
        key: SettingKey::HotkeyMechanism,
        options: &[
            ("Segundo plano", SettingValue::HotkeyMechanism(HotkeyMechanism::Daemon)),
            ("Acceso directo", SettingValue::HotkeyMechanism(HotkeyMechanism::Lnk)),
        ],
    },
    Row::Kbd { title: "Nuevo temporal", keys: "Win+Alt+N" },
    Row::Kbd { title: "Nuevo permanente", keys: "Win+Alt+Shift+N" },
    Row::Toggle { title: "Iniciar con Windows", desc: "", key: SettingKey::StartWithWindows },
];
```

5. Añadir ambas a `SECTIONS`:

```rust
pub const SECTIONS: &[Section] = &[
    Section { id: "apariencia", name: "Apariencia", rows: APARIENCIA },
    Section { id: "ventana", name: "Ventana", rows: VENTANA },
    Section { id: "teclado", name: "Teclado", rows: TECLADO },
    Section { id: "archivos", name: "Archivos", rows: ARCHIVOS },
    Section { id: "atajo_global", name: "Atajo global", rows: ATAJO_GLOBAL },
];
```

6. En `apply`, los cuatro ajustes nuevos van en su propio `match` que actualiza `cfg.files`/`cfg.hotkey` y **no** toca `cfg.ui.preset` (a diferencia del resto, que sí lo marca `Custom` al final): sepáralos del `match` existente con un `return` propio, por ejemplo:

```rust
pub fn apply(cfg: &mut Config, key: SettingKey, value: SettingValue) {
    if let (SettingKey::Preset, SettingValue::Preset(p)) = (key, value) {
        notty_config::apply_preset(&mut cfg.ui, p);
        return;
    }
    match (key, value) {
        (SettingKey::TempMode, SettingValue::TempMode(m)) => { cfg.files.temp_mode = m; return; }
        (SettingKey::Autosave, SettingValue::Bool(b)) => { cfg.files.autosave = b; return; }
        (SettingKey::HotkeyMechanism, SettingValue::HotkeyMechanism(m)) => { cfg.hotkey.mechanism = m; return; }
        (SettingKey::StartWithWindows, SettingValue::Bool(b)) => { cfg.hotkey.start_with_windows = b; return; }
        _ => {}
    }
    match (key, value) {
        // ... el resto de brazos ya existentes (Theme, LineNumbers, Files, ...) ...
        _ => return, // combinación key/value que no tiene sentido: no hace nada
    }
    cfg.ui.preset = Preset::Custom;
}
```

Guarda también con `notty_config::save` en el mismo sitio donde `settings_window.rs` ya lo hace tras cualquier `apply` (no cambia: sigue siendo un único punto para todos los ajustes).

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, +2 tests (y el test `three_sections_matching_config_today` ya existente hay que actualizarlo/renombrarlo para las 5 secciones, o queda cubierto por el nuevo `archivos_and_atajo_global_sections_exist` — elimina el duplicado si hace falta).

- [x] **Step 5: `vim_always` al crear documentos**

En `crates/notty-ui/src/window.rs`, en cada sitio donde se construye un `EditorState::new_empty()`/`new_temp(..)`/`from_opened(..)` para añadirlo al `Workspace` (el `Ctrl+N` de `UiCommand::NewTab`, la apertura por ruta, la recepción de `Message::OpenPath` de la Task 8, los flags `--new-temp`/`--new-permanent` de la Task 9), envolver la creación así:

```rust
fn maybe_vim(mut st: crate::EditorState, cfg: &notty_config::Config) -> crate::EditorState {
    if cfg.ui.vim_always {
        st.vim = Some(crate::VimState::default());
    }
    st
}
```

y llamar a `maybe_vim(EditorState::new_empty(), &cfg.borrow())` (o el constructor que toque) en cada uno de esos puntos en vez de usar el `EditorState` a secas.

- [x] **Step 6: Compilar**

Run: `cargo build --workspace`
Expected: compila.

- [x] **Step 7: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): Ajustes de Archivos/Atajo global y vim_always al crear documentos"
```

---

### Task 11: Comprobación final del workspace

**Files:** ninguno nuevo; solo verificación.

- [ ] **Step 1: Tests y lints de todo el workspace**

Run: `cargo test --workspace`
Expected: PASS. Cuenta los tests reales con el propio `cargo test`.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: sin avisos. Corregir cualquier aviso en el archivo que lo señale y repetir hasta que quede limpio.

- [ ] **Step 2: Build release**

Run: `cargo build --release --workspace`
Expected: compila sin errores.

- [ ] **Step 3: Commit (si hubo cambios de la revisión)**

```bash
git add -A
git commit -m "chore(ui): pasa clippy y build release tras archivos vivos" --allow-empty
```

---

## Hoja de ruta: planes siguientes

| Plan | Contenido |
|---|---|
| **7 · Pulido y garantías** | Campos de texto libre en Ajustes (`default_extension`, `large_file_mb`), tecla rápida del `.lnk`, UI Automation, alto contraste/DPI, fuzzing (`cargo-fuzz`) de detección de tipo y buffer, presupuesto de rendimiento (tamaño de binario, arranque, apertura de 100 MB) vigilado en CI. |
