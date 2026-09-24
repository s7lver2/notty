# notty · Plan 1: Núcleo (notty-core + notty-io) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Construir las dos librerías sin UI de notty: el documento editable (buffer, selección, deshacer agrupado, búsqueda) y la E/S de archivos (detección texto/raw, codificaciones, fin de línea, guardado atómico, mmap).

**Architecture:** Workspace de Rust con `crates/*`. `notty-core` no depende de nada de Windows: un `Buffer` sobre `ropey`, `Edit` reversible, `History` con grupos e ids, `Document` que lo une todo, y búsqueda con `regex`. `notty-io` decide si un archivo es texto o raw a partir de los primeros 8 KB, decodifica de forma estricta (con modo lossy de solo lectura como red de seguridad) y guarda siempre de forma atómica. El texto se guarda **tal cual** en el buffer, con sus `\r\n`, para que abrir y guardar sea sin pérdidas.

**Tech Stack:** Rust stable 1.96 (MSVC), edition 2024, `ropey` 1.6, `regex` 1, `thiserror` 2, `encoding_rs` 0.8, `memmap2` 0.9, `tempfile` 3 (solo tests).

**Spec:** `docs/superpowers/specs/2026-09-24-notty-design.md`

## Global Constraints

- Solo Windows 10/11; toolchain `stable-x86_64-pc-windows-msvc`.
- `notty-core` **no** puede depender de `windows-rs` ni de nada específico de Windows.
- Nunca perder datos: guardado siempre atómico (`archivo.tmp~` → flush → renombrar); una codificación dudosa nunca se "arregla" en silencio (se abre en solo lectura).
- Se conservan codificación, BOM y CRLF/LF originales al guardar salvo cambio explícito.
- Detección: texto si hay BOM, o UTF-8 válido sin bytes nulos en los primeros ~8 KB; todo lo demás es raw.
- Deshacer por grupos (ráfagas de escritura), no letra a letra.
- Textos visibles para el usuario en español.
- Fuera siempre: IA, nube, plugins.
- Convención del usuario: un commit por tarea terminada (cada tarea es una unidad completa).

## File Structure

```
Cargo.toml                          workspace (members = crates/*), deps compartidas, perfil release
crates/notty-core/Cargo.toml
crates/notty-core/src/lib.rs        re-exports
crates/notty-core/src/buffer.rs     Buffer: rope, índices de char, línea/columna
crates/notty-core/src/edit.rs       Edit: cambio reversible (apply / inverse)
crates/notty-core/src/history.rs    History: grupos de edits, undo/redo, ids para "sucio"
crates/notty-core/src/document.rs   Document + Selection: operaciones de edición de alto nivel
crates/notty-core/src/search.rs     find_all / replace_all + SearchOptions
crates/notty-io/Cargo.toml
crates/notty-io/src/lib.rs          re-exports
crates/notty-io/src/encoding.rs     TextEncoding, detect, decode, decode_lossy, encode
crates/notty-io/src/eol.rs          LineEnding, detect_eol, convert
crates/notty-io/src/fsutil.rs       atomic_write, create_parent_dirs, can_write
crates/notty-io/src/open.rs         open / open_raw → Opened (Text | Raw)
```

Los tests van en un módulo `#[cfg(test)] mod tests` al final de cada archivo.

---

### Task 1: Workspace + Buffer

**Files:**
- Create: `Cargo.toml`
- Create: `crates/notty-core/Cargo.toml`
- Create: `crates/notty-core/src/lib.rs`
- Create: `crates/notty-core/src/buffer.rs`

**Interfaces:**
- Consumes: nada.
- Produces: `notty_core::Buffer` con `Buffer::new(&str)`, `len_chars() -> usize`, `len_lines() -> usize`, `insert(usize, &str)`, `remove(Range<usize>)`, `slice(Range<usize>) -> String`, `line_col(usize) -> (usize, usize)` (0-based), `line_start(usize) -> usize`, `byte_to_char(usize) -> usize`, `Display` (texto completo). Todos los índices son **de char**.

- [x] **Step 1: Crear el workspace**

`Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
edition = "2024"
rust-version = "1.85"

[workspace.dependencies]
ropey = "1.6"
regex = "1"
thiserror = "2"
encoding_rs = "0.8"
memmap2 = "0.9"
tempfile = "3"

[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
strip = true
```

`crates/notty-core/Cargo.toml`:

```toml
[package]
name = "notty-core"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
ropey.workspace = true
regex.workspace = true
thiserror.workspace = true
```

`crates/notty-core/src/lib.rs`:

```rust
//! notty-core: buffer, edición, historial y búsqueda. Sin nada de Windows.

mod buffer;

pub use buffer::Buffer;
```

- [x] **Step 2: Escribir los tests que fallan**

`crates/notty-core/src/buffer.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_remove_round_trip() {
        let mut b = Buffer::new("hola");
        b.insert(4, " mundo");
        assert_eq!(b.to_string(), "hola mundo");
        b.remove(0..5);
        assert_eq!(b.to_string(), "mundo");
    }

    #[test]
    fn crlf_counts_as_one_line_break() {
        let b = Buffer::new("ab\r\ncd");
        assert_eq!(b.len_lines(), 2);
        assert_eq!(b.line_col(1), (0, 1));
        assert_eq!(b.line_col(4), (1, 0));
        assert_eq!(b.line_start(1), 4);
    }

    #[test]
    fn empty_buffer_has_one_line() {
        assert_eq!(Buffer::new("").len_lines(), 1);
    }

    #[test]
    fn byte_to_char_handles_multibyte() {
        let b = Buffer::new("ña");
        assert_eq!(b.byte_to_char(2), 1);
        assert_eq!(b.slice(0..1), "ñ");
    }
}
```

- [x] **Step 3: Ejecutar y ver que falla**

Run: `cargo test -p notty-core`
Expected: FAIL de compilación, `cannot find type Buffer`.

- [x] **Step 4: Implementar Buffer**

Añadir **encima** del módulo de tests en `crates/notty-core/src/buffer.rs`:

```rust
use std::fmt;
use std::ops::Range;

use ropey::Rope;

/// Texto del documento. Todos los índices son de char, no de byte.
/// Guarda los saltos de línea tal cual (`\r\n` o `\n`); ropey cuenta `\r\n` como uno.
#[derive(Debug, Clone, Default)]
pub struct Buffer {
    rope: Rope,
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        Self { rope: Rope::from_str(text) }
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        self.rope.insert(char_idx, text);
    }

    pub fn remove(&mut self, range: Range<usize>) {
        if !range.is_empty() {
            self.rope.remove(range);
        }
    }

    pub fn slice(&self, range: Range<usize>) -> String {
        self.rope.slice(range).to_string()
    }

    /// (línea, columna), ambas empezando en 0.
    pub fn line_col(&self, char_idx: usize) -> (usize, usize) {
        let line = self.rope.char_to_line(char_idx);
        (line, char_idx - self.rope.line_to_char(line))
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.rope.line_to_char(line.min(self.len_lines() - 1))
    }

    pub fn byte_to_char(&self, byte_idx: usize) -> usize {
        self.rope.byte_to_char(byte_idx)
    }
}

impl fmt::Display for Buffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for chunk in self.rope.chunks() {
            f.write_str(chunk)?;
        }
        Ok(())
    }
}
```

