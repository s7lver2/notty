# notty · Plan 2: Ventana y editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Un `notty.exe` que abre un archivo de texto pasado por línea de comandos en una ventana nativa de Windows 11 (Mica, barra de título del sistema), lo renderiza con DirectWrite, se edita con teclado y ratón, y se guarda con `Ctrl+S` de forma atómica conservando codificación y fin de línea.

**Architecture:** Dos crates nuevos. `notty-ui` contiene todo lo reutilizable: cálculo de líneas visibles (lógica pura, testable sin ventana), el mapeo de teclas a acciones de edición (lógica pura, testable), y el código Win32/Direct2D/DirectWrite que crea la ventana y pinta el texto. El binario `notty` solo interpreta `argv`, crea la ventana y entra en el bucle de mensajes. `notty-ui` usa `notty_core::Document` para el texto y `notty_io` para abrir y guardar.

**Tech Stack:** Rust stable 1.96 (MSVC), `windows` (crate `windows-rs`, funcionalidades Win32 de ventanas, Direct2D, DirectWrite, DWM, portapapeles), `notty-core`, `notty-io` (del Plan 1).

**Spec:** `docs/superpowers/specs/2026-09-24-notty-design.md`
**Plan anterior:** `docs/superpowers/plans/2026-09-24-notty-01-nucleo.md` (núcleo ya implementado y con tests en verde)

## Global Constraints

- Solo Windows 10/11; toolchain `stable-x86_64-pc-windows-msvc`.
- Ventana nativa Win32 con barra de título del sistema (sin ventana sin bordes ni chrome dibujado a mano) y fondo Mica.
- El texto se renderiza con Direct2D + DirectWrite (mismo motor que usa Windows), no con ningún framework de UI de terceros.
- El guardado siempre pasa por `notty_io::atomic_write`; nunca se escribe el archivo directamente.
- Se conservan codificación, BOM y fin de línea originales del archivo al guardar (usa lo que `notty_io::open` detectó), salvo que una fase posterior lo cambie explícitamente.
- Repintar solo lo necesario: en `WM_PAINT` solo se calculan y dibujan las líneas visibles, nunca el documento entero.
- Textos visibles para el usuario en español.
- Un commit por tarea terminada.
- **Sobre el código Win32/Direct2D/DirectWrite de este plan:** se da la arquitectura, los tipos y las llamadas por su nombre tal como existen en el crate `windows`. Como la firma exacta (orden de parámetros, si un método es `unsafe`, si devuelve `windows::core::Result<T>` o `T`) depende de la versión instalada, compílalo, y si no coincide, ajusta la llamada consultando `cargo doc -p windows --open` o el propio error del compilador **sin cambiar el comportamiento descrito**. Los pasos marcados como "lógica pura" sí son código exacto y no se deben alterar.

## File Structure

```
crates/notty-ui/Cargo.toml
crates/notty-ui/src/lib.rs          re-exports
crates/notty-ui/src/viewport.rs     lógica pura: rango de líneas visibles, autoscroll
crates/notty-ui/src/keymap.rs       lógica pura: tecla virtual + modificadores → EditorAction
crates/notty-ui/src/doc_io.rs       puente Document <-> notty_io::Opened (abrir/guardar con codificación)
crates/notty-ui/src/window.rs       clase de ventana, WndProc, bucle de mensajes, Mica
crates/notty-ui/src/render.rs       recursos Direct2D/DirectWrite, dibujar texto+caret+selección+estado
crates/notty-ui/src/editor.rs       EditorState: une Document + scroll + selección de ratón; procesa mensajes
crates/notty-ui/src/clipboard.rs    copiar/cortar/pegar vía CF_UNICODETEXT
crates/notty/Cargo.toml
crates/notty/src/main.rs            argv -> ruta -> abrir -> crear ventana -> bucle de mensajes
```

---

### Task 1: notty-ui crate + rango de líneas visibles (lógica pura)

**Files:**
- Create: `crates/notty-ui/Cargo.toml`
- Create: `crates/notty-ui/src/lib.rs`
- Create: `crates/notty-ui/src/viewport.rs`
- Modify: `Cargo.toml` (workspace)

**Interfaces:**
- Consumes: nada.
- Produces: `pub struct Viewport { pub first_line: usize, pub visible_lines: usize }` con `Viewport::new(line_height_px: f32, client_height_px: f32) -> Self` (calcula `visible_lines` a partir del alto del cliente; mínimo 1), `Viewport::range(&self, total_lines: usize) -> Range<usize>` (recortado a `[0, total_lines)`), `Viewport::scroll_to_include(&mut self, line: usize, total_lines: usize)` (mueve `first_line` lo mínimo para que `line` quede visible, sin pasarse del final del documento).

- [x] **Step 1: Añadir el crate al workspace**

`Cargo.toml` (raíz), añadir `"crates/notty-ui"` no hace falta explícito porque `members = ["crates/*"]` ya lo recoge. Solo comprobar que sigue así; si no, añadirlo.

`crates/notty-ui/Cargo.toml`:

```toml
[package]
name = "notty-ui"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
notty-core = { path = "../notty-core" }
notty-io = { path = "../notty-io" }
```

`crates/notty-ui/src/lib.rs`:

```rust
//! notty-ui: ventana Win32, render Direct2D/DirectWrite y lógica de edición en pantalla.

mod viewport;

pub use viewport::Viewport;
```

- [x] **Step 2: Escribir los tests que fallan**

`crates/notty-ui/src/viewport.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_lines_from_pixel_height() {
        let v = Viewport::new(20.0, 205.0);
        assert_eq!(v.visible_lines, 10);
    }

    #[test]
    fn at_least_one_visible_line() {
        let v = Viewport::new(20.0, 5.0);
        assert_eq!(v.visible_lines, 1);
    }

    #[test]
    fn range_is_clamped_to_document() {
        let v = Viewport { first_line: 0, visible_lines: 10 };
        assert_eq!(v.range(3), 0..3);
    }

    #[test]
    fn range_starts_at_first_line() {
        let v = Viewport { first_line: 5, visible_lines: 4 };
        assert_eq!(v.range(100), 5..9);
    }

    #[test]
    fn scroll_down_to_include_line_below() {
        let mut v = Viewport { first_line: 0, visible_lines: 10 };
        v.scroll_to_include(15, 100);
        assert_eq!(v.first_line, 6);
        assert!(v.range(100).contains(&15));
    }

    #[test]
    fn scroll_up_to_include_line_above() {
        let mut v = Viewport { first_line: 20, visible_lines: 10 };
        v.scroll_to_include(5, 100);
        assert_eq!(v.first_line, 5);
    }

    #[test]
    fn already_visible_line_does_not_move_scroll() {
        let mut v = Viewport { first_line: 10, visible_lines: 10 };
        v.scroll_to_include(15, 100);
        assert_eq!(v.first_line, 10);
    }

    #[test]
    fn scroll_never_goes_past_document_end() {
        let mut v = Viewport { first_line: 0, visible_lines: 10 };
        v.scroll_to_include(4, 5);
        assert_eq!(v.first_line, 0);
    }
}
```

- [x] **Step 3: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui`
Expected: FAIL de compilación, `cannot find type Viewport`.

- [x] **Step 4: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-ui/src/viewport.rs`:

```rust
use std::ops::Range;

/// Qué líneas del documento hay que dibujar. `first_line` es la primera línea
/// visible (scroll); `visible_lines` sale del alto de la ventana en píxeles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub first_line: usize,
    pub visible_lines: usize,
}

impl Viewport {
    pub fn new(line_height_px: f32, client_height_px: f32) -> Self {
        let lines = (client_height_px / line_height_px).floor() as usize;
        Self { first_line: 0, visible_lines: lines.max(1) }
    }

    /// Rango `[first_line, first_line + visible_lines)` recortado a `[0, total_lines)`.
    pub fn range(&self, total_lines: usize) -> Range<usize> {
        let start = self.first_line.min(total_lines);
        let end = (start + self.visible_lines).min(total_lines);
        start..end
    }

    /// Mueve `first_line` lo mínimo para que `line` quede dentro de lo visible.
    pub fn scroll_to_include(&mut self, line: usize, total_lines: usize) {
        if line < self.first_line {
            self.first_line = line;
        } else if line >= self.first_line + self.visible_lines {
            self.first_line = line + 1 - self.visible_lines;
        }
        let max_first = total_lines.saturating_sub(1);
        self.first_line = self.first_line.min(max_first);
    }
}
```

- [x] **Step 5: Ejecutar y ver que pasa**

Run: `cargo test -p notty-ui`
Expected: PASS, 8 tests.

- [x] **Step 6: Commit**

```bash
git add Cargo.toml crates/notty-ui
git commit -m "feat(ui): crate notty-ui y cálculo de líneas visibles"
```

---