- [x] **Step 5: Ejecutar y ver que pasa**

Run: `cargo test -p notty-core`
Expected: PASS, 4 tests.

- [x] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/notty-core
git commit -m "feat(core): workspace y Buffer sobre ropey"
```

---

### Task 2: Edit + History (deshacer agrupado)

**Files:**
- Create: `crates/notty-core/src/edit.rs`
- Create: `crates/notty-core/src/history.rs`
- Modify: `crates/notty-core/src/lib.rs`

**Interfaces:**
- Consumes: `Buffer` (Task 1).
- Produces:
  - `pub struct Edit { pub at: usize, pub removed: String, pub inserted: String }` con `apply(&self, &mut Buffer)` y `inverse(&self) -> Edit`.
  - `History` (`Default`) con `record(Edit, Instant)`, `seal()`, `top_id() -> Option<u64>`, `undo(&mut Buffer) -> Option<usize>`, `redo(&mut Buffer) -> Option<usize>`. `undo`/`redo` devuelven la posición del cursor.
  - Regla de agrupado: se une al grupo anterior si no está sellado, han pasado < 1 s, y es escritura contigua sin salto de línea o borrado hacia atrás contiguo.

- [ ] **Step 1: Escribir Edit**

`crates/notty-core/src/edit.rs`:

```rust
use crate::Buffer;

/// Un cambio reversible: en `at` se quitó `removed` y se puso `inserted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub at: usize,
    pub removed: String,
    pub inserted: String,
}

impl Edit {
    pub fn apply(&self, buf: &mut Buffer) {
        let n = self.removed.chars().count();
        buf.remove(self.at..self.at + n);
        buf.insert(self.at, &self.inserted);
    }

    pub fn inverse(&self) -> Edit {
        Edit { at: self.at, removed: self.inserted.clone(), inserted: self.removed.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_undoes_apply() {
        let mut b = Buffer::new("hola mundo");
        let e = Edit { at: 5, removed: "mundo".into(), inserted: "notty".into() };
        e.apply(&mut b);
        assert_eq!(b.to_string(), "hola notty");
        e.inverse().apply(&mut b);
        assert_eq!(b.to_string(), "hola mundo");
    }
}
```

`crates/notty-core/src/lib.rs` queda:

```rust
//! notty-core: buffer, edición, historial y búsqueda. Sin nada de Windows.

mod buffer;
mod edit;
mod history;

pub use buffer::Buffer;
pub use edit::Edit;
pub use history::History;
```

- [ ] **Step 2: Escribir los tests de History que fallan**

`crates/notty-core/src/history.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn type_at(b: &mut Buffer, h: &mut History, at: usize, s: &str, now: Instant) {
        let e = Edit { at, removed: String::new(), inserted: s.into() };
        e.apply(b);
        h.record(e, now);
    }

    #[test]
    fn fast_typing_is_one_undo_step() {
        let (mut b, mut h, t) = (Buffer::new(""), History::default(), Instant::now());
        type_at(&mut b, &mut h, 0, "a", t);
        type_at(&mut b, &mut h, 1, "b", t + ms(200));
        assert_eq!(h.undo(&mut b), Some(0));
        assert_eq!(b.to_string(), "");
        assert_eq!(h.undo(&mut b), None);
    }

    #[test]
    fn pause_starts_new_group() {
        let (mut b, mut h, t) = (Buffer::new(""), History::default(), Instant::now());
        type_at(&mut b, &mut h, 0, "a", t);
        type_at(&mut b, &mut h, 1, "b", t + ms(1500));
        h.undo(&mut b);
        assert_eq!(b.to_string(), "a");
    }

    #[test]
    fn seal_starts_new_group() {
        let (mut b, mut h, t) = (Buffer::new(""), History::default(), Instant::now());
        type_at(&mut b, &mut h, 0, "a", t);
        h.seal();
        type_at(&mut b, &mut h, 1, "b", t);
        h.undo(&mut b);
        assert_eq!(b.to_string(), "a");
    }

    #[test]
    fn newline_starts_new_group() {
        let (mut b, mut h, t) = (Buffer::new(""), History::default(), Instant::now());
        type_at(&mut b, &mut h, 0, "a", t);
        type_at(&mut b, &mut h, 1, "\r\n", t);
        h.undo(&mut b);
        assert_eq!(b.to_string(), "a");
    }

    #[test]
    fn backspaces_group_and_restore_cursor() {
        let (mut b, mut h, t) = (Buffer::new("abc"), History::default(), Instant::now());
        for (at, ch) in [(2, "c"), (1, "b")] {
            let e = Edit { at, removed: ch.into(), inserted: String::new() };
            e.apply(&mut b);
            h.record(e, t);
        }
        assert_eq!(b.to_string(), "a");
        assert_eq!(h.undo(&mut b), Some(3));
        assert_eq!(b.to_string(), "abc");
    }

    #[test]
    fn redo_reapplies_and_new_edit_clears_redo() {
        let (mut b, mut h, t) = (Buffer::new(""), History::default(), Instant::now());
        type_at(&mut b, &mut h, 0, "ab", t);
        h.undo(&mut b);
        assert_eq!(h.redo(&mut b), Some(2));
        assert_eq!(b.to_string(), "ab");
        h.undo(&mut b);
        type_at(&mut b, &mut h, 0, "x", t);
        assert_eq!(h.redo(&mut b), None);
    }

    #[test]
    fn top_id_follows_undo_and_redo() {
        let (mut b, mut h, t) = (Buffer::new(""), History::default(), Instant::now());
        assert_eq!(h.top_id(), None);
        type_at(&mut b, &mut h, 0, "a", t);
        let id = h.top_id();
        assert!(id.is_some());
        h.undo(&mut b);
        assert_eq!(h.top_id(), None);
        h.redo(&mut b);
        assert_eq!(h.top_id(), id);
    }
}
```

- [ ] **Step 3: Ejecutar y ver que falla**

Run: `cargo test -p notty-core`
Expected: FAIL de compilación, `cannot find type History`.

- [ ] **Step 4: Implementar History**

Añadir **encima** del módulo de tests en `crates/notty-core/src/history.rs`:

```rust
use std::time::{Duration, Instant};

use crate::{Buffer, Edit};

const GROUP_WINDOW: Duration = Duration::from_millis(1000);

#[derive(Debug)]
struct Group {
    id: u64,
    edits: Vec<Edit>,
}

/// Deshacer/rehacer por grupos. Cada grupo tiene un id único: el documento
/// compara `top_id()` con el id guardado para saber si está "sucio".
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Group>,
    redo: Vec<Group>,
    next_id: u64,
    last: Option<Instant>,
    sealed: bool,
}

impl History {
    pub fn record(&mut self, edit: Edit, now: Instant) {
        self.redo.clear();
        let recent = self.last.is_some_and(|t| now.saturating_duration_since(t) < GROUP_WINDOW);
        let joins = !self.sealed
            && recent
            && self.undo.last().and_then(|g| g.edits.last()).is_some_and(|prev| continues(prev, &edit));
        if joins {
            self.undo.last_mut().expect("joins implica grupo").edits.push(edit);
        } else {
            self.next_id += 1;
            self.undo.push(Group { id: self.next_id, edits: vec![edit] });
        }
        self.last = Some(now);
        self.sealed = false;
    }