### Task 2: Mapeo de teclas a acciones de edición (lógica pura)

**Files:**
- Create: `crates/notty-ui/src/keymap.rs`
- Modify: `crates/notty-ui/src/lib.rs`

**Interfaces:**
- Consumes: nada (constantes `VK_*` propias, sin depender de `windows` para poder testear sin la crate de Windows).
- Produces:
  - `pub struct Modifiers { pub ctrl: bool, pub shift: bool, pub alt: bool }`.
  - `pub enum EditorAction { MoveLeft, MoveRight, MoveUp, MoveDown, MoveHome, MoveEnd, MoveDocStart, MoveDocEnd, ExtendLeft, ExtendRight, ExtendUp, ExtendDown, ExtendHome, ExtendEnd, Backspace, DeleteForward, InsertNewline, Undo, Redo, SelectAll, Copy, Cut, Paste, Save, Find, None }`.
  - `pub fn action_for_vk(vk: u32, m: Modifiers) -> EditorAction` (usa los códigos de tecla virtual de Win32: `VK_LEFT = 0x25`, `VK_RIGHT = 0x27`, `VK_UP = 0x26`, `VK_DOWN = 0x28`, `VK_HOME = 0x24`, `VK_END = 0x23`, `VK_BACK = 0x08`, `VK_DELETE = 0x2E`, `VK_RETURN = 0x0D`, `'Z' = 0x5A`, `'Y' = 0x59`, `'A' = 0x41`, `'C' = 0x43`, `'X' = 0x58`, `'V' = 0x56`, `'S' = 0x53`, `'F' = 0x46`). Estas constantes son las mismas que expone `windows::Win32::UI::Input::KeyboardAndMouse`, así que `window.rs` podrá llamar a esta función pasándole el `wparam` del mensaje tal cual.

- [x] **Step 1: Escribir los tests que fallan**

`crates/notty-ui/src/keymap.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn m(ctrl: bool, shift: bool) -> Modifiers {
        Modifiers { ctrl, shift, alt: false }
    }

    #[test]
    fn plain_arrows_move() {
        assert_eq!(action_for_vk(0x25, m(false, false)), EditorAction::MoveLeft);
        assert_eq!(action_for_vk(0x27, m(false, false)), EditorAction::MoveRight);
        assert_eq!(action_for_vk(0x26, m(false, false)), EditorAction::MoveUp);
        assert_eq!(action_for_vk(0x28, m(false, false)), EditorAction::MoveDown);
    }

    #[test]
    fn shift_arrows_extend_selection() {
        assert_eq!(action_for_vk(0x25, m(false, true)), EditorAction::ExtendLeft);
        assert_eq!(action_for_vk(0x28, m(false, true)), EditorAction::ExtendDown);
    }

    #[test]
    fn home_end_plain_and_shift() {
        assert_eq!(action_for_vk(0x24, m(false, false)), EditorAction::MoveHome);
        assert_eq!(action_for_vk(0x24, m(false, true)), EditorAction::ExtendHome);
        assert_eq!(action_for_vk(0x23, m(false, false)), EditorAction::MoveEnd);
    }

    #[test]
    fn ctrl_home_end_go_to_doc_bounds() {
        assert_eq!(action_for_vk(0x24, m(true, false)), EditorAction::MoveDocStart);
        assert_eq!(action_for_vk(0x23, m(true, false)), EditorAction::MoveDocEnd);
    }

    #[test]
    fn backspace_delete_enter() {
        assert_eq!(action_for_vk(0x08, m(false, false)), EditorAction::Backspace);
        assert_eq!(action_for_vk(0x2E, m(false, false)), EditorAction::DeleteForward);
        assert_eq!(action_for_vk(0x0D, m(false, false)), EditorAction::InsertNewline);
    }

    #[test]
    fn ctrl_letters_map_to_commands() {
        assert_eq!(action_for_vk(0x5A, m(true, false)), EditorAction::Undo);
        assert_eq!(action_for_vk(0x5A, m(true, true)), EditorAction::Redo);
        assert_eq!(action_for_vk(0x59, m(true, false)), EditorAction::Redo);
        assert_eq!(action_for_vk(0x41, m(true, false)), EditorAction::SelectAll);
        assert_eq!(action_for_vk(0x43, m(true, false)), EditorAction::Copy);
        assert_eq!(action_for_vk(0x58, m(true, false)), EditorAction::Cut);
        assert_eq!(action_for_vk(0x56, m(true, false)), EditorAction::Paste);
        assert_eq!(action_for_vk(0x53, m(true, false)), EditorAction::Save);
        assert_eq!(action_for_vk(0x46, m(true, false)), EditorAction::Find);
    }

    #[test]
    fn plain_letter_is_not_a_command() {
        assert_eq!(action_for_vk(0x41, m(false, false)), EditorAction::None);
    }

    #[test]
    fn unknown_key_is_none() {
        assert_eq!(action_for_vk(0x90, m(false, false)), EditorAction::None);
    }
}
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui keymap`
Expected: FAIL de compilación, `cannot find function action_for_vk`.

- [x] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-ui/src/keymap.rs`:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    MoveLeft, MoveRight, MoveUp, MoveDown, MoveHome, MoveEnd, MoveDocStart, MoveDocEnd,
    ExtendLeft, ExtendRight, ExtendUp, ExtendDown, ExtendHome, ExtendEnd,
    Backspace, DeleteForward, InsertNewline,
    Undo, Redo, SelectAll, Copy, Cut, Paste, Save, Find,
    None,
}

/// Traduce una tecla virtual de Win32 (`WM_KEYDOWN`'s `wparam`) + modificadores a una acción.
/// Los códigos son los estándar de `windows::Win32::UI::Input::KeyboardAndMouse` (VK_*).
pub fn action_for_vk(vk: u32, m: Modifiers) -> EditorAction {
    use EditorAction::*;
    match (vk, m.ctrl, m.shift) {
        (0x25, false, false) => MoveLeft,
        (0x25, false, true) => ExtendLeft,
        (0x27, false, false) => MoveRight,
        (0x27, false, true) => ExtendRight,
        (0x26, false, false) => MoveUp,
        (0x26, false, true) => ExtendUp,
        (0x28, false, false) => MoveDown,
        (0x28, false, true) => ExtendDown,
        (0x24, false, false) => MoveHome,
        (0x24, false, true) => ExtendHome,
        (0x24, true, _) => MoveDocStart,
        (0x23, false, false) => MoveEnd,
        (0x23, false, true) => ExtendEnd,
        (0x23, true, _) => MoveDocEnd,
        (0x08, _, _) => Backspace,
        (0x2E, _, _) => DeleteForward,
        (0x0D, _, _) => InsertNewline,
        (0x5A, true, false) => Undo,
        (0x5A, true, true) => Redo,
        (0x59, true, _) => Redo,
        (0x41, true, _) => SelectAll,
        (0x43, true, _) => Copy,
        (0x58, true, _) => Cut,
        (0x56, true, _) => Paste,
        (0x53, true, _) => Save,
        (0x46, true, _) => Find,
        _ => None,
    }
}
```

`crates/notty-ui/src/lib.rs` queda:

```rust
//! notty-ui: ventana Win32, render Direct2D/DirectWrite y lógica de edición en pantalla.

mod keymap;
mod viewport;

pub use keymap::{EditorAction, Modifiers, action_for_vk};
pub use viewport::Viewport;
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test -p notty-ui`
Expected: PASS, 17 tests.

- [x] **Step 5: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): mapeo de teclas virtuales a acciones de edición"
```

---

### Task 3: Puente Document <-> notty_io (abrir y guardar con codificación)

**Files:**
- Create: `crates/notty-ui/src/doc_io.rs`
- Modify: `crates/notty-ui/src/lib.rs`

**Interfaces:**
- Consumes: `notty_core::Document`, `notty_io::{open, atomic_write, encode, decode, TextEncoding, LineEnding, Opened, CodecError}`.
- Produces:
  - `pub struct OpenedDoc { pub document: Document, pub encoding: TextEncoding, pub eol: LineEnding, pub writable: bool, pub lossy: bool, pub path: std::path::PathBuf }`.
  - `pub fn open_as_document(path: &Path) -> std::io::Result<OpenedDoc>`: usa `notty_io::open`; si devuelve `Opened::Raw`, error `io::Error` con `ErrorKind::InvalidData` y mensaje `"no es texto: ábrelo en modo raw"` (la vista raw llega en el Plan 5, así que por ahora el binario lanzador rechaza esos archivos con un mensaje claro).
  - `pub fn save_document(doc: &Document, path: &Path, encoding: TextEncoding) -> Result<(), notty_io::CodecError>`: codifica `doc.text()` con `encoding` y llama a `notty_io::atomic_write` (los errores de E/O se envuelven en una variante nueva `CodecError::Io(String)`).

- [x] **Step 1: Escribir los tests que fallan**

`crates/notty-ui/src/doc_io.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn opens_utf8_file_as_document() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        fs::write(&p, "hola\r\nmundo").unwrap();
        let opened = open_as_document(&p).unwrap();
        assert_eq!(opened.document.text(), "hola\r\nmundo");
        assert_eq!(opened.encoding, notty_io::TextEncoding::Utf8);
        assert_eq!(opened.eol, notty_io::LineEnding::Crlf);
        assert!(opened.writable);
        assert!(!opened.lossy);
        assert_eq!(opened.path, p);
    }

    #[test]
    fn opening_binary_file_is_an_error() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("bin.dat");
        fs::write(&p, [0u8, 1, 2, 3]).unwrap();
        assert!(open_as_document(&p).is_err());
    }

    #[test]
    fn opening_missing_file_is_an_error() {
        let dir = tempdir().unwrap();
        assert!(open_as_document(&dir.path().join("no-existe.txt")).is_err());
    }

    #[test]
    fn save_document_round_trips_utf8() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("out.txt");
        let doc = notty_core::Document::new("línea uno\r\nlínea dos", "\r\n");
        save_document(&doc, &p, notty_io::TextEncoding::Utf8).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "línea uno\r\nlínea dos");
    }

    #[test]
    fn save_document_writes_bom_for_utf16() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("out16.txt");
        let doc = notty_core::Document::new("hola", "\n");
        save_document(&doc, &p, notty_io::TextEncoding::Utf16Le).unwrap();
        let bytes = fs::read(&p).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
    }

    #[test]
    fn save_document_rejects_unmappable_chars() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("out.txt");
        let doc = notty_core::Document::new("日本語", "\n");
        assert!(save_document(&doc, &p, notty_io::TextEncoding::Windows1252).is_err());
    }
}
```

- [x] **Step 2: Ejecutar y ver que falla**

Añadir `tempfile` como dependencia de desarrollo. En `crates/notty-ui/Cargo.toml` añadir:

```toml
[dev-dependencies]
tempfile.workspace = true
```

Run: `cargo test -p notty-ui doc_io`
Expected: FAIL de compilación, `cannot find function open_as_document`.

- [x] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-ui/src/doc_io.rs`:

```rust
use std::io;
use std::path::{Path, PathBuf};

use notty_core::Document;
use notty_io::{CodecError, LineEnding, Opened, TextEncoding};

pub struct OpenedDoc {
    pub document: Document,
    pub encoding: TextEncoding,
    pub eol: LineEnding,
    pub writable: bool,
    pub lossy: bool,
    pub path: PathBuf,
}

pub fn open_as_document(path: &Path) -> io::Result<OpenedDoc> {
    match notty_io::open(path)? {
        Opened::Text { text, encoding, eol, writable, lossy } => {
            let document = Document::new(&text, eol.as_str());
            Ok(OpenedDoc { document, encoding, eol, writable: writable && !lossy, lossy, path: path.to_path_buf() })
        }
        Opened::Raw { .. } => Err(io::Error::new(io::ErrorKind::InvalidData, "no es texto: ábrelo en modo raw")),
    }
}

pub fn save_document(doc: &Document, path: &Path, encoding: TextEncoding) -> Result<(), CodecError> {
    let bytes = notty_io::encode(&doc.text(), encoding)?;
    notty_io::atomic_write(path, &bytes).map_err(|e| CodecError::Io(e.to_string()))
}
```

`notty_io::CodecError` necesita una variante nueva para envolver errores de E/S. Modificar `crates/notty-io/src/encoding.rs`, en el `enum CodecError`, añadiendo la variante:

```rust
    #[error("error de E/S: {0}")]
    Io(String),
```

(queda como tercera variante, junto a `Invalid` y `Unmappable`; no cambia ningún test existente porque ninguno construye `CodecError` exhaustivamente con un `match` sin `_`).

`crates/notty-ui/src/lib.rs` queda:

```rust
//! notty-ui: ventana Win32, render Direct2D/DirectWrite y lógica de edición en pantalla.

mod doc_io;
mod keymap;
mod viewport;

pub use doc_io::{OpenedDoc, open_as_document, save_document};
pub use keymap::{EditorAction, Modifiers, action_for_vk};
pub use viewport::Viewport;
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, 22 tests en `notty-ui`, y los 58 anteriores de `notty-core`/`notty-io` siguen en verde (80 en total).

- [x] **Step 5: Commit**

```bash
git add crates/notty-io crates/notty-ui
git commit -m "feat(ui): puente entre Document y notty-io para abrir/guardar"
```

---

### Task 4: Ventana Win32 con Mica (bin notty)

**Files:**
- Create: `crates/notty/Cargo.toml`
- Create: `crates/notty/src/main.rs`
- Create: `crates/notty-ui/src/window.rs`
- Modify: `crates/notty-ui/src/lib.rs`
- Modify: `crates/notty-ui/Cargo.toml`

**Interfaces:**
- Consumes: nada de las tareas anteriores todavía (esta tarea solo abre una ventana vacía; el texto llega en la Task 5).
- Produces: `pub fn run(initial_title: &str) -> windows::core::Result<()>` en `notty-ui::window`, que registra la clase de ventana, crea una `HWND` visible de 900x600 con fondo Mica y barra de título nativa, y entra en el bucle de mensajes hasta `WM_DESTROY`.

- [x] **Step 1: Añadir la dependencia `windows`**

Run (desde la raíz del repo):

```bash
cargo add windows -p notty-ui --features Win32_Foundation,Win32_UI_WindowsAndMessaging,Win32_Graphics_Gdi,Win32_Graphics_Dwm,Win32_System_LibraryLoader,Win32_UI_Input_KeyboardAndMouse
```

Esto añade `windows` a `crates/notty-ui/Cargo.toml` con las funcionalidades necesarias para esta tarea. Las funcionalidades de Direct2D/DirectWrite/portapapeles se añadirán en tareas posteriores con el mismo comando.

- [x] **Step 2: Crear el binario `notty`**

`crates/notty/Cargo.toml`:

```toml
[package]
name = "notty"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "notty"
path = "src/main.rs"

[dependencies]
notty-ui = { path = "../notty-ui" }
```

`crates/notty/src/main.rs`:

```rust
//! Punto de entrada de notty: interpreta argv y abre la ventana.

fn main() -> windows::core::Result<()> {
    let path = std::env::args().nth(1);
    notty_ui::window::run(path.as_deref())
}
```

- [x] **Step 3: Implementar la ventana (window.rs)**

`crates/notty-ui/src/window.rs`. Este código es de arranque Win32; compílalo y, si alguna firma no coincide con la versión de `windows` instalada, ajústala manteniendo el comportamiento (registrar clase, crear ventana visible con Mica, bucle de mensajes que termina en `WM_DESTROY` con `PostQuitMessage(0)`):

```rust
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMSBT_MAINWINDOW, DwmSetWindowAttribute,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG,
    PostQuitMessage, RegisterClassExW, ShowWindow, SW_SHOW, TranslateMessage, WM_DESTROY,
    WNDCLASSEXW, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW,
};
use windows::core::{PCWSTR, Result, w};

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Abre la ventana principal de notty y bloquea hasta que se cierra.
/// `path` es la ruta pasada por línea de comandos, si la hay (aún sin usar: llega en la Task 5).
pub fn run(path: Option<&str>) -> Result<()> {
    let title = match path {
        Some(p) => format!("{p} · notty"),
        None => "sin título · notty".to_string(),
    };

    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("NottyWindowClass");

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let title_wide = to_wide(&title);
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name,
            PCWSTR(title_wide.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            900,
            600,
            None,
            None,
            instance,
            None,
        )?;

        enable_mica(hwnd);

        let _ = ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

/// Activa el fondo Mica y el modo oscuro de la barra de título si el sistema está en oscuro.
/// Si `DwmSetWindowAttribute` falla (Windows más viejo que 11 22621), la ventana sigue
/// funcionando con el fondo por defecto: no es un error fatal.
unsafe fn enable_mica(hwnd: HWND) {
    let dark: i32 = if system_uses_dark_mode() { 1 } else { 0 };
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        &dark as *const _ as *const _,
        std::mem::size_of::<i32>() as u32,
    );
    let backdrop = DWMSBT_MAINWINDOW.0;
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_SYSTEMBACKDROP_TYPE,
        &backdrop as *const _ as *const _,
        std::mem::size_of::<i32>() as u32,
    );
}