    /// Corta el grupo actual (mover el cursor, guardar, deshacer...).
    pub fn seal(&mut self) {
        self.sealed = true;
    }

    pub fn top_id(&self) -> Option<u64> {
        self.undo.last().map(|g| g.id)
    }

    pub fn undo(&mut self, buf: &mut Buffer) -> Option<usize> {
        let group = self.undo.pop()?;
        for e in group.edits.iter().rev() {
            e.inverse().apply(buf);
        }
        let first = &group.edits[0];
        let cursor = first.at + first.removed.chars().count();
        self.redo.push(group);
        self.sealed = true;
        Some(cursor)
    }

    pub fn redo(&mut self, buf: &mut Buffer) -> Option<usize> {
        let group = self.redo.pop()?;
        for e in &group.edits {
            e.apply(buf);
        }
        let last = group.edits.last().expect("grupo no vacío");
        let cursor = last.at + last.inserted.chars().count();
        self.undo.push(group);
        self.sealed = true;
        Some(cursor)
    }
}

fn continues(prev: &Edit, next: &Edit) -> bool {
    let typing = prev.removed.is_empty()
        && next.removed.is_empty()
        && next.at == prev.at + prev.inserted.chars().count()
        && !next.inserted.contains('\n');
    let erasing = prev.inserted.is_empty()
        && next.inserted.is_empty()
        && next.at + next.removed.chars().count() == prev.at;
    typing || erasing
}
```

- [ ] **Step 5: Ejecutar y ver que pasa**

Run: `cargo test -p notty-core`
Expected: PASS, 12 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/notty-core
git commit -m "feat(core): Edit reversible e historial con grupos"
```

---

### Task 3: Document + Selection

**Files:**
- Create: `crates/notty-core/src/document.rs`
- Modify: `crates/notty-core/src/lib.rs`

**Interfaces:**
- Consumes: `Buffer`, `Edit`, `History`.
- Produces:
  - `Selection { pub anchor: usize, pub head: usize }` con `caret(usize)`, `range() -> Range<usize>`, `is_empty()`.
  - `Document::new(text: &str, newline: &'static str)` y métodos: `text() -> String`, `buffer() -> &Buffer`, `selection() -> Selection`, `set_selection(anchor, head)`, `set_cursor(usize)`, `line_col() -> (usize, usize)`, `newline() -> &'static str`, `set_newline(&'static str)`, `is_dirty() -> bool`, `mark_saved()`, `insert(&str, Instant)`, `insert_newline(Instant)`, `replace_range(Range<usize>, &str, Instant)`, `backspace(Instant)`, `delete_forward(Instant)`, `undo() -> bool`, `redo() -> bool`.
  - `newline` es `"\r\n"` o `"\n"`; la UI lo sacará de `notty_io::LineEnding::as_str()`.

- [ ] **Step 1: Escribir los tests que fallan**

`crates/notty-core/src/document.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn typing_then_undo_restores_text() {
        let mut d = Document::new("", "\r\n");
        let t = Instant::now();
        for (i, c) in "hola".chars().enumerate() {
            d.insert(&c.to_string(), t + Duration::from_millis(i as u64 * 100));
        }
        assert_eq!(d.text(), "hola");
        assert!(d.undo());
        assert_eq!(d.text(), "");
        assert_eq!(d.selection(), Selection::caret(0));
    }

    #[test]
    fn insert_replaces_selection() {
        let mut d = Document::new("hola mundo", "\r\n");
        d.set_selection(5, 10);
        d.insert("notty", Instant::now());
        assert_eq!(d.text(), "hola notty");
        assert_eq!(d.selection(), Selection::caret(10));
    }

    #[test]
    fn newline_uses_document_line_ending() {
        let mut d = Document::new("ab", "\r\n");
        d.set_cursor(1);
        d.insert_newline(Instant::now());
        assert_eq!(d.text(), "a\r\nb");
        assert_eq!(d.selection(), Selection::caret(3));
    }

    #[test]
    fn backspace_removes_whole_crlf() {
        let mut d = Document::new("a\r\nb", "\r\n");
        d.set_cursor(3);
        d.backspace(Instant::now());
        assert_eq!(d.text(), "ab");
        assert_eq!(d.selection(), Selection::caret(1));
    }

    #[test]
    fn delete_forward_removes_whole_crlf() {
        let mut d = Document::new("a\r\nb", "\r\n");
        d.set_cursor(1);
        d.delete_forward(Instant::now());
        assert_eq!(d.text(), "ab");
    }

    #[test]
    fn backspace_at_start_does_nothing() {
        let mut d = Document::new("a", "\n");
        d.set_cursor(0);
        d.backspace(Instant::now());
        assert_eq!(d.text(), "a");
        assert!(!d.is_dirty());
    }

    #[test]
    fn dirty_tracks_saved_point() {
        let mut d = Document::new("", "\n");
        let t = Instant::now();
        assert!(!d.is_dirty());
        d.insert("a", t);
        assert!(d.is_dirty());
        d.mark_saved();
        assert!(!d.is_dirty());
        d.insert("b", t);
        assert!(d.is_dirty());
        d.undo();
        assert!(!d.is_dirty());
        d.redo();
        assert!(d.is_dirty());
    }

    #[test]
    fn cursor_is_clamped() {
        let mut d = Document::new("ab", "\n");
        d.set_cursor(99);
        assert_eq!(d.selection(), Selection::caret(2));
        assert_eq!(d.line_col(), (0, 2));
    }
}
```

- [ ] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-core document`
Expected: FAIL de compilación, `cannot find type Document`.

- [ ] **Step 3: Implementar Document**

Añadir **encima** del módulo de tests en `crates/notty-core/src/document.rs`:

```rust
use std::ops::Range;
use std::time::Instant;

use crate::{Buffer, Edit, History};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn caret(idx: usize) -> Self {
        Self { anchor: idx, head: idx }
    }

    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// Un archivo abierto en memoria: texto, selección e historial.
#[derive(Debug)]
pub struct Document {
    buffer: Buffer,
    history: History,
    sel: Selection,
    newline: &'static str,
    saved: Option<u64>,
}

impl Document {
    pub fn new(text: &str, newline: &'static str) -> Self {
        Self { buffer: Buffer::new(text), history: History::default(), sel: Selection::default(), newline, saved: None }
    }

    pub fn text(&self) -> String {
        self.buffer.to_string()
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn selection(&self) -> Selection {
        self.sel
    }

    pub fn set_selection(&mut self, anchor: usize, head: usize) {
        let n = self.buffer.len_chars();
        self.sel = Selection { anchor: anchor.min(n), head: head.min(n) };
        self.history.seal();
    }

    pub fn set_cursor(&mut self, idx: usize) {
        self.set_selection(idx, idx);
    }

    pub fn line_col(&self) -> (usize, usize) {
        self.buffer.line_col(self.sel.head)
    }

    pub fn newline(&self) -> &'static str {
        self.newline
    }