/// Lee `HKCU\...\Personalize\AppsUseLightTheme`. Si no se puede leer, asume modo claro.
fn system_uses_dark_mode() -> bool {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    unsafe {
        let subkey = w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        let value = w!("AppsUseLightTheme");
        let mut data: u32 = 1;
        let mut size = std::mem::size_of::<u32>() as u32;
        let ok = RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut _ as *mut _),
            Some(&mut size),
        );
        ok.is_ok() && data == 0
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
```

Si `RegGetValueW` no está disponible con esas features, añade `Win32_System_Registry` con:

```bash
cargo add windows -p notty-ui --features Win32_System_Registry
```

`crates/notty-ui/src/lib.rs` queda:

```rust
//! notty-ui: ventana Win32, render Direct2D/DirectWrite y lógica de edición en pantalla.

mod doc_io;
mod keymap;
pub mod window;
mod viewport;

pub use doc_io::{OpenedDoc, open_as_document, save_document};
pub use keymap::{EditorAction, Modifiers, action_for_vk};
pub use viewport::Viewport;
```

(`window` es `pub mod`, no un re-export, porque `main.rs` llama a `notty_ui::window::run`.)

- [x] **Step 4: Compilar y comprobar manualmente**

Run: `cargo build --workspace`
Expected: compila sin errores. Corrige cualquier firma de `windows` que no coincida (ver la nota de Global Constraints) hasta que compile.

Run: `cargo run --bin notty`
Expected (manual): se abre una ventana de 900x600 con la barra de título de Windows 11, título "sin título · notty", fondo Mica (translúcido, sigue el tema claro/oscuro del sistema). Cerrarla con la X termina el proceso sin error. Confírmalo visualmente antes de seguir.

- [x] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/notty crates/notty-ui
git commit -m "feat(ui): ventana Win32 con Mica y bucle de mensajes"
```

---

### Task 5: Render del texto con Direct2D/DirectWrite

**Files:**
- Create: `crates/notty-ui/src/render.rs`
- Create: `crates/notty-ui/src/editor.rs`
- Modify: `crates/notty-ui/src/window.rs`
- Modify: `crates/notty-ui/src/lib.rs`

**Interfaces:**
- Consumes: `Document` (Plan 1), `Viewport` (Task 1), `OpenedDoc`/`open_as_document` (Task 3).
- Produces:
  - `pub struct EditorState { pub doc: Document, pub viewport: Viewport, pub encoding: TextEncoding, pub eol: LineEnding, pub path: Option<PathBuf> }` con `EditorState::new_empty() -> Self` y `EditorState::from_opened(OpenedDoc) -> Self`.
  - `pub struct Renderer` en `render.rs`: envuelve los recursos de Direct2D/DirectWrite ligados a una `HWND` (factory, render target, formato de texto monoespaciado). `Renderer::new(hwnd: HWND) -> Result<Self>`, `Renderer::resize(&mut self, width: u32, height: u32)`, `Renderer::line_height(&self) -> f32`, `Renderer::paint(&mut self, state: &EditorState)` (dibuja fondo + las líneas visibles según `state.viewport`, sin caret ni selección todavía: llegan en la Task 6).
  - `window::run` pasa a guardar un `EditorState` y un `Renderer` por ventana (vía `GWLP_USERDATA` o una estructura `Box` cuyo puntero se guarda ahí) y llama a `renderer.paint(&state)` en `WM_PAINT`.

- [x] **Step 1: Añadir las funcionalidades de Direct2D/DirectWrite**

Run:

```bash
cargo add windows -p notty-ui --features Win32_Graphics_Direct2D,Win32_Graphics_Direct2D_Common,Win32_Graphics_DirectWrite,Win32_Graphics_Dxgi_Common,Win32_UI_HiDpi
```

- [x] **Step 2: EditorState**

`crates/notty-ui/src/editor.rs`:

```rust
use std::path::PathBuf;

use notty_core::Document;
use notty_io::{LineEnding, TextEncoding};

use crate::{OpenedDoc, Viewport};

/// Todo lo que necesita una ventana de notty para saber qué mostrar y qué guardar.
pub struct EditorState {
    pub doc: Document,
    pub viewport: Viewport,
    pub encoding: TextEncoding,
    pub eol: LineEnding,
    pub path: Option<PathBuf>,
}

impl EditorState {
    pub fn new_empty() -> Self {
        Self {
            doc: Document::new("", LineEnding::Crlf.as_str()),
            viewport: Viewport { first_line: 0, visible_lines: 1 },
            encoding: TextEncoding::Utf8,
            eol: LineEnding::Crlf,
            path: None,
        }
    }

    pub fn from_opened(opened: OpenedDoc) -> Self {
        Self {
            doc: opened.document,
            viewport: Viewport { first_line: 0, visible_lines: 1 },
            encoding: opened.encoding,
            eol: opened.eol,
            path: Some(opened.path),
        }
    }
}
```

No lleva tests unitarios propios: es una estructura de datos sin lógica (la lógica ya está testeada en `Document`, `Viewport` y `doc_io`).

- [x] **Step 3: Renderer**

`crates/notty-ui/src/render.rs`. Arquitectura: crear `ID2D1Factory` con `D2D1CreateFactory`, un `ID2D1HwndRenderTarget` con `factory.CreateHwndRenderTarget`, un `IDWriteFactory` con `DWriteCreateFactory`, y un `IDWriteTextFormat` monoespaciado (`"Cascadia Mono"`, si no está disponible en el sistema usar `"Consolas"` como alternativa) de 13pt. En `paint`, `BeginDraw`, `Clear` con el color de fondo, un `IDWriteTextLayout` por línea visible dibujado con `DrawTextLayout`, `EndDraw`.

```rust
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_POINT_2F, D2D_RECT_F, D2D_SIZE_U};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BRUSH_PROPERTIES, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_PROPERTIES, D2D1CreateFactory,
    ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
};
use windows::core::Result;

use crate::EditorState;

const FONT_SIZE: f32 = 16.0;
const BG: D2D1_COLOR_F = D2D1_COLOR_F { r: 0.09, g: 0.09, b: 0.10, a: 1.0 };
const FG: D2D1_COLOR_F = D2D1_COLOR_F { r: 0.92, g: 0.92, b: 0.93, a: 1.0 };
const PADDING_X: f32 = 8.0;
const PADDING_TOP: f32 = 8.0;

pub struct Renderer {
    _d2d: ID2D1Factory,
    target: ID2D1HwndRenderTarget,
    _dwrite: IDWriteFactory,
    text_format: IDWriteTextFormat,
    fg_brush: ID2D1SolidColorBrush,
    line_height: f32,
}

impl Renderer {
    pub fn new(hwnd: HWND) -> Result<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let target = d2d.CreateHwndRenderTarget(
                &D2D1_RENDER_TARGET_PROPERTIES::default(),
                &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                    hwnd,
                    pixelSize: D2D_SIZE_U { width: 900, height: 600 },
                    ..Default::default()
                },
            )?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let text_format = dwrite.CreateTextFormat(
                windows::core::w!("Cascadia Mono"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                FONT_SIZE,
                windows::core::w!(""),
            )?;
            let fg_brush = target.CreateSolidColorBrush(&FG, Some(&D2D1_BRUSH_PROPERTIES { opacity: 1.0, transform: Matrix3x2::identity() }))?;
            let line_height = FONT_SIZE * 1.35;
            Ok(Self { _d2d: d2d, target, _dwrite: dwrite, text_format, fg_brush, line_height })
        }
    }

    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        unsafe {
            let _ = self.target.Resize(&D2D_SIZE_U { width, height });
        }
    }

    /// Dibuja fondo + las líneas visibles de `state`. No dibuja caret ni selección (Task 6).
    pub fn paint(&mut self, state: &EditorState) {
        let buf = state.doc.buffer();
        let total_lines = buf.len_lines();
        let range = state.viewport.range(total_lines);

        unsafe {
            self.target.BeginDraw();
            self.target.Clear(Some(&BG));

            let mut y = PADDING_TOP;
            for line in range {
                let start = buf.line_start(line);
                let end = if line + 1 < total_lines { buf.line_start(line + 1) } else { buf.len_chars() };
                let text: String = buf.slice(start..end).trim_end_matches(['\r', '\n']).to_string();
                if !text.is_empty() {
                    let wide: Vec<u16> = text.encode_utf16().collect();
                    if let Ok(layout) = self._dwrite.CreateTextLayout(&wide, &self.text_format, f32::MAX, self.line_height) {
                        self.target.DrawTextLayout(
                            D2D_POINT_2F { x: PADDING_X, y },
                            &layout,
                            &self.fg_brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                        );
                    }
                }
                y += self.line_height;
            }

            let _ = self.target.EndDraw(None, None);
        }
    }
}
```

Si `D2D1_RENDER_TARGET_PROPERTIES` o alguna constante no implementan `Default`, constrúyela explícitamente con sus campos a cero/valores por defecto documentados por `windows-rs` (consulta `cargo doc`). Si `CreateHwndRenderTarget` pide el tamaño real del cliente en vez de un valor fijo, usa `GetClientRect` (de `Win32_UI_WindowsAndMessaging`, ya importado) en `window.rs` antes de llamar a `Renderer::new`.