    pub fn set_newline(&mut self, newline: &'static str) {
        self.newline = newline;
    }

    pub fn is_dirty(&self) -> bool {
        self.history.top_id() != self.saved
    }

    pub fn mark_saved(&mut self) {
        self.saved = self.history.top_id();
        self.history.seal();
    }

    pub fn insert(&mut self, text: &str, now: Instant) {
        let range = self.sel.range();
        self.replace_range(range, text, now);
    }

    pub fn insert_newline(&mut self, now: Instant) {
        let nl = self.newline;
        self.insert(nl, now);
    }

    pub fn replace_range(&mut self, range: Range<usize>, text: &str, now: Instant) {
        let removed = self.buffer.slice(range.clone());
        if removed.is_empty() && text.is_empty() {
            return;
        }
        let edit = Edit { at: range.start, removed, inserted: text.to_string() };
        edit.apply(&mut self.buffer);
        self.sel = Selection::caret(range.start + text.chars().count());
        self.history.record(edit, now);
    }

    pub fn backspace(&mut self, now: Instant) {
        if !self.sel.is_empty() {
            let range = self.sel.range();
            return self.replace_range(range, "", now);
        }
        let c = self.sel.head;
        if c == 0 {
            return;
        }
        let start = if c >= 2 && self.buffer.slice(c - 2..c) == "\r\n" { c - 2 } else { c - 1 };
        self.replace_range(start..c, "", now);
    }

    pub fn delete_forward(&mut self, now: Instant) {
        if !self.sel.is_empty() {
            let range = self.sel.range();
            return self.replace_range(range, "", now);
        }
        let (c, n) = (self.sel.head, self.buffer.len_chars());
        if c >= n {
            return;
        }
        let end = if c + 2 <= n && self.buffer.slice(c..c + 2) == "\r\n" { c + 2 } else { c + 1 };
        self.replace_range(c..end, "", now);
    }

    pub fn undo(&mut self) -> bool {
        match self.history.undo(&mut self.buffer) {
            Some(c) => {
                self.sel = Selection::caret(c);
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.history.redo(&mut self.buffer) {
            Some(c) => {
                self.sel = Selection::caret(c);
                true
            }
            None => false,
        }
    }
}
```

`crates/notty-core/src/lib.rs` queda:

```rust
//! notty-core: buffer, edición, historial y búsqueda. Sin nada de Windows.

mod buffer;
mod document;
mod edit;
mod history;

pub use buffer::Buffer;
pub use document::{Document, Selection};
pub use edit::Edit;
pub use history::History;
```

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test -p notty-core`
Expected: PASS, 20 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/notty-core
git commit -m "feat(core): Document con selección, CRLF y estado sucio"
```

---

### Task 4: Búsqueda y reemplazo

**Files:**
- Create: `crates/notty-core/src/search.rs`
- Modify: `crates/notty-core/src/document.rs` (nuevo bloque `impl Document` + tests)
- Modify: `crates/notty-core/src/lib.rs`

**Interfaces:**
- Consumes: `Document`, `Buffer::byte_to_char`.
- Produces:
  - `SearchOptions { pub case_sensitive: bool, pub whole_word: bool, pub regex: bool }` (`Default` = todo `false`, es decir, sin distinguir mayúsculas, texto literal).
  - `SearchError::BadRegex(String)`.
  - `find_all(text: &str, query: &str, SearchOptions) -> Result<Vec<Range<usize>>, SearchError>` en **bytes**.
  - `replace_all(text, query, replacement, SearchOptions) -> Result<(String, usize), SearchError>`; en modo literal `$1` no se expande; en modo regex sí.
  - `Document::find_all(&self, query, opts) -> Result<Vec<Range<usize>>, SearchError>` en **chars**.
  - `Document::replace_all(&mut self, query, replacement, opts, Instant) -> Result<usize, SearchError>`: un único paso de deshacer.

- [ ] **Step 1: Escribir los tests que fallan**

`crates/notty-core/src/search.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> SearchOptions {
        SearchOptions::default()
    }

    #[test]
    fn default_search_ignores_case() {
        assert_eq!(find_all("Juan y juan", "juan", opts()).unwrap(), vec![0..4, 7..11]);
    }

    #[test]
    fn case_sensitive_search() {
        let o = SearchOptions { case_sensitive: true, ..opts() };
        assert_eq!(find_all("Juan y juan", "juan", o).unwrap(), vec![7..11]);
    }

    #[test]
    fn whole_word_search() {
        let o = SearchOptions { whole_word: true, ..opts() };
        assert_eq!(find_all("pan panadero pan", "pan", o).unwrap(), vec![0..3, 13..16]);
    }

    #[test]
    fn literal_by_default() {
        assert_eq!(find_all("a.b axb", "a.b", opts()).unwrap(), vec![0..3]);
    }

    #[test]
    fn regex_mode() {
        let o = SearchOptions { regex: true, ..opts() };
        assert_eq!(find_all("a1 b22", r"\d+", o).unwrap(), vec![1..2, 4..6]);
    }

    #[test]
    fn bad_regex_is_an_error() {
        let o = SearchOptions { regex: true, ..opts() };
        assert!(matches!(find_all("x", "(", o), Err(SearchError::BadRegex(_))));
    }

    #[test]
    fn empty_query_finds_nothing() {
        assert!(find_all("abc", "", opts()).unwrap().is_empty());
    }

    #[test]
    fn replace_all_literal_does_not_expand() {
        assert_eq!(replace_all("a b a", "a", "$1", opts()).unwrap(), ("$1 b $1".to_string(), 2));
    }

    #[test]
    fn replace_all_regex_expands_groups() {
        let o = SearchOptions { regex: true, ..opts() };
        assert_eq!(replace_all("2026-09", r"(\d+)-(\d+)", "$2/$1", o).unwrap(), ("09/2026".to_string(), 1));
    }
}
```

Añadir al final del módulo de tests de `crates/notty-core/src/document.rs` (dentro de `mod tests`):

```rust
    #[test]
    fn find_all_returns_char_ranges() {
        let d = Document::new("ñandú ñ", "\n");
        assert_eq!(d.find_all("ñ", crate::SearchOptions::default()).unwrap(), vec![0..1, 6..7]);
    }

    #[test]
    fn replace_all_is_one_undo_step() {
        let mut d = Document::new("a b a", "\n");
        let n = d.replace_all("a", "x", crate::SearchOptions::default(), Instant::now()).unwrap();
        assert_eq!(n, 2);
        assert_eq!(d.text(), "x b x");
        d.undo();
        assert_eq!(d.text(), "a b a");
    }
```

- [ ] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-core`
Expected: FAIL de compilación, `cannot find function find_all` / `no method named find_all`.

- [ ] **Step 3: Implementar la búsqueda**

Añadir **encima** del módulo de tests en `crates/notty-core/src/search.rs`:

```rust
use std::ops::Range;

use regex::{NoExpand, Regex, RegexBuilder};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SearchError {
    #[error("regex no válida: {0}")]
    BadRegex(String),
}

fn build(query: &str, opts: SearchOptions) -> Result<Regex, SearchError> {
    let mut pattern = if opts.regex { query.to_string() } else { regex::escape(query) };
    if opts.whole_word {
        pattern = format!(r"\b(?:{pattern})\b");
    }
    RegexBuilder::new(&pattern)
        .case_insensitive(!opts.case_sensitive)
        .build()
        .map_err(|e| SearchError::BadRegex(e.to_string()))
}

/// Coincidencias como rangos de **bytes** sobre `text`. Ignora coincidencias vacías.
pub fn find_all(text: &str, query: &str, opts: SearchOptions) -> Result<Vec<Range<usize>>, SearchError> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let re = build(query, opts)?;
    Ok(re.find_iter(text).filter(|m| !m.is_empty()).map(|m| m.range()).collect())
}

pub fn replace_all(text: &str, query: &str, replacement: &str, opts: SearchOptions) -> Result<(String, usize), SearchError> {
    let count = find_all(text, query, opts)?.len();
    if count == 0 {
        return Ok((text.to_string(), 0));
    }
    let re = build(query, opts)?;
    let out = if opts.regex {
        re.replace_all(text, replacement).into_owned()
    } else {
        re.replace_all(text, NoExpand(replacement)).into_owned()
    };
    Ok((out, count))
}
```

Añadir en `crates/notty-core/src/document.rs`, después del primer `impl Document` y antes de los tests:

```rust
impl Document {
    /// Coincidencias como rangos de **chars**, listos para seleccionar o resaltar.
    pub fn find_all(&self, query: &str, opts: crate::SearchOptions) -> Result<Vec<Range<usize>>, crate::SearchError> {
        let text = self.text();
        Ok(crate::find_all(&text, query, opts)?
            .into_iter()
            .map(|r| self.buffer.byte_to_char(r.start)..self.buffer.byte_to_char(r.end))
            .collect())
    }

    /// Reemplaza todas las coincidencias como un único paso de deshacer.
    pub fn replace_all(&mut self, query: &str, replacement: &str, opts: crate::SearchOptions, now: Instant) -> Result<usize, crate::SearchError> {
        let text = self.text();
        let (new_text, count) = crate::replace_all(&text, query, replacement, opts)?;
        if count > 0 {
            self.history.seal();
            let len = self.buffer.len_chars();
            self.replace_range(0..len, &new_text, now);
            self.history.seal();
        }
        Ok(count)
    }
}
```

`crates/notty-core/src/lib.rs` queda:

```rust
//! notty-core: buffer, edición, historial y búsqueda. Sin nada de Windows.

mod buffer;
mod document;
mod edit;
mod history;
mod search;

pub use buffer::Buffer;
pub use document::{Document, Selection};
pub use edit::Edit;
pub use history::History;
pub use search::{SearchError, SearchOptions, find_all, replace_all};
```

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test -p notty-core`
Expected: PASS, 31 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/notty-core
git commit -m "feat(core): búsqueda y reemplazo (Aa, palabra, regex)"
```

---

### Task 5: notty-io: codificaciones y detección texto/raw

**Files:**
- Create: `crates/notty-io/Cargo.toml`
- Create: `crates/notty-io/src/lib.rs`
- Create: `crates/notty-io/src/encoding.rs`

**Interfaces:**
- Consumes: nada.
- Produces:
  - `TextEncoding { Utf8, Utf8Bom, Utf16Le, Utf16Be, Windows1252 }` con `label() -> &'static str` ("UTF-8", "UTF-8 con BOM", "UTF-16 LE", "UTF-16 BE", "ANSI (1252)").
  - `Detected { Text(TextEncoding), Raw }` y `detect(sample: &[u8]) -> Detected`.
  - `CodecError { Invalid(&'static str), Unmappable(&'static str) }`.
  - `decode(&[u8], TextEncoding) -> Result<String, CodecError>` (estricto, quita el BOM), `decode_lossy(&[u8], TextEncoding) -> String`, `encode(&str, TextEncoding) -> Result<Vec<u8>, CodecError>` (escribe el BOM).

- [ ] **Step 1: Crear el crate**

`crates/notty-io/Cargo.toml`:

```toml
[package]
name = "notty-io"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
encoding_rs.workspace = true
memmap2.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

`crates/notty-io/src/lib.rs`:

```rust
//! notty-io: detección texto/raw, codificaciones, fin de línea y guardado atómico.

mod encoding;

pub use encoding::{CodecError, Detected, TextEncoding, decode, decode_lossy, detect, encode};
```

- [ ] **Step 2: Escribir los tests que fallan**

`crates/notty-io/src/encoding.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use TextEncoding::*;

    #[test]
    fn detects_boms() {
        assert_eq!(detect(&[0xEF, 0xBB, 0xBF, b'a']), Detected::Text(Utf8Bom));
        assert_eq!(detect(&[0xFF, 0xFE, b'a', 0]), Detected::Text(Utf16Le));
        assert_eq!(detect(&[0xFE, 0xFF, 0, b'a']), Detected::Text(Utf16Be));
    }

    #[test]
    fn plain_utf8_is_text() {
        assert_eq!(detect("hola ñ".as_bytes()), Detected::Text(Utf8));
    }

    #[test]
    fn empty_is_text() {
        assert_eq!(detect(&[]), Detected::Text(Utf8));
    }

    #[test]
    fn nul_bytes_mean_raw() {
        assert_eq!(detect(&[b'a', 0, b'b']), Detected::Raw);
    }

    #[test]
    fn invalid_utf8_means_raw() {
        assert_eq!(detect(&[0xC3, 0x28]), Detected::Raw);
    }

    #[test]
    fn utf8_cut_at_sample_end_is_still_text() {
        let s = "añ".as_bytes();
        assert_eq!(detect(&s[..2]), Detected::Text(Utf8));
    }

    #[test]
    fn round_trips_every_encoding() {
        for enc in [Utf8, Utf8Bom, Utf16Le, Utf16Be, Windows1252] {
            let bytes = encode("Año €", enc).unwrap();
            assert_eq!(decode(&bytes, enc).unwrap(), "Año €", "{}", enc.label());
        }
    }

    #[test]
    fn boms_are_written() {
        assert_eq!(encode("a", Utf8Bom).unwrap(), vec![0xEF, 0xBB, 0xBF, b'a']);
        assert_eq!(encode("a", Utf16Le).unwrap(), vec![0xFF, 0xFE, b'a', 0]);
        assert_eq!(encode("a", Utf16Be).unwrap(), vec![0xFE, 0xFF, 0, b'a']);
    }

    #[test]
    fn strict_decode_rejects_bad_bytes() {
        assert_eq!(decode(&[0xC3, 0x28], Utf8), Err(CodecError::Invalid("UTF-8")));
    }

    #[test]
    fn lossy_decode_replaces() {
        assert_eq!(decode_lossy(&[b'a', 0xFF], Utf8), "a\u{FFFD}");
    }

    #[test]
    fn unmappable_in_1252_is_an_error() {
        assert_eq!(encode("日", Windows1252), Err(CodecError::Unmappable("ANSI (1252)")));
    }
}
```

- [ ] **Step 3: Ejecutar y ver que falla**

Run: `cargo test -p notty-io`
Expected: FAIL de compilación, `cannot find type TextEncoding`.

- [ ] **Step 4: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-io/src/encoding.rs`:

```rust
use std::borrow::Cow;

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Windows1252,
}

impl TextEncoding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 con BOM",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::Windows1252 => "ANSI (1252)",
        }
    }

    fn bom(self) -> &'static [u8] {
        match self {
            Self::Utf8Bom => &[0xEF, 0xBB, 0xBF],
            Self::Utf16Le => &[0xFF, 0xFE],
            Self::Utf16Be => &[0xFE, 0xFF],
            Self::Utf8 | Self::Windows1252 => &[],
        }
    }

    fn coder(self) -> &'static Encoding {
        match self {
            Self::Utf8 | Self::Utf8Bom => UTF_8,
            Self::Utf16Le => UTF_16LE,
            Self::Utf16Be => UTF_16BE,
            Self::Windows1252 => WINDOWS_1252,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detected {
    Text(TextEncoding),
    Raw,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodecError {
    #[error("los bytes no son {0} válido")]
    Invalid(&'static str),
    #[error("el texto tiene caracteres que no caben en {0}")]
    Unmappable(&'static str),
}

/// Decide texto o raw a partir de los primeros bytes del archivo (~8 KB).
/// Texto: BOM, o UTF-8 válido sin bytes nulos. Todo lo demás: raw.
pub fn detect(sample: &[u8]) -> Detected {
    use TextEncoding::*;
    if sample.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Detected::Text(Utf8Bom);
    }
    if sample.starts_with(&[0xFF, 0xFE]) {
        return Detected::Text(Utf16Le);
    }
    if sample.starts_with(&[0xFE, 0xFF]) {
        return Detected::Text(Utf16Be);
    }
    if sample.contains(&0) {
        return Detected::Raw;
    }
    match std::str::from_utf8(sample) {
        Ok(_) => Detected::Text(Utf8),
        // La muestra puede cortar un carácter multibyte por la mitad.
        Err(e) if e.error_len().is_none() => Detected::Text(Utf8),
        Err(_) => Detected::Raw,
    }
}

/// Decodificación estricta: si un solo byte no encaja, error. Quita el BOM.
pub fn decode(bytes: &[u8], enc: TextEncoding) -> Result<String, CodecError> {
    let body = bytes.strip_prefix(enc.bom()).unwrap_or(bytes);
    enc.coder()
        .decode_without_bom_handling_and_without_replacement(body)
        .map(Cow::into_owned)
        .ok_or(CodecError::Invalid(enc.label()))
}

/// Solo para abrir en modo lectura cuando `decode` falla: cambia lo inválido por U+FFFD.
pub fn decode_lossy(bytes: &[u8], enc: TextEncoding) -> String {
    let body = bytes.strip_prefix(enc.bom()).unwrap_or(bytes);
    enc.coder().decode_without_bom_handling(body).0.into_owned()
}

pub fn encode(text: &str, enc: TextEncoding) -> Result<Vec<u8>, CodecError> {
    let mut out = enc.bom().to_vec();
    match enc {
        TextEncoding::Utf8 | TextEncoding::Utf8Bom => out.extend_from_slice(text.as_bytes()),
        // encoding_rs no codifica a UTF-16: se hace a mano.
        TextEncoding::Utf16Le => text.encode_utf16().for_each(|u| out.extend_from_slice(&u.to_le_bytes())),
        TextEncoding::Utf16Be => text.encode_utf16().for_each(|u| out.extend_from_slice(&u.to_be_bytes())),
        TextEncoding::Windows1252 => {
            let (bytes, _, unmappable) = WINDOWS_1252.encode(text);
            if unmappable {
                return Err(CodecError::Unmappable(enc.label()));
            }
            out.extend_from_slice(&bytes);
        }
    }
    Ok(out)
}
```

- [ ] **Step 5: Ejecutar y ver que pasa**

Run: `cargo test -p notty-io`
Expected: PASS, 11 tests.

- [ ] **Step 6: Commit**

```bash
git add Cargo.lock crates/notty-io
git commit -m "feat(io): detección texto/raw y codificaciones estrictas"
```

---

### Task 6: Fin de línea

**Files:**
- Create: `crates/notty-io/src/eol.rs`
- Modify: `crates/notty-io/src/lib.rs`

**Interfaces:**
- Consumes: nada.
- Produces: `LineEnding { Crlf, Lf }` con `as_str() -> &'static str` y `label() -> &'static str` ("CRLF"/"LF"); `detect_eol(&str) -> LineEnding` (mayoría; sin saltos → `Crlf`); `convert(&str, LineEnding) -> String`.

- [ ] **Step 1: Escribir los tests que fallan**

`crates/notty-io/src/eol.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_majority() {
        assert_eq!(detect_eol("a\r\nb\r\n"), LineEnding::Crlf);
        assert_eq!(detect_eol("a\nb\n"), LineEnding::Lf);
        assert_eq!(detect_eol("a\r\nb\nc\n"), LineEnding::Lf);
    }

    #[test]
    fn no_line_breaks_defaults_to_crlf() {
        assert_eq!(detect_eol("sin saltos"), LineEnding::Crlf);
    }

    #[test]
    fn converts_mixed_text() {
        assert_eq!(convert("a\r\nb\nc", LineEnding::Lf), "a\nb\nc");
        assert_eq!(convert("a\r\nb\nc", LineEnding::Crlf), "a\r\nb\r\nc");
    }

    #[test]
    fn labels() {
        assert_eq!(LineEnding::Crlf.label(), "CRLF");
        assert_eq!(LineEnding::Lf.as_str(), "\n");
    }
}
```

`crates/notty-io/src/lib.rs` queda:

```rust
//! notty-io: detección texto/raw, codificaciones, fin de línea y guardado atómico.

mod encoding;
mod eol;

pub use encoding::{CodecError, Detected, TextEncoding, decode, decode_lossy, detect, encode};
pub use eol::{LineEnding, convert, detect_eol};
```

- [ ] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-io eol`
Expected: FAIL de compilación, `cannot find type LineEnding`.

- [ ] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-io/src/eol.rs`:

```rust
/// Solo se usa para decidir qué inserta Enter y qué muestra la barra de estado.
/// El texto se guarda tal cual; solo `convert` lo cambia, y solo si el usuario lo pide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Crlf,
    Lf,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Crlf => "\r\n",
            Self::Lf => "\n",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Crlf => "CRLF",
            Self::Lf => "LF",
        }
    }
}

pub fn detect_eol(text: &str) -> LineEnding {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    if lf > crlf { LineEnding::Lf } else { LineEnding::Crlf }
}

pub fn convert(text: &str, to: LineEnding) -> String {
    let lf = text.replace("\r\n", "\n");
    match to {
        LineEnding::Lf => lf,
        LineEnding::Crlf => lf.replace('\n', "\r\n"),
    }
}
```

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test -p notty-io`
Expected: PASS, 15 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/notty-io
git commit -m "feat(io): detección y conversión de fin de línea"
```

---

### Task 7: Guardado atómico y utilidades de disco