- [x] **Step 4: Conectar con la ventana**

Modificar `crates/notty-ui/src/window.rs` para:
1. Tras `enable_mica(hwnd)`, construir `let mut state = match path { Some(p) => EditorState::from_opened(crate::open_as_document(std::path::Path::new(p)).expect("no se pudo abrir el archivo")), None => EditorState::new_empty() };` y `let mut renderer = Renderer::new(hwnd)?;`.
2. Ajustar `state.viewport.visible_lines` a partir de `renderer.line_height()` y el alto real del cliente (`GetClientRect`).
3. Guardar `state` y `renderer` en una estructura que viva mientras la ventana exista (la forma más simple: mover el bucle de mensajes a un closure o guardar un puntero en `GWLP_USERDATA` con `SetWindowLongPtrW`/`GetWindowLongPtrW`). Usa la variante que compile más simple mientras cumpla: `WM_PAINT` puede acceder a `renderer` y `state` de esa ventana concreta.
4. En `wndproc`, manejar `WM_PAINT`: llamar a `renderer.paint(&state)` entre `BeginPaint`/`EndPaint` (o simplemente llamar a `paint` y luego `ValidateRect(hwnd, None)`, ya que `Renderer` ya hace su propio `BeginDraw`/`EndDraw`).
5. Manejar `WM_SIZE`: leer el nuevo alto/ancho de `lparam` (`LOWORD`/`HIWORD`), llamar a `renderer.resize(...)` y recalcular `state.viewport.visible_lines`, luego forzar repintado con `InvalidateRect(hwnd, None, false)`.

`crates/notty-ui/src/lib.rs` queda:

```rust
//! notty-ui: ventana Win32, render Direct2D/DirectWrite y lógica de edición en pantalla.

mod doc_io;
pub mod editor;
mod keymap;
pub mod render;
mod viewport;
pub mod window;

pub use doc_io::{OpenedDoc, open_as_document, save_document};
pub use editor::EditorState;
pub use keymap::{EditorAction, Modifiers, action_for_vk};
pub use render::Renderer;
pub use viewport::Viewport;
```

- [x] **Step 5: Compilar y comprobar manualmente**

Run: `cargo build --workspace`
Expected: compila. Ajusta las firmas de `windows` que hagan falta (ver Global Constraints).

Preparar un archivo de prueba:

```bash
printf 'Lista de la compra\r\n- leche\r\n- pan\r\n\r\nLlamar a Juan antes de las 6.\r\n' > /tmp/notas.txt
```

Run: `cargo run --bin notty -- /tmp/notas.txt`
Expected (manual): la ventana muestra las 5 líneas del archivo con fuente monoespaciada, fondo oscuro, sin caret todavía. Redimensionar la ventana no rompe el render (puede recortar líneas, eso es correcto: `visible_lines` cambia con el tamaño).

Run: `cargo run --bin notty`
Expected (manual): ventana vacía sin texto, sin fallos.

- [x] **Step 6: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): render del texto visible con Direct2D/DirectWrite"
```

---

### Task 6: Edición con teclado, caret y selección

**Files:**
- Modify: `crates/notty-ui/src/editor.rs`
- Modify: `crates/notty-ui/src/render.rs`
- Modify: `crates/notty-ui/src/window.rs`

**Interfaces:**
- Consumes: `EditorAction`, `action_for_vk` (Task 2), `Document` (edición, Plan 1), `Viewport::scroll_to_include`.
- Produces: `EditorState::apply(&mut self, action: EditorAction, now: Instant)` (lógica pura, testable: mueve selección o edita el documento y hace `scroll_to_include`), `EditorState::insert_char(&mut self, ch: char, now: Instant)` (para `WM_CHAR`, ignora caracteres de control salvo que ya los cubra `InsertNewline`/`Backspace`), `Renderer::paint` ahora también dibuja el caret y el fondo de la selección; `window.rs` reenvía `WM_KEYDOWN` → `action_for_vk` → `EditorState::apply`, y `WM_CHAR` → `EditorState::insert_char`, seguido de `InvalidateRect`.

- [x] **Step 1: Escribir los tests que fallan**

Añadir a `crates/notty-ui/src/editor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditorAction::*;
    use std::time::Instant;

    fn state(text: &str) -> EditorState {
        let mut s = EditorState::new_empty();
        s.doc = Document::new(text, "\n");
        s.viewport = Viewport { first_line: 0, visible_lines: 3 };
        s
    }

    #[test]
    fn move_right_advances_caret() {
        let mut s = state("abc");
        s.apply(MoveRight, Instant::now());
        assert_eq!(s.doc.selection(), notty_core::Selection::caret(1));
    }

    #[test]
    fn move_left_at_start_stays_at_zero() {
        let mut s = state("abc");
        s.apply(MoveLeft, Instant::now());
        assert_eq!(s.doc.selection(), notty_core::Selection::caret(0));
    }

    #[test]
    fn extend_right_grows_selection_from_anchor() {
        let mut s = state("abcdef");
        s.apply(ExtendRight, Instant::now());
        s.apply(ExtendRight, Instant::now());
        assert_eq!(s.doc.selection(), notty_core::Selection { anchor: 0, head: 2 });
    }

    #[test]
    fn move_after_extend_collapses_to_caret() {
        let mut s = state("abcdef");
        s.apply(ExtendRight, Instant::now());
        s.apply(ExtendRight, Instant::now());
        s.apply(MoveRight, Instant::now());
        assert_eq!(s.doc.selection(), notty_core::Selection::caret(3));
    }

    #[test]
    fn move_down_scrolls_viewport() {
        let mut s = state("a\nb\nc\nd\ne\n");
        for _ in 0..4 {
            s.apply(MoveDown, Instant::now());
        }
        assert!(s.viewport.range(s.doc.buffer().len_lines()).contains(&4));
    }

    #[test]
    fn select_all_selects_whole_document() {
        let mut s = state("abc");
        s.apply(SelectAll, Instant::now());
        assert_eq!(s.doc.selection(), notty_core::Selection { anchor: 0, head: 3 });
    }

    #[test]
    fn backspace_removes_previous_char() {
        let mut s = state("abc");
        s.apply(MoveDocEnd, Instant::now());
        s.apply(Backspace, Instant::now());
        assert_eq!(s.doc.text(), "ab");
    }

    #[test]
    fn insert_char_types_at_caret() {
        let mut s = state("ac");
        s.apply(MoveRight, Instant::now());
        s.insert_char('b', Instant::now());
        assert_eq!(s.doc.text(), "abc");
        assert_eq!(s.doc.selection(), notty_core::Selection::caret(2));
    }

    #[test]
    fn insert_char_ignores_control_chars() {
        let mut s = state("a");
        s.insert_char('\r', Instant::now());
        s.insert_char('\u{7f}', Instant::now());
        assert_eq!(s.doc.text(), "a");
    }

    #[test]
    fn undo_redo_go_through_document() {
        let mut s = state("");
        s.insert_char('a', Instant::now());
        s.apply(Undo, Instant::now());
        assert_eq!(s.doc.text(), "");
        s.apply(Redo, Instant::now());
        assert_eq!(s.doc.text(), "a");
    }
}
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui editor`
Expected: FAIL de compilación, `no method named apply found for struct EditorState`.

- [x] **Step 3: Implementar**

Añadir a `crates/notty-ui/src/editor.rs`, **entre** `impl EditorState { ... }` (el de `new_empty`/`from_opened`) y el módulo de tests, un segundo bloque `impl EditorState`:

```rust
impl EditorState {
    pub fn apply(&mut self, action: crate::EditorAction, now: std::time::Instant) {
        use crate::EditorAction::*;
        let buf_len = || self.doc.buffer().len_chars();
        let cur = self.doc.selection();
        let (anchor, head) = (cur.anchor, cur.head);

        let mut move_to = |doc: &mut Document, idx: usize, extend: bool| {
            if extend {
                doc.set_selection(anchor, idx);
            } else {
                doc.set_cursor(idx);
            }
        };

        match action {
            MoveLeft => move_to(&mut self.doc, head.saturating_sub(1), false),
            ExtendLeft => move_to(&mut self.doc, head.saturating_sub(1), true),
            MoveRight => move_to(&mut self.doc, (head + 1).min(buf_len()), false),
            ExtendRight => move_to(&mut self.doc, (head + 1).min(buf_len()), true),
            MoveUp => self.vertical(-1, false),
            ExtendUp => self.vertical(-1, true),
            MoveDown => self.vertical(1, false),
            ExtendDown => self.vertical(1, true),
            MoveHome => {
                let (line, _) = self.doc.buffer().line_col(head);
                move_to(&mut self.doc, self.doc.buffer().line_start(line), false);
            }
            ExtendHome => {
                let (line, _) = self.doc.buffer().line_col(head);
                move_to(&mut self.doc, self.doc.buffer().line_start(line), true);
            }
            MoveEnd => move_to(&mut self.doc, self.line_end(head), false),
            ExtendEnd => move_to(&mut self.doc, self.line_end(head), true),
            MoveDocStart => move_to(&mut self.doc, 0, false),
            MoveDocEnd => move_to(&mut self.doc, buf_len(), false),
            SelectAll => self.doc.set_selection(0, buf_len()),
            Backspace => self.doc.backspace(now),
            DeleteForward => self.doc.delete_forward(now),
            InsertNewline => self.doc.insert_newline(now),
            Undo => {
                self.doc.undo();
            }
            Redo => {
                self.doc.redo();
            }
            Copy | Cut | Paste | Save | Find | None => {} // se resuelven en window.rs (Tasks 7-8) o en planes posteriores
        }

        let (line, _) = self.doc.buffer().line_col(self.doc.selection().head);
        self.viewport.scroll_to_include(line, self.doc.buffer().len_lines());
    }

    /// Inserta un carácter tecleado. Ignora control chars salvo los que ya
    /// llegan como `EditorAction` (Enter, Backspace, Delete).
    pub fn insert_char(&mut self, ch: char, now: std::time::Instant) {
        if ch.is_control() {
            return;
        }
        self.doc.insert(&ch.to_string(), now);
        let (line, _) = self.doc.buffer().line_col(self.doc.selection().head);
        self.viewport.scroll_to_include(line, self.doc.buffer().len_lines());
    }

    fn line_end(&self, idx: usize) -> usize {
        let buf = self.doc.buffer();
        let (line, _) = buf.line_col(idx);
        let total = buf.len_lines();
        let end = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
        let text = buf.slice(buf.line_start(line)..end);
        buf.line_start(line) + text.trim_end_matches(['\r', '\n']).chars().count()
    }

    fn vertical(&mut self, delta: i32, extend: bool) {
        let buf = self.doc.buffer();
        let (line, col) = buf.line_col(self.doc.selection().head);
        let target_line = (line as i64 + delta as i64).clamp(0, buf.len_lines() as i64 - 1) as usize;
        let target_start = buf.line_start(target_line);
        let target_end = self.line_end(target_start);
        let idx = (target_start + col).min(target_end);
        if extend {
            self.doc.set_selection(self.doc.selection().anchor, idx);
        } else {
            self.doc.set_cursor(idx);
        }
    }
}
```

`Document` necesita ser importable en `editor.rs`: comprobar que `use notty_core::Document;` sigue en la parte de arriba del archivo (ya está desde la Task 5).

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS. `notty-ui` pasa a 33 tests (17 + 22 previos ya incluían editor sin tests, ahora +11 de esta tarea sobre el total de la Task 2/3); en total debe ser 91 tests en el workspace.

- [x] **Step 5: Dibujar caret y selección**

En `crates/notty-ui/src/render.rs`, añadir tras crear `fg_brush` en `Renderer::new` un segundo pincel para el caret/selección:

```rust
            let caret_brush = target.CreateSolidColorBrush(&FG, Some(&D2D1_BRUSH_PROPERTIES { opacity: 1.0, transform: Matrix3x2::identity() }))?;
            let sel_color = D2D1_COLOR_F { r: 0.30, g: 0.45, b: 0.85, a: 0.35 };
            let sel_brush = target.CreateSolidColorBrush(&sel_color, Some(&D2D1_BRUSH_PROPERTIES { opacity: 1.0, transform: Matrix3x2::identity() }))?;
```

Añadir esos dos campos a `struct Renderer` (`caret_brush: ID2D1SolidColorBrush`, `sel_brush: ID2D1SolidColorBrush`) y a la construcción de `Self { ... }`.

Dentro de `paint`, por cada línea visible que contenga el caret o parte de la selección, calcular su posición en X con `layout` (usando `IDWriteTextLayout::HitTestTextPosition` para la columna del caret, en chars relativos al inicio de esa línea) y dibujar:
- si `state.doc.selection()` no está vacía y la línea se solapa con `state.doc.selection().range()`: un `FillRectangle` con `sel_brush` cubriendo el tramo de esa línea dentro de la selección (ancho mínimo 2px si la selección incluye el salto de línea, para que se note).
- si la línea contiene `state.doc.selection().head` (el caret): un `FillRectangle` de 2px de ancho por `line_height` de alto con `caret_brush` en la X de esa columna.

Deja el cálculo exacto de X con `HitTestTextPosition` (ajusta la firma exacta al compilar; devuelve la coordenada X del punto de inserción para un índice de char dentro del layout de esa línea).

- [x] **Step 6: Conectar teclado en window.rs**

En `wndproc`, añadir dos casos nuevos (además de `WM_PAINT`/`WM_SIZE`/`WM_DESTROY` ya existentes):
- `WM_KEYDOWN`: leer `vk = wparam.0 as u32`, construir `Modifiers` desde `GetKeyState(VK_CONTROL)`/`GetKeyState(VK_SHIFT)` (bit alto negativo = tecla pulsada), llamar a `crate::action_for_vk(vk, mods)`, y si no es `EditorAction::None`, llamar a `state.apply(action, std::time::Instant::now())` y `InvalidateRect(hwnd, None, false)`.
- `WM_CHAR`: `wparam.0 as u32` es un code point UTF-16; convertir a `char` con `char::from_u32` (si falla, ignorar el mensaje) y llamar a `state.insert_char(ch, Instant::now())` + `InvalidateRect`.

Necesitarás `Win32_UI_Input_KeyboardAndMouse` para `GetKeyState` (ya añadido en la Task 4) y las constantes `VK_CONTROL = 0x11`, `VK_SHIFT = 0x10`.

- [x] **Step 7: Comprobar manualmente**

Run: `cargo build --workspace` hasta que compile sin errores.

Run: `cargo run --bin notty -- /tmp/notas.txt`
Expected (manual):
- Aparece un caret parpadeante-o-fijo (sin parpadeo es aceptable para esta tarea) al principio del texto.
- Las flechas mueven el caret línea a línea y carácter a carácter, `Home`/`End` van al principio/fin de línea, `Ctrl+Home`/`Ctrl+End` al principio/fin del documento.
- Escribir letras las inserta en el caret. `Enter` crea una línea nueva. `Backspace`/`Delete` borran.
- Mantener `Shift` con las flechas selecciona texto (se ve el fondo de selección).
- `Ctrl+Z`/`Ctrl+Y` deshacen/rehacen lo escrito.

- [x] **Step 8: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): edición con teclado, caret y selección"
```

---

### Task 7: Ratón: clic, arrastre y rueda

**Files:**
- Modify: `crates/notty-ui/src/render.rs`
- Modify: `crates/notty-ui/src/editor.rs`
- Modify: `crates/notty-ui/src/window.rs`

**Interfaces:**
- Consumes: `IDWriteTextLayout::HitTestPoint` (Direct2D/DirectWrite), `EditorState`.
- Produces: `pub fn char_index_at(&self, renderer: &Renderer, x: f32, y: f32) -> usize` en `EditorState` (traduce un punto del cliente a un índice de char del documento; no es testeable sin un `Renderer` real con `HWND`, así que se verifica solo manualmente), `EditorState::scroll_by(&mut self, delta_lines: i32)` (lógica pura, testable).

- [x] **Step 1: Escribir el test que falla (scroll_by, lógica pura)**

Añadir a `mod tests` en `crates/notty-ui/src/editor.rs`:

```rust
    #[test]
    fn scroll_by_moves_first_line_within_bounds() {
        let mut s = state("a\nb\nc\nd\ne\nf\n");
        s.viewport = Viewport { first_line: 0, visible_lines: 2 };
        s.scroll_by(3);
        assert_eq!(s.viewport.first_line, 3);
        s.scroll_by(-10);
        assert_eq!(s.viewport.first_line, 0);
        s.scroll_by(100);
        assert_eq!(s.viewport.first_line, s.doc.buffer().len_lines() - 1);
    }
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui scroll_by`
Expected: FAIL de compilación, `no method named scroll_by`.

- [x] **Step 3: Implementar scroll_by**

Añadir dentro del segundo `impl EditorState` (el de `apply`/`insert_char`, Task 6) en `crates/notty-ui/src/editor.rs`:

```rust
    pub fn scroll_by(&mut self, delta_lines: i32) {
        let total = self.doc.buffer().len_lines();
        let max_first = total.saturating_sub(1) as i64;
        let new_first = (self.viewport.first_line as i64 + delta_lines as i64).clamp(0, max_first);
        self.viewport.first_line = new_first as usize;
    }
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, 92 tests en el workspace.

- [x] **Step 5: Hit-testing (char_index_at) y conexión con el ratón**

En `crates/notty-ui/src/render.rs`, añadir un método a `Renderer`:

```rust
    /// Traduce un punto del cliente (en píxeles, origen arriba-izquierda) al índice de
    /// char del documento más cercano, usando `state.viewport` para saber qué línea es cada fila.
    pub fn char_index_at(&self, state: &EditorState, x: f32, y: f32) -> usize {
        let buf = state.doc.buffer();
        let total = buf.len_lines();
        let range = state.viewport.range(total);
        let row = ((y - PADDING_TOP) / self.line_height).floor().max(0.0) as usize;
        let line = (state.viewport.first_line + row).min(total.saturating_sub(1)).max(range.start);

        let start = buf.line_start(line);
        let end = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
        let text: String = buf.slice(start..end).trim_end_matches(['\r', '\n']).to_string();
        if text.is_empty() {
            return start;
        }
        let wide: Vec<u16> = text.encode_utf16().collect();
        let Ok(layout) = (unsafe { self._dwrite.CreateTextLayout(&wide, &self.text_format, f32::MAX, self.line_height) }) else {
            return start;
        };
        let mut trailing = windows::core::BOOL(0);
        let mut inside = windows::core::BOOL(0);
        let mut metrics = Default::default();
        if unsafe { layout.HitTestPoint(x - PADDING_X, 0.0, &mut trailing, &mut inside, &mut metrics) }.is_ok() {
            let utf16_offset = metrics.textPosition as usize + if trailing.as_bool() { 1 } else { 0 };
            let prefix = String::from_utf16_lossy(&wide[..utf16_offset.min(wide.len())]);
            start + prefix.chars().count()
        } else {
            start
        }
    }
```

Ajusta el nombre exacto de los campos de `DWRITE_HIT_TEST_METRICS` (`textPosition` u otro) y la firma de `HitTestPoint` al compilar; el comportamiento a mantener es: dado un punto (x, y) del cliente, devolver el índice de char más cercano dentro de la línea que cae en esa fila.

En `crates/notty-ui/src/window.rs`, añadir a `wndproc`:
- `WM_LBUTTONDOWN`: leer `(x, y)` de `lparam` (`LOWORD`/`HIWORD` como `i16`), llamar a `renderer.char_index_at(&state, x, y)`, `state.doc.set_cursor(idx)`, guardar que el botón está pulsado (un `bool` junto al resto del estado de ventana), capturar el ratón con `SetCapture(hwnd)`, `InvalidateRect`.
- `WM_MOUSEMOVE`: si el botón está pulsado, calcular `idx` igual que arriba y `state.doc.set_selection(anchor_guardado, idx)` (guarda `anchor` en `WM_LBUTTONDOWN` antes de mover el cursor), `InvalidateRect`.
- `WM_LBUTTONUP`: soltar el botón (`bool = false`) y `ReleaseCapture()`.
- `WM_MOUSEWHEEL`: leer el delta de `wparam` con `GET_WHEEL_DELTA_WPARAM` (alto de `wparam`, con signo), `state.scroll_by(-(delta / WHEEL_DELTA) * 3)` (3 líneas por muesca, como el resto de apps de Windows), `InvalidateRect`.

- [x] **Step 6: Comprobar manualmente**

Run: `cargo run --bin notty -- /tmp/notas.txt`
Expected (manual): clic en cualquier punto del texto mueve el caret ahí; arrastrar con el botón izquierdo selecciona; la rueda del ratón desplaza el documento cuando hay más líneas que las visibles.

- [x] **Step 7: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): clic, arrastre y rueda del ratón"
```

---

### Task 8: Portapapeles (copiar, cortar, pegar)

**Files:**
- Create: `crates/notty-ui/src/clipboard.rs`
- Modify: `crates/notty-ui/src/window.rs`
- Modify: `crates/notty-ui/src/lib.rs`
- Modify: `crates/notty-ui/Cargo.toml`

**Interfaces:**
- Consumes: nada de las tareas anteriores.
- Produces: `pub fn set_clipboard_text(hwnd: HWND, text: &str) -> windows::core::Result<()>`, `pub fn get_clipboard_text(hwnd: HWND) -> windows::core::Result<String>` en `notty-ui::clipboard`. `window.rs` conecta `EditorAction::Copy/Cut/Paste` (que Task 6 dejó sin manejar) a estas funciones.

- [x] **Step 1: Añadir las funcionalidades de portapapeles**

Run:

```bash
cargo add windows -p notty-ui --features Win32_System_DataExchange,Win32_System_Memory,Win32_System_Ole
```

- [x] **Step 2: Implementar clipboard.rs**

No lleva tests unitarios automáticos: tocar el portapapeles real del sistema desde `cargo test` sería frágil (comparte estado global con el resto de la máquina). Se verifica manualmente en el Step 4.

`crates/notty-ui/src/clipboard.rs`:

```rust
use windows::Win32::Foundation::HWND;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GHND, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::core::Result;

/// Pone `text` en el portapapeles como CF_UNICODETEXT (terminado en NUL, como espera Windows).
pub fn set_clipboard_text(hwnd: HWND, text: &str) -> Result<()> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * std::mem::size_of::<u16>();

    unsafe {
        OpenClipboard(hwnd)?;
        let result = (|| -> Result<()> {
            EmptyClipboard()?;
            let handle = GlobalAlloc(GHND, bytes)?;
            let ptr = GlobalLock(handle) as *mut u16;
            if ptr.is_null() {
                return Err(windows::core::Error::from_win32());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
            let _ = GlobalUnlock(handle);
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(windows::Win32::Foundation::HANDLE(handle.0)))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

/// Lee el portapapeles como texto. Si no hay texto Unicode, devuelve cadena vacía.
pub fn get_clipboard_text(hwnd: HWND) -> Result<String> {
    unsafe {
        OpenClipboard(hwnd)?;
        let result = (|| -> Result<String> {
            let Ok(handle) = GetClipboardData(CF_UNICODETEXT.0 as u32) else {
                return Ok(String::new());
            };
            let ptr = GlobalLock(windows::Win32::Foundation::HGLOBAL(handle.0)) as *const u16;
            if ptr.is_null() {
                return Ok(String::new());
            }
            let len_bytes = GlobalSize(windows::Win32::Foundation::HGLOBAL(handle.0));
            let len_u16 = len_bytes / std::mem::size_of::<u16>();
            let slice = std::slice::from_raw_parts(ptr, len_u16);
            let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
            let text = String::from_utf16_lossy(&slice[..end]);
            let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(handle.0));
            Ok(text)
        })();
        let _ = CloseClipboard();
        result
    }
}
```

Ajusta los tipos exactos (`HGLOBAL` vs `HANDLE`, si `GlobalAlloc`/`GlobalLock` devuelven `Result` o el valor directo) al compilar, manteniendo: abrir portapapeles, vaciarlo, reservar memoria global, copiar el texto UTF-16 con NUL final, publicarlo como `CF_UNICODETEXT`, cerrar portapapeles.

`crates/notty-ui/src/lib.rs` queda:

```rust
//! notty-ui: ventana Win32, render Direct2D/DirectWrite y lógica de edición en pantalla.

pub mod clipboard;
mod doc_io;
pub mod editor;
mod keymap;
pub mod render;
mod viewport;
pub mod window;

pub use doc_io::{OpenedDoc, open_as_document, save_document};
pub use editor::EditorState;
pub use keymap::{EditorAction, Modifiers, action_for_vk};
pub use render::Renderer;
pub use viewport::Viewport;
```

- [x] **Step 3: Conectar en window.rs**

En el `match action` de `WM_KEYDOWN` (Task 6), antes o después de llamar a `state.apply(...)`, interceptar los tres casos que `EditorState::apply` deja vacíos:

```rust
match action {
    crate::EditorAction::Copy | crate::EditorAction::Cut => {
        let sel = state.doc.selection();
        if !sel.is_empty() {
            let text = state.doc.buffer().slice(sel.range());
            let _ = crate::clipboard::set_clipboard_text(hwnd, &text);
            if matches!(action, crate::EditorAction::Cut) {
                state.doc.backspace(std::time::Instant::now()); // el rango ya es la selección
            }
        }
    }
    crate::EditorAction::Paste => {
        if let Ok(text) = crate::clipboard::get_clipboard_text(hwnd) {
            if !text.is_empty() {
                state.doc.insert(&text, std::time::Instant::now());
            }
        }
    }
    other => state.apply(other, std::time::Instant::now()),
}
```

(Nota: `Document::backspace` con una selección no vacía borra exactamente esa selección, según su implementación del Plan 1 — reutilizarlo para `Cut` es correcto y evita duplicar lógica.)

- [x] **Step 4: Comprobar manualmente**