**Files:**
- Create: `crates/notty-io/src/fsutil.rs`
- Modify: `crates/notty-io/src/lib.rs`

**Interfaces:**
- Consumes: nada.
- Produces: `atomic_write(&Path, &[u8]) -> io::Result<()>` (escribe `<nombre>.tmp~` al lado, `sync_all`, renombra encima; si falla borra el temporal), `create_parent_dirs(&Path) -> io::Result<bool>` (true si creó alguna carpeta), `can_write(&Path) -> bool`.

- [ ] **Step 1: Escribir los tests que fallan**

`crates/notty-io/src/fsutil.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn atomic_write_creates_and_overwrites() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        atomic_write(&p, b"uno").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"uno");
        atomic_write(&p, b"dos").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"dos");
        assert!(!dir.path().join("a.txt.tmp~").exists());
    }

    #[test]
    fn failed_write_keeps_target_and_leaves_no_temp_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("carpeta");
        fs::create_dir(&target).unwrap();
        assert!(atomic_write(&target, b"x").is_err());
        assert!(target.is_dir());
        assert!(!dir.path().join("carpeta.tmp~").exists());
    }

    #[test]
    fn create_parent_dirs_reports_creation() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("x").join("y").join("z.txt");
        assert!(create_parent_dirs(&p).unwrap());
        assert!(dir.path().join("x").join("y").is_dir());
        assert!(!create_parent_dirs(&p).unwrap());
    }

    #[test]
    #[allow(clippy::permissions_set_readonly_false)]
    fn can_write_detects_readonly() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        fs::write(&p, "x").unwrap();
        assert!(can_write(&p));
        let mut perms = fs::metadata(&p).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&p, perms.clone()).unwrap();
        assert!(!can_write(&p));
        perms.set_readonly(false);
        fs::set_permissions(&p, perms).unwrap();
    }
}
```

`crates/notty-io/src/lib.rs` queda:

```rust
//! notty-io: detección texto/raw, codificaciones, fin de línea y guardado atómico.

mod encoding;
mod eol;
mod fsutil;

pub use encoding::{CodecError, Detected, TextEncoding, decode, decode_lossy, detect, encode};
pub use eol::{LineEnding, convert, detect_eol};
pub use fsutil::{atomic_write, can_write, create_parent_dirs};
```

- [ ] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-io fsutil`
Expected: FAIL de compilación, `cannot find function atomic_write`.

- [ ] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-io/src/fsutil.rs`:

```rust
use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Guardado que nunca deja el original a medias: se escribe `<nombre>.tmp~`,
/// se fuerza a disco y se renombra encima. Si algo falla, el original sigue intacto.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "la ruta no tiene nombre de archivo"))?;
    let mut tmp_name = name.to_os_string();
    tmp_name.push(".tmp~");
    let tmp = path.with_file_name(tmp_name);

    let result = (|| -> io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Crea las carpetas que falten para `path`. Devuelve true si creó alguna.
pub fn create_parent_dirs(path: &Path) -> io::Result<bool> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() && !parent.exists() => {
            fs::create_dir_all(parent)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// ¿Podemos escribir el archivo? Cubre atributo de solo lectura, permisos y bloqueos.
/// Abrir para escritura sin truncar no modifica el archivo.
pub fn can_write(path: &Path) -> bool {
    fs::OpenOptions::new().write(true).open(path).is_ok()
}
```

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test -p notty-io`
Expected: PASS, 19 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/notty-io
git commit -m "feat(io): guardado atómico, crear carpetas y comprobar escritura"
```

---

### Task 8: Abrir archivos (texto o raw con mmap)

**Files:**
- Create: `crates/notty-io/src/open.rs`
- Modify: `crates/notty-io/src/lib.rs`

**Interfaces:**
- Consumes: `detect`, `decode`, `decode_lossy`, `detect_eol`, `can_write`.
- Produces:
  - `RawBytes { Mapped(memmap2::Mmap), Owned(Vec<u8>) }` con `Deref<Target = [u8]>`.
  - `Opened::Text { text: String, encoding: TextEncoding, eol: LineEnding, writable: bool, lossy: bool }` y `Opened::Raw { bytes: RawBytes, writable: bool }`.
  - `open(&Path) -> io::Result<Opened>` y `open_raw(&Path) -> io::Result<Opened>` (para "forzar raw").
  - Regla: si `decode` estricto falla, `lossy = true` y `writable = false` (nunca se guarda un texto con U+FFFD inventados).
  - Aviso para el Plan 5: en Windows no se puede renombrar encima de un archivo mapeado; antes de guardar un raw hay que soltar el `Mmap`.

- [ ] **Step 1: Escribir los tests que fallan**

`crates/notty-io/src/open.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn opens_utf8_text() {
        let dir = tempdir().unwrap();
        let p = write(dir.path(), "a.txt", b"hola\r\nmundo");
        match open(&p).unwrap() {
            Opened::Text { text, encoding, eol, writable, lossy } => {
                assert_eq!(text, "hola\r\nmundo");
                assert_eq!(encoding, TextEncoding::Utf8);
                assert_eq!(eol, LineEnding::Crlf);
                assert!(writable);
                assert!(!lossy);
            }
            Opened::Raw { .. } => panic!("esperaba texto"),
        }
    }

    #[test]
    fn opens_utf16_with_bom() {
        let dir = tempdir().unwrap();
        let bytes = crate::encode("hola\nñ", TextEncoding::Utf16Le).unwrap();
        let p = write(dir.path(), "u16.txt", &bytes);
        match open(&p).unwrap() {
            Opened::Text { text, encoding, eol, .. } => {
                assert_eq!(text, "hola\nñ");
                assert_eq!(encoding, TextEncoding::Utf16Le);
                assert_eq!(eol, LineEnding::Lf);
            }
            Opened::Raw { .. } => panic!("esperaba texto"),
        }
    }

    #[test]
    fn empty_file_is_text() {
        let dir = tempdir().unwrap();
        let p = write(dir.path(), "vacio.txt", b"");
        assert!(matches!(open(&p).unwrap(), Opened::Text { .. }));
    }

    #[test]
    fn binary_opens_raw_with_same_bytes() {
        let dir = tempdir().unwrap();
        let data = [0x89, b'P', b'N', b'G', 0, 0, 1];
        let p = write(dir.path(), "logo.png", &data);
        match open(&p).unwrap() {
            Opened::Raw { bytes, writable } => {
                assert_eq!(&*bytes, &data[..]);
                assert!(writable);
            }
            Opened::Text { .. } => panic!("esperaba raw"),
        }
    }

    #[test]
    fn bad_bytes_after_sample_open_lossy_and_read_only() {
        let dir = tempdir().unwrap();
        let mut data = vec![b'a'; 9000];
        data.push(0xFF);
        let p = write(dir.path(), "raro.txt", &data);
        match open(&p).unwrap() {
            Opened::Text { text, writable, lossy, .. } => {
                assert!(lossy);
                assert!(!writable);
                assert!(text.ends_with('\u{FFFD}'));
            }
            Opened::Raw { .. } => panic!("esperaba texto lossy"),
        }
    }

    #[test]
    #[allow(clippy::permissions_set_readonly_false)]
    fn readonly_file_is_not_writable() {
        let dir = tempdir().unwrap();
        let p = write(dir.path(), "ro.txt", b"x");
        let mut perms = fs::metadata(&p).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&p, perms.clone()).unwrap();
        assert!(matches!(open(&p).unwrap(), Opened::Text { writable: false, .. }));
        perms.set_readonly(false);
        fs::set_permissions(&p, perms).unwrap();
    }

    #[test]
    fn open_raw_forces_raw_view() {
        let dir = tempdir().unwrap();
        let p = write(dir.path(), "a.txt", b"hola");
        match open_raw(&p).unwrap() {
            Opened::Raw { bytes, .. } => assert_eq!(&*bytes, b"hola"),
            Opened::Text { .. } => panic!("esperaba raw"),
        }
    }

    #[test]
    fn open_raw_of_empty_file_works() {
        let dir = tempdir().unwrap();
        let p = write(dir.path(), "vacio.bin", b"");
        match open_raw(&p).unwrap() {
            Opened::Raw { bytes, .. } => assert!(bytes.is_empty()),
            Opened::Text { .. } => panic!("esperaba raw"),
        }
    }
}
```