Run: `cargo run --bin notty -- /tmp/notas.txt`
Expected (manual): seleccionar texto y `Ctrl+C`, pegarlo en el Bloc de notas de Windows muestra el mismo texto. Escribir algo en el Bloc de notas, copiarlo, y `Ctrl+V` en notty lo inserta. `Ctrl+X` corta la selección (desaparece del documento y aparece al pegar en otra app).

- [x] **Step 5: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): copiar, cortar y pegar con el portapapeles del sistema"
```

---

### Task 9: Guardar (Ctrl+S), título y barra de estado

**Files:**
- Modify: `crates/notty-ui/src/editor.rs`
- Modify: `crates/notty-ui/src/render.rs`
- Modify: `crates/notty-ui/src/window.rs`

**Interfaces:**
- Consumes: `save_document` (Task 3), `EditorState`.
- Produces: `EditorState::save(&mut self) -> Result<(), notty_io::CodecError>` (si `path` es `None`, devuelve `CodecError::Io("sin ruta".into())`: la línea de ruta llega en el Plan 4, así que por ahora un archivo sin ruta simplemente no se puede guardar con `Ctrl+S`), `Renderer::status_line(&self, state: &EditorState) -> String` (lógica pura y testable, sin FFI: da el texto `"Texto · Ln X, Col Y · ENC · EOL"` — luego `Renderer::paint` lo dibuja en una franja inferior).

- [x] **Step 1: Escribir los tests que fallan**

Añadir a `mod tests` en `crates/notty-ui/src/editor.rs`:

```rust
    #[test]
    fn save_without_path_is_an_error() {
        let mut s = state("hola");
        assert!(s.save().is_err());
    }

    #[test]
    fn save_writes_file_and_clears_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        let mut s = state("hola");
        s.path = Some(path.clone());
        assert!(s.doc.is_dirty());
        s.save().unwrap();
        assert!(!s.doc.is_dirty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hola");
    }
```

Añadir en un nuevo archivo de tests de lógica pura para la barra de estado. Como `Renderer` necesita una `HWND` real y no se puede crear en un test de `cargo test`, este dato se separa: crear `crates/notty-ui/src/status.rs` con la función pura:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use notty_core::Document;
    use notty_io::{LineEnding, TextEncoding};

    #[test]
    fn status_line_for_saved_text_file() {
        let mut doc = Document::new("hola\nmundo", "\n");
        doc.set_cursor(7);
        let s = status_line(&doc, TextEncoding::Utf8, LineEnding::Lf);
        assert_eq!(s, "Texto · Ln 2, Col 2 · UTF-8 · LF");
    }

    #[test]
    fn status_line_uses_1_based_line_and_col() {
        let doc = Document::new("abc", "\r\n");
        let s = status_line(&doc, TextEncoding::Windows1252, LineEnding::Crlf);
        assert_eq!(s, "Texto · Ln 1, Col 1 · ANSI (1252) · CRLF");
    }
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui`
Expected: FAIL de compilación (`no method named save`, `cannot find function status_line`).

- [x] **Step 3: Implementar**

Añadir dentro del segundo `impl EditorState` en `crates/notty-ui/src/editor.rs`:

```rust
    pub fn save(&mut self) -> Result<(), notty_io::CodecError> {
        let path = self.path.clone().ok_or_else(|| notty_io::CodecError::Io("sin ruta".to_string()))?;
        crate::save_document(&self.doc, &path, self.encoding)?;
        self.doc.mark_saved();
        Ok(())
    }
```

`crates/notty-ui/src/status.rs` (nuevo archivo, añadir **encima** del módulo de tests del Step 1):

```rust
use notty_core::Document;
use notty_io::{LineEnding, TextEncoding};

/// Texto de la barra de estado: "Texto · Ln <línea 1-based>, Col <col 1-based> · <codificación> · <CRLF/LF>".
pub fn status_line(doc: &Document, encoding: TextEncoding, eol: LineEnding) -> String {
    let (line, col) = doc.line_col();
    format!("Texto · Ln {}, Col {} · {} · {}", line + 1, col + 1, encoding.label(), eol.label())
}
```

Registrar el módulo en `crates/notty-ui/src/lib.rs`:

```rust
//! notty-ui: ventana Win32, render Direct2D/DirectWrite y lógica de edición en pantalla.

pub mod clipboard;
mod doc_io;
pub mod editor;
mod keymap;
pub mod render;
mod status;
mod viewport;
pub mod window;

pub use doc_io::{OpenedDoc, open_as_document, save_document};
pub use editor::EditorState;
pub use keymap::{EditorAction, Modifiers, action_for_vk};
pub use render::Renderer;
pub use status::status_line;
pub use viewport::Viewport;
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, 96 tests en el workspace.

- [x] **Step 5: Dibujar la barra de estado y conectar Ctrl+S**

En `crates/notty-ui/src/render.rs`, dentro de `paint`, reservar una franja de `line_height` de alto al final del área de cliente (antes de eso, reducir en esa misma altura el número de líneas de texto que se dibujan, para que la barra no tape la última línea visible — o simplemente dibujarla encima al final, ya que `Renderer::resize`/`Viewport::new` ya reservan ese hueco si le restas `line_height` al alto del cliente al calcularlo en `window.rs`). Dibujar el resultado de `crate::status_line(&state.doc, state.encoding, state.eol)` con un `IDWriteTextLayout` en la esquina inferior derecha, con el mismo `text_format` pero un tamaño más pequeño si se quiere (opcional; para v1 basta con reutilizar `text_format`).

En `window.rs`:
- En el `match action` de `WM_KEYDOWN`, añadir el caso `crate::EditorAction::Save => { let _ = state.save(); }` antes del `other => state.apply(...)` genérico.
- Actualizar el título de la ventana (`SetWindowTextW`) cada vez que cambie `state.doc.is_dirty()`: tras cualquier acción que edite el documento, si `state.doc.is_dirty()` añadir `" •"` al título ya usado en la Task 4 (`"<ruta> · notty"` → `"<ruta> • · notty"` mientras esté sucio); tras guardar, volver al título sin `•`. La forma más simple: recalcular el título entero en cada `InvalidateRect` justo antes de llamarlo, con una función `fn window_title(state: &EditorState) -> String` en `window.rs` que arme `"<nombre o 'sin título'>{ ' •' si sucio} · notty"`.

- [x] **Step 6: Comprobar manualmente**

Run: `cargo run --bin notty -- /tmp/notas.txt`
Expected (manual):
- La esquina inferior derecha muestra algo como `Texto · Ln 1, Col 1 · UTF-8 · CRLF` y cambia al mover el caret.
- Escribir algo hace aparecer `•` en el título de la ventana.
- `Ctrl+S` guarda (comprobar abriendo el archivo con otro programa) y el `•` desaparece del título.
- Cerrar y reabrir con `cargo run --bin notty -- /tmp/notas.txt` muestra los cambios guardados.

- [x] **Step 7: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): guardar con Ctrl+S, título y barra de estado"
```

---

### Task 10: Comprobación final del workspace

**Files:** ninguno nuevo; solo verificación.

- [x] **Step 1: Tests y lints de todo el workspace**

Run: `cargo test --workspace`
Expected: PASS, 96 tests (58 del Plan 1 + 38 nuevos de este plan).

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: sin avisos. Corregir cualquier aviso en el archivo que lo señale y repetir hasta que quede limpio.

- [x] **Step 2: Arranque en frío**

Run: `cargo build --release --workspace`
Expected: build en modo release sin errores.

Run (PowerShell, medir arranque; ajusta la ruta del exe si el layout de target difiere): `Measure-Command { Start-Process -FilePath "target\release\notty.exe" -ArgumentList "/tmp/notas.txt" -PassThru | Wait-Process -Timeout 2 }`
Expected: informativo (el presupuesto de 50 ms se exige formalmente en el Plan 7 con CI; aquí basta confirmar que arranca y no cuelga).

- [x] **Step 3: Commit (si hubo cambios de la revisión)**

```bash
git add -A
git commit -m "chore(ui): pasa clippy y build release en todo el workspace" --allow-empty
```

---

## Hoja de ruta: planes siguientes

| Plan | Contenido |
|---|---|
| **3 · Config, comandos y ventana** | `notty-config`, `notty-input`, piezas de ventana (pestañas/buffers, menús, barra nano, números de línea), ventana de Ajustes. |
| **4 · Línea de ruta y búsqueda** | Prompts en la barra de estado: ruta con sugerencias/`Tab`, buscar/reemplazar con `Ctrl+F`/`Ctrl+H`, `F3`, `Ctrl+G`, `Ctrl+D`. |
| **5 · Vim y raw** | Keymap vim, vista hexadecimal sobre `RawBytes`. |
| **6 · Archivos vivos** | Temporales, instancia única, daemon/atajo global, autoguardado, conflictos, recuperación tras caída. |
| **7 · Pulido y garantías** | UI Automation, alto contraste/DPI, fuzzing, presupuesto de rendimiento en CI. |