`crates/notty-io/src/lib.rs` queda:

```rust
//! notty-io: detección texto/raw, codificaciones, fin de línea y guardado atómico.

mod encoding;
mod eol;
mod fsutil;
mod open;

pub use encoding::{CodecError, Detected, TextEncoding, decode, decode_lossy, detect, encode};
pub use eol::{LineEnding, convert, detect_eol};
pub use fsutil::{atomic_write, can_write, create_parent_dirs};
pub use open::{Opened, RawBytes, open, open_raw};
```

- [ ] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-io open`
Expected: FAIL de compilación, `cannot find type Opened`.

- [ ] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-io/src/open.rs`:

```rust
use std::fs;
use std::io::{self, Read};
use std::ops::Deref;
use std::path::Path;

use memmap2::Mmap;

use crate::{Detected, LineEnding, TextEncoding, can_write, decode, decode_lossy, detect, detect_eol};

const SAMPLE: u64 = 8 * 1024;

/// Bytes de un archivo raw: mapeado en memoria (sin copiar) o, si está vacío, un Vec.
pub enum RawBytes {
    Mapped(Mmap),
    Owned(Vec<u8>),
}

impl Deref for RawBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            Self::Mapped(m) => m,
            Self::Owned(v) => v,
        }
    }
}

pub enum Opened {
    Text { text: String, encoding: TextEncoding, eol: LineEnding, writable: bool, lossy: bool },
    Raw { bytes: RawBytes, writable: bool },
}

pub fn open(path: &Path) -> io::Result<Opened> {
    let mut file = fs::File::open(path)?;
    let mut sample = Vec::with_capacity(SAMPLE as usize);
    Read::by_ref(&mut file).take(SAMPLE).read_to_end(&mut sample)?;
    drop(file);

    match detect(&sample) {
        Detected::Raw => open_raw(path),
        Detected::Text(encoding) => {
            let bytes = fs::read(path)?;
            let (text, lossy) = match decode(&bytes, encoding) {
                Ok(text) => (text, false),
                Err(_) => (decode_lossy(&bytes, encoding), true),
            };
            let eol = detect_eol(&text);
            let writable = !lossy && can_write(path);
            Ok(Opened::Text { text, encoding, eol, writable, lossy })
        }
    }
}

pub fn open_raw(path: &Path) -> io::Result<Opened> {
    let file = fs::File::open(path)?;
    let bytes = if file.metadata()?.len() == 0 {
        RawBytes::Owned(Vec::new())
    } else {
        // SAFETY: si otro programa trunca el archivo mientras está mapeado, leer
        // podría fallar. Es el precio de abrir binarios de GB al instante; notty
        // vigila cambios externos (Plan 6) y reabre el archivo si cambia.
        RawBytes::Mapped(unsafe { Mmap::map(&file)? })
    };
    Ok(Opened::Raw { bytes, writable: can_write(path) })
}
```

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test -p notty-io`
Expected: PASS, 27 tests.

- [ ] **Step 5: Comprobar todo el workspace con clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: sin avisos. Si clippy marca algo, corregirlo en el archivo que indique y volver a ejecutar `cargo test --workspace`.

- [ ] **Step 6: Commit**

```bash
git add crates/notty-io
git commit -m "feat(io): abrir archivos como texto o raw con mmap"
```

---

## Hoja de ruta: planes siguientes

Cada uno se escribirá como plan propio (mismo formato) cuando termine el anterior, porque dependen de decisiones que este plan fija (APIs de `Document` y `Opened`).

| Plan | Contenido | Resultado usable |
|---|---|---|
| **2 · Ventana y editor** | `notty-ui` + bin `notty`: ventana Win32 con Mica y barra de título de Windows 11, render del texto con DirectWrite (solo líneas visibles), cursor y selección con teclado y ratón, portapapeles, IME básico, abrir un archivo por argumento, `Ctrl+S` con guardado atómico, barra de estado estática (`Texto · Ln, Col · UTF-8 · CRLF`). | `notty.exe archivo.txt` edita y guarda. |
| **3 · Config, comandos y ventana** | `notty-config` (`config.toml` en `%APPDATA%\notty\`, presets Moderna/Clásica/Zen, validación, recarga en caliente, config rota → defaults sin sobrescribir). `notty-input` (comandos con nombre + keymap). Piezas de la ventana: pestañas o buffers, barra de menús, barra de atajos nano contextual, números de línea. Ventana de Ajustes. | Toda la personalización de la maqueta. |
| **4 · Línea de ruta y búsqueda** | Prompts en la barra de estado: línea de ruta (normalización, sugerencia fantasma, `Tab`, búsqueda difusa, `~`/`%VAR%`/`.`, crear carpetas, `CLICKME`), `Ctrl+F`/`Ctrl+H` con resaltado en vivo y `Alt+C/W/R`, `F3`, `Ctrl+G`, `Ctrl+D`. | Crear, abrir y buscar sin diálogos. |
| **5 · Vim y raw** | Keymap vim (normal/insert/visual, `hjkl`, `dd`, `/`, `:w`, `:%s`), alternar con `Ctrl+Alt+V` o global. Vista hex sobre `RawBytes`, solo lectura por defecto, lápiz, edición byte a byte, guardado (soltar el mmap antes), `Ctrl+Shift+H`. | Modo vim y editor hexadecimal. |
| **6 · Archivos vivos** | Temporales (borrador en `%LOCALAPPDATA%\notty\drafts\` / volátil), instancia única por named pipe, `--daemon` con bandeja y atajos globales, alternativa `.lnk`, autoguardado, vigilancia de cambios externos y conflicto al guardar (mío / disco), volcado de recuperación ante pánico, errores de E/S en la barra de estado. | La premisa completa de la v1. |
| **7 · Pulido y garantías** | UI Automation (lector de pantalla), alto contraste y DPI, fuzzing (`cargo-fuzz`) de `detect` y `Document`, CI con presupuesto (< 3 MB, arranque < 50 ms, 100 MB < 1 s). | v1 lista. |
