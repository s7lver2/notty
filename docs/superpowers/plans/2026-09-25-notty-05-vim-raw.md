# notty · Plan 5: Vim y raw Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `Ctrl+Alt+V` alterna un modo vim real (normal/insert/visual: `hjkl`, `i`/`a`/`o`, `dd`, `x`, `gg`/`G`, `/`, `:w`, `:q`, `:%s/a/b/g`) sobre el mismo `Document`, con la barra de estado mostrando `-- NORMAL --`/`-- INSERT --`. `Ctrl+Shift+H` fuerza un archivo de texto a verse como raw y viceversa; la vista raw es un editor hexadecimal que arranca en solo lectura y solo permite escribir tras pulsar el lápiz.

**Architecture:** El modo vim vive en `notty-ui::vim` como una máquina de estados pura (`VimState`) que traduce teclas en operaciones sobre `notty_core::Document`, exactamente igual que ya hace `EditorState::apply` para el modo normal — se testea igual, sin Win32. La vista raw introduce `notty-ui::raw_doc::RawDoc`, que envuelve el `notty_io::RawBytes` (mmap) de solo lectura y solo materializa una copia editable en memoria (`Vec<u8>`) cuando el usuario pulsa el lápiz; al guardar, esa copia se escribe con `notty_io::atomic_write` después de soltar cualquier vista mapeada del archivo (nunca se escribe encima de un `Mmap` abierto). Dibujar la cuadrícula hexadecimal y el indicador de modo vim es lo único que toca Direct2D.

**Tech Stack:** Rust stable 1.96 (MSVC), `notty-core`/`notty-io`/`notty-ui`/`notty-config`/`notty-input` (Planes 1-4), `windows` (ya en el workspace).

**Spec:** `docs/superpowers/specs/2026-09-24-notty-design.md`
**Planes anteriores:** Plan 1 (`2026-09-24-notty-01-nucleo.md`), Plan 2 (`2026-09-25-notty-02-ventana-editor.md`), Plan 3 (`2026-09-25-notty-03-config-ventana.md`), Plan 4 (`2026-09-25-notty-04-ruta-busqueda.md`)

## Global Constraints

- Solo Windows 10/11; toolchain `stable-x86_64-pc-windows-msvc`.
- `notty-ui::vim` y `notty-ui::raw_doc` no dependen de Win32; se testean con `notty_core::Document`/`tempfile::tempdir()` como el resto del código de lógica pura.
- La vista raw **siempre arranca en solo lectura**. Solo pasa a escritura si el archivo es escribible en disco (`notty_io::can_write`) **y** el usuario pulsa el lápiz explícitamente. Si el sistema de archivos no permite escribir, el lápiz debe estar deshabilitado sin excepción.
- Nunca se escribe encima de un archivo mientras está mapeado (`Mmap`); el guardado de raw siempre suelta la vista mapeada antes de escribir.
- Textos visibles para el usuario en español.
- Un commit por tarea terminada.
- **Corrección de un fallo real de los Planes 2/4:** `notty_ui::keymap::action_for_vk` ignoraba `Modifiers.alt` en su `match`, así que `Ctrl+Alt+V` era indistinguible de `Ctrl+V` (Paste). Además, el `Replace` del Plan 4 (`(0x48, true, _) => Replace`) capturaba también `Ctrl+Shift+H` antes de que pudiera existir un `ToggleRaw`. La Task 1 de este plan reescribe `action_for_vk` por completo para incluir `alt` en todos los brazos existentes (comportamiento idéntico al de antes en todos los casos ya cubiertos) y añade los dos nuevos de forma no ambigua. Los tests existentes de los Planes 2 y 4 sobre `action_for_vk` deben seguir pasando sin cambios.
- **Sobre el código Win32/Direct2D de este plan** (Tasks 6-7: enrutar el teclado en modo vim/raw, dibujar la cuadrícula hexadecimal y el indicador de modo): se da la arquitectura y las llamadas por su nombre. Compílalo, y si una firma no coincide con la versión de `windows` instalada, ajústala consultando `cargo doc -p windows --open` o el error del compilador, sin cambiar el comportamiento descrito. Los pasos marcados como "lógica pura" son código exacto.

## File Structure

```
crates/notty-ui/src/keymap.rs      (modificado) alt en el match, ToggleVim, ToggleRaw
crates/notty-ui/src/vim.rs         VimState: modos normal/insert/visual sobre Document (nuevo)
crates/notty-ui/src/vim_cmd.rs     parser de :w/:q/:wq/:%s (nuevo)
crates/notty-ui/src/raw_doc.rs     RawDoc: mmap de solo lectura + copia editable perezosa (nuevo)
crates/notty-ui/src/hex.rs         formateo de la cuadrícula hexadecimal (lógica pura, nuevo)
crates/notty-ui/src/render.rs      (modificado) dibuja hex, indicador de modo vim, lápiz
crates/notty-ui/src/window.rs      (modificado) enruta teclado a vim/raw, guarda raw
crates/notty-ui/src/settings_window.rs  (modificado) "Modo vim siempre"
```

---

### Task 1: keymap: incluir `alt`, `ToggleVim`, `ToggleRaw` (lógica pura, corrige un fallo real)

**Files:**
- Modify: `crates/notty-ui/src/keymap.rs`

**Interfaces:**
- Consumes: nada nuevo.
- Produces: `EditorAction` gana `ToggleVim, ToggleRaw`. `action_for_vk` pasa a considerar los tres modificadores; `Ctrl+Alt+V` (`0x56`) → `ToggleVim`, `Ctrl+Shift+H` (`0x48`) → `ToggleRaw`, `Ctrl+H` sin `Shift` sigue siendo `Replace` (Plan 4).

- [x] **Step 1: Escribir los tests que fallan**

Añadir a `mod tests` en `crates/notty-ui/src/keymap.rs` (además de todos los que ya había):

```rust
    #[test]
    fn ctrl_alt_v_is_toggle_vim_not_paste() {
        assert_eq!(action_for_vk(0x56, Modifiers { ctrl: true, shift: false, alt: true }), EditorAction::ToggleVim);
        assert_eq!(action_for_vk(0x56, Modifiers { ctrl: true, shift: false, alt: false }), EditorAction::Paste);
    }

    #[test]
    fn ctrl_shift_h_is_toggle_raw_not_replace() {
        assert_eq!(action_for_vk(0x48, Modifiers { ctrl: true, shift: true, alt: false }), EditorAction::ToggleRaw);
        assert_eq!(action_for_vk(0x48, Modifiers { ctrl: true, shift: false, alt: false }), EditorAction::Replace);
    }

    #[test]
    fn plain_letter_with_stray_alt_is_still_none() {
        assert_eq!(action_for_vk(0x41, Modifiers { ctrl: false, shift: false, alt: true }), EditorAction::None);
    }
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui keymap`
Expected: FAIL: `ctrl_alt_v_is_toggle_vim_not_paste` y `ctrl_shift_h_is_toggle_raw_not_replace` fallan (dan `Paste`/`Replace` en vez de `ToggleVim`/`ToggleRaw`, o no compila si `ToggleVim`/`ToggleRaw` todavía no existen como variantes).

- [x] **Step 3: Reescribir `action_for_vk` por completo**

Añadir las dos variantes nuevas al `enum EditorAction` (junto a las que ya había de los Planes 2 y 4):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    MoveLeft, MoveRight, MoveUp, MoveDown, MoveHome, MoveEnd, MoveDocStart, MoveDocEnd,
    ExtendLeft, ExtendRight, ExtendUp, ExtendDown, ExtendHome, ExtendEnd,
    Backspace, DeleteForward, InsertNewline,
    Undo, Redo, SelectAll, Copy, Cut, Paste, Save, Find, Replace, FindNext, FindPrev, OpenPathPrompt,
    ToggleVim, ToggleRaw,
    None,
}
```

Sustituir el cuerpo de `action_for_vk` entero por (mismo comportamiento que antes para todo lo que no lleve `Alt`, más los dos casos nuevos):

```rust
pub fn action_for_vk(vk: u32, m: Modifiers) -> EditorAction {
    use EditorAction::*;
    match (vk, m.ctrl, m.shift, m.alt) {
        (0x56, true, false, true) => ToggleVim,
        (0x48, true, true, false) => ToggleRaw,
        (0x25, false, false, false) => MoveLeft,
        (0x25, false, true, false) => ExtendLeft,
        (0x27, false, false, false) => MoveRight,
        (0x27, false, true, false) => ExtendRight,
        (0x26, false, false, false) => MoveUp,
        (0x26, false, true, false) => ExtendUp,
        (0x28, false, false, false) => MoveDown,
        (0x28, false, true, false) => ExtendDown,
        (0x24, false, false, false) => MoveHome,
        (0x24, false, true, false) => ExtendHome,
        (0x24, true, _, false) => MoveDocStart,
        (0x23, false, false, false) => MoveEnd,
        (0x23, false, true, false) => ExtendEnd,
        (0x23, true, _, false) => MoveDocEnd,
        (0x08, _, _, false) => Backspace,
        (0x2E, _, _, false) => DeleteForward,
        (0x0D, _, _, false) => InsertNewline,
        (0x5A, true, false, false) => Undo,
        (0x5A, true, true, false) => Redo,
        (0x59, true, _, false) => Redo,
        (0x41, true, _, false) => SelectAll,
        (0x43, true, _, false) => Copy,
        (0x58, true, _, false) => Cut,
        (0x56, true, _, false) => Paste,
        (0x53, true, _, false) => Save,
        (0x46, true, _, false) => Find,
        (0x48, true, false, false) => Replace,
        (0x72, false, false, false) => FindNext,
        (0x72, false, true, false) => FindPrev,
        (0x4F, true, _, false) => OpenPathPrompt,
        _ => None,
    }
}
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS. Todos los tests anteriores de `action_for_vk` (Planes 2 y 4) siguen pasando sin tocarlos, más los 3 nuevos.

- [x] **Step 5: Commit**

```bash
git add crates/notty-ui
git commit -m "fix(ui): Ctrl+Alt+V y Ctrl+Shift+H ya no chocan con Paste/Replace"
```

---

### Task 2: notty-ui::vim — modos normal/insert/visual (lógica pura sobre Document)

**Files:**
- Create: `crates/notty-ui/src/vim.rs`
- Modify: `crates/notty-ui/src/lib.rs`

**Interfaces:**
- Consumes: `notty_core::Document`.
- Produces:
  - `pub enum VimMode { Normal, Insert, Visual }` (`Default` = `Normal`).
  - `pub struct VimState { pub mode: VimMode, pending: String, visual_anchor: usize }` (`Default`).
  - `pub enum VimOutcome { Handled, OpenFind, OpenCmdline, Bubble }`: `Handled` = la tecla ya hizo su efecto sobre `doc`; `OpenFind`/`OpenCmdline` = hay que abrir el prompt de búsqueda (`/`) o de comandos (`:`) de `notty-ui` (Plan 4); `Bubble` = la tecla no es de vim (por ejemplo con `Ctrl` u otro modificador que vim no usa) y debe tratarse como si vim no estuviera activo.
  - `VimState::handle_key(&mut self, doc: &mut notty_core::Document, vk: u32, ch: Option<char>, now: std::time::Instant) -> VimOutcome`: en modo `Insert`, cualquier `ch` imprimible se inserta con `doc.insert`, `Esc` (`vk == 0x1B`) pasa a `Normal`; en modo `Normal`/`Visual`, interpreta las teclas de la maqueta: `i`/`a`/`A`/`o` entran en `Insert` (con el cursor colocado según corresponda), `hjkl` mueven el cursor (en `Visual`, extendiendo la selección desde `visual_anchor`), `0`/`$` van a inicio/fin de línea, `x` borra el carácter bajo el cursor, `dd` (dos `d` seguidas) borra la línea, `gg`/`G` van al principio/fin del documento, `u` deshace (`doc.undo()`), `v` entra/sale de `Visual` marcando `visual_anchor`, `/` devuelve `VimOutcome::OpenFind`, `:` devuelve `VimOutcome::OpenCmdline`, cualquier otra tecla sin traducción devuelve `VimOutcome::Bubble`.

- [x] **Step 1: Escribir los tests que fallan**

`crates/notty-ui/src/vim.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use notty_core::{Document, Selection};
    use std::time::Instant;

    fn doc(text: &str) -> Document {
        Document::new(text, "\n")
    }

    fn key(v: &mut VimState, d: &mut Document, vk: u32) -> VimOutcome {
        v.handle_key(d, vk, None, Instant::now())
    }

    fn ch(v: &mut VimState, d: &mut Document, c: char) -> VimOutcome {
        v.handle_key(d, 0, Some(c), Instant::now())
    }

    #[test]
    fn starts_in_normal_mode() {
        assert_eq!(VimState::default().mode, VimMode::Normal);
    }

    #[test]
    fn i_enters_insert_and_typing_edits_the_document() {
        let mut v = VimState::default();
        let mut d = doc("ac");
        ch(&mut v, &mut d, 'i');
        assert_eq!(v.mode, VimMode::Insert);
        d.set_cursor(1);
        ch(&mut v, &mut d, 'b');
        assert_eq!(d.text(), "abc");
    }

    #[test]
    fn esc_returns_to_normal() {
        let mut v = VimState::default();
        let mut d = doc("a");
        ch(&mut v, &mut d, 'i');
        key(&mut v, &mut d, 0x1B);
        assert_eq!(v.mode, VimMode::Normal);
    }

    #[test]
    fn hjkl_move_the_cursor() {
        let mut v = VimState::default();
        let mut d = doc("ab\ncd");
        ch(&mut v, &mut d, 'l');
        assert_eq!(d.selection(), Selection::caret(1));
        ch(&mut v, &mut d, 'j');
        assert_eq!(d.line_col().0, 1);
        ch(&mut v, &mut d, 'h');
        ch(&mut v, &mut d, 'k');
        assert_eq!(d.line_col().0, 0);
    }

    #[test]
    fn zero_and_dollar_go_to_line_bounds() {
        let mut v = VimState::default();
        let mut d = doc("abcdef");
        d.set_cursor(3);
        ch(&mut v, &mut d, '0');
        assert_eq!(d.selection(), Selection::caret(0));
        ch(&mut v, &mut d, '$');
        assert_eq!(d.selection(), Selection::caret(6));
    }

    #[test]
    fn x_deletes_char_under_cursor() {
        let mut v = VimState::default();
        let mut d = doc("abc");
        ch(&mut v, &mut d, 'x');
        assert_eq!(d.text(), "bc");
    }

    #[test]
    fn dd_deletes_the_whole_line() {
        let mut v = VimState::default();
        let mut d = doc("uno\ndos\ntres");
        d.set_cursor(5); // dentro de "dos"
        ch(&mut v, &mut d, 'd');
        ch(&mut v, &mut d, 'd');
        assert_eq!(d.text(), "uno\ntres");
    }

    #[test]
    fn gg_and_g_go_to_document_bounds() {
        let mut v = VimState::default();
        let mut d = doc("abc\ndef");
        ch(&mut v, &mut d, 'G');
        assert_eq!(d.selection(), Selection::caret(7));
        ch(&mut v, &mut d, 'g');
        ch(&mut v, &mut d, 'g');
        assert_eq!(d.selection(), Selection::caret(0));
    }

    #[test]
    fn u_undoes() {
        let mut v = VimState::default();
        let mut d = doc("a");
        d.insert("b", Instant::now());
        ch(&mut v, &mut d, 'u');
        assert_eq!(d.text(), "a");
    }

    #[test]
    fn v_enters_visual_and_hjkl_extends_selection() {
        let mut v = VimState::default();
        let mut d = doc("abcdef");
        ch(&mut v, &mut d, 'v');
        assert_eq!(v.mode, VimMode::Visual);
        ch(&mut v, &mut d, 'l');
        ch(&mut v, &mut d, 'l');
        assert_eq!(d.selection(), Selection { anchor: 0, head: 2 });
    }

    #[test]
    fn slash_and_colon_open_prompts() {
        let mut v = VimState::default();
        let mut d = doc("a");
        assert_eq!(ch(&mut v, &mut d, '/'), VimOutcome::OpenFind);
        assert_eq!(ch(&mut v, &mut d, ':'), VimOutcome::OpenCmdline);
    }

    #[test]
    fn unmapped_key_bubbles_up() {
        let mut v = VimState::default();
        let mut d = doc("a");
        assert_eq!(key(&mut v, &mut d, 0xBD /* algo que vim no usa */), VimOutcome::Bubble);
    }
```

(Cierra el bloque `mod tests` con `}` tras el último test.)

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui vim`
Expected: FAIL de compilación, `cannot find type VimState`.

- [x] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-ui/src/vim.rs`:

```rust
use std::time::Instant;

use notty_core::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VimMode {
    #[default]
    Normal,
    Insert,
    Visual,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VimState {
    pub mode: VimMode,
    pending: String,
    visual_anchor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimOutcome {
    Handled,
    OpenFind,
    OpenCmdline,
    Bubble,
}

impl VimState {
    pub fn handle_key(&mut self, doc: &mut Document, vk: u32, ch: Option<char>, now: Instant) -> VimOutcome {
        if self.mode == VimMode::Insert {
            return self.handle_insert(doc, vk, ch, now);
        }
        match ch {
            Some(c) => self.handle_normal_or_visual(doc, c, now),
            None if vk == 0x1B => {
                self.pending.clear();
                VimOutcome::Handled
            }
            None => VimOutcome::Bubble,
        }
    }

    fn handle_insert(&mut self, doc: &mut Document, vk: u32, ch: Option<char>, now: Instant) -> VimOutcome {
        if vk == 0x1B {
            self.mode = VimMode::Normal;
            return VimOutcome::Handled;
        }
        match ch {
            Some(c) if !c.is_control() => {
                doc.insert(&c.to_string(), now);
                VimOutcome::Handled
            }
            Some(_) | None => VimOutcome::Bubble,
        }
    }

    fn line_end(&self, doc: &Document, idx: usize) -> usize {
        let buf = doc.buffer();
        let (line, _) = buf.line_col(idx);
        let total = buf.len_lines();
        let end = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
        let text = buf.slice(buf.line_start(line)..end);
        buf.line_start(line) + text.trim_end_matches(['\r', '\n']).chars().count()
    }

    fn move_to(&mut self, doc: &mut Document, idx: usize) {
        if self.mode == VimMode::Visual {
            doc.set_selection(self.visual_anchor, idx);
        } else {
            doc.set_cursor(idx);
        }
    }

    fn handle_normal_or_visual(&mut self, doc: &mut Document, c: char, now: Instant) -> VimOutcome {
        let key = self.pending.clone() + &c.to_string();
        let head = doc.selection().head;
        let buf_len = doc.buffer().len_chars();
        self.pending.clear();

        match key.as_str() {
            "i" => self.mode = VimMode::Insert,
            "a" => {
                doc.set_cursor((head + 1).min(buf_len));
                self.mode = VimMode::Insert;
            }
            "A" => {
                let end = self.line_end(doc, head);
                doc.set_cursor(end);
                self.mode = VimMode::Insert;
            }
            "o" => {
                let end = self.line_end(doc, head);
                doc.set_cursor(end);
                doc.insert_newline(now);
                self.mode = VimMode::Insert;
            }
            "h" => self.move_to(doc, head.saturating_sub(1)),
            "l" => self.move_to(doc, (head + 1).min(buf_len)),
            "j" | "k" => {
                let buf = doc.buffer();
                let (line, col) = buf.line_col(head);
                let delta: i64 = if key == "j" { 1 } else { -1 };
                let target_line = (line as i64 + delta).clamp(0, buf.len_lines() as i64 - 1) as usize;
                let start = buf.line_start(target_line);
                let end = self.line_end(doc, start);
                let idx = (start + col).min(end);
                self.move_to(doc, idx);
            }
            "0" => {
                let (line, _) = doc.buffer().line_col(head);
                let start = doc.buffer().line_start(line);
                self.move_to(doc, start);
            }
            "$" => {
                let end = self.line_end(doc, head);
                self.move_to(doc, end);
            }
            "x" => {
                if head < buf_len {
                    doc.replace_range(head..head + 1, "", now);
                }
            }
            "dd" => {
                let buf = doc.buffer();
                let (line, _) = buf.line_col(head);
                let start = buf.line_start(line);
                let total = buf.len_lines();
                let end = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
                doc.replace_range(start..end, "", now);
            }
            "d" => self.pending = "d".to_string(),
            "gg" => self.move_to(doc, 0),
            "g" => self.pending = "g".to_string(),
            "G" => self.move_to(doc, buf_len),
            "u" => {
                doc.undo();
            }
            "v" => {
                if self.mode == VimMode::Visual {
                    self.mode = VimMode::Normal;
                } else {
                    self.visual_anchor = head;
                    self.mode = VimMode::Visual;
                }
            }
            "/" => return VimOutcome::OpenFind,
            ":" => return VimOutcome::OpenCmdline,
            _ => return VimOutcome::Bubble,
        }
        VimOutcome::Handled
    }
}
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, 173 tests (159 anteriores + 3 de la Task 1 + 13 de esta tarea, más los que aporten el resto de tareas de este plan; recuenta con `cargo test` en vez de fiarte solo de este número si algo no cuadra).

- [x] **Step 5: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): modo vim (normal/insert/visual) sobre Document"
```

---

### Task 3: notty-ui::vim_cmd — `:w`, `:q`, `:wq`, `:%s/a/b/g` (lógica pura)

**Files:**
- Create: `crates/notty-ui/src/vim_cmd.rs`
- Modify: `crates/notty-ui/src/lib.rs`

**Interfaces:**
- Consumes: nada (el resultado lo ejecuta `window.rs`).
- Produces: `pub enum VimCmd { Save, Quit, SaveAndQuit, Substitute { pattern: String, replacement: String, global: bool, ignore_case: bool }, Unknown(String) }` y `pub fn parse_vim_cmd(line: &str) -> VimCmd`. Reconoce `w`, `q`, `wq`, `x` (alias de `wq`), y `%s/patrón/reemplazo/flags` con `/` escapable como `\/` dentro de patrón/reemplazo; cualquier otra cosa es `Unknown(line.to_string())`.

- [x] **Step 1: Escribir los tests que fallan**

`crates/notty-ui/src/vim_cmd.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w_q_wq() {
        assert_eq!(parse_vim_cmd("w"), VimCmd::Save);
        assert_eq!(parse_vim_cmd("q"), VimCmd::Quit);
        assert_eq!(parse_vim_cmd("wq"), VimCmd::SaveAndQuit);
        assert_eq!(parse_vim_cmd("x"), VimCmd::SaveAndQuit);
    }

    #[test]
    fn substitute_without_flags() {
        assert_eq!(
            parse_vim_cmd("%s/hola/adios/"),
            VimCmd::Substitute { pattern: "hola".into(), replacement: "adios".into(), global: false, ignore_case: false }
        );
    }

    #[test]
    fn substitute_with_global_and_case_flags() {
        assert_eq!(
            parse_vim_cmd("%s/a/b/gi"),
            VimCmd::Substitute { pattern: "a".into(), replacement: "b".into(), global: true, ignore_case: true }
        );
    }

    #[test]
    fn substitute_allows_escaped_slash() {
        assert_eq!(
            parse_vim_cmd(r"%s/a\/b/c/"),
            VimCmd::Substitute { pattern: "a/b".into(), replacement: "c".into(), global: false, ignore_case: false }
        );
    }

    #[test]
    fn unknown_command_is_preserved_verbatim() {
        assert_eq!(parse_vim_cmd("nope"), VimCmd::Unknown("nope".to_string()));
    }

    #[test]
    fn malformed_substitute_is_unknown() {
        assert_eq!(parse_vim_cmd("%s/solo-dos-barras"), VimCmd::Unknown("%s/solo-dos-barras".to_string()));
    }
}
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui vim_cmd`
Expected: FAIL de compilación, `cannot find function parse_vim_cmd`.

- [x] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-ui/src/vim_cmd.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VimCmd {
    Save,
    Quit,
    SaveAndQuit,
    Substitute { pattern: String, replacement: String, global: bool, ignore_case: bool },
    Unknown(String),
}

pub fn parse_vim_cmd(line: &str) -> VimCmd {
    match line {
        "w" => return VimCmd::Save,
        "q" => return VimCmd::Quit,
        "wq" | "x" => return VimCmd::SaveAndQuit,
        _ => {}
    }
    if let Some(rest) = line.strip_prefix("%s/") {
        if let Some(cmd) = parse_substitute(rest) {
            return cmd;
        }
    }
    VimCmd::Unknown(line.to_string())
}

fn parse_substitute(rest: &str) -> Option<VimCmd> {
    let parts = split_unescaped_slash(rest);
    if parts.len() != 3 {
        return None;
    }
    let flags = parts[2];
    Some(VimCmd::Substitute {
        pattern: parts[0].replace(r"\/", "/"),
        replacement: parts[1].replace(r"\/", "/"),
        global: flags.contains('g'),
        ignore_case: flags.contains('i'),
    })
}

fn split_unescaped_slash(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && (i == 0 || bytes[i - 1] != b'\\') {
            out.push(&s[start..i]);
            start = i + 1;
        }
        i += 1;
    }
    out.push(&s[start..]);
    out
}
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, +6 tests sobre los de la Task 2.

- [x] **Step 5: Exportar los módulos nuevos**

`crates/notty-ui/src/lib.rs` añade (junto a los `mod`/`pub use` ya existentes de los planes anteriores):

```rust
mod vim;
mod vim_cmd;

pub use vim::{VimMode, VimOutcome, VimState};
pub use vim_cmd::{VimCmd, parse_vim_cmd};
```

- [x] **Step 6: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): parser de comandos vim (:w, :q, :wq, :%s)"
```

---

### Task 4: notty-ui::raw_doc — vista hex de solo lectura con copia editable perezosa (tempdir)

**Files:**
- Create: `crates/notty-ui/src/raw_doc.rs`
- Modify: `crates/notty-ui/src/lib.rs`

**Interfaces:**
- Consumes: `notty_io::{open_raw, Opened, RawBytes, atomic_write, can_write}`.
- Produces:
  - `pub struct RawDoc { path: std::path::PathBuf, view: notty_io::RawBytes, writable_fs: bool, edit_buf: Option<Vec<u8>>, dirty: bool }`.
  - `pub fn open_raw_doc(path: &Path) -> std::io::Result<RawDoc>` (usa `notty_io::open_raw`; error si el `Opened` no es `Raw`, aunque en la práctica siempre lo es).
  - `RawDoc::len(&self) -> usize`, `RawDoc::byte(&self, i: usize) -> u8` (lee de `edit_buf` si existe, si no de `view`), `RawDoc::writable_fs(&self) -> bool`, `RawDoc::is_editing(&self) -> bool` (`edit_buf.is_some()`), `RawDoc::is_dirty(&self) -> bool`.
  - `RawDoc::enable_write(&mut self) -> bool`: si `writable_fs`, materializa `edit_buf` (copia de `view`, byte a byte, sin depender de que `RawBytes` implemente `to_vec` directamente) y devuelve `true`; si no, no hace nada y devuelve `false`.
  - `RawDoc::set_byte(&mut self, i: usize, value: u8) -> bool`: si `edit_buf` es `Some` y `i < len`, escribe y marca `dirty`; devuelve si se pudo escribir.
  - `RawDoc::save(&mut self) -> std::io::Result<()>`: si no hay `edit_buf`, no hace nada (`Ok(())`); si lo hay, **primero** sustituye `self.view` por una vista vacía en memoria (soltando así el `Mmap` del archivo original) y **después** llama a `notty_io::atomic_write(&self.path, edit_buf)`, y limpia `dirty`.

- [x] **Step 1: Escribir los tests que fallan**

`crates/notty-ui/src/raw_doc.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn opens_with_the_same_bytes_as_the_file() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8, 2, 3, 4]).unwrap();
        let doc = open_raw_doc(&p).unwrap();
        assert_eq!(doc.len(), 4);
        assert_eq!(doc.byte(2), 3);
        assert!(!doc.is_editing());
        assert!(!doc.is_dirty());
    }

    #[test]
    fn writable_file_reports_writable_fs() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8]).unwrap();
        assert!(open_raw_doc(&p).unwrap().writable_fs());
    }

    #[test]
    #[allow(clippy::permissions_set_readonly_false)]
    fn readonly_file_reports_not_writable_and_enable_write_fails() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8]).unwrap();
        let mut perms = fs::metadata(&p).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&p, perms.clone()).unwrap();
        let mut doc = open_raw_doc(&p).unwrap();
        assert!(!doc.writable_fs());
        assert!(!doc.enable_write());
        assert!(!doc.is_editing());
        perms.set_readonly(false);
        fs::set_permissions(&p, perms).unwrap();
    }

    #[test]
    fn enable_write_then_set_byte_updates_in_memory_only() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8, 2, 3]).unwrap();
        let mut doc = open_raw_doc(&p).unwrap();
        assert!(doc.enable_write());
        assert!(doc.set_byte(1, 0xFF));
        assert_eq!(doc.byte(1), 0xFF);
        assert!(doc.is_dirty());
        assert_eq!(fs::read(&p).unwrap(), vec![1, 2, 3]); // el disco no cambia todavía
    }

    #[test]
    fn set_byte_without_enable_write_fails() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8]).unwrap();
        let mut doc = open_raw_doc(&p).unwrap();
        assert!(!doc.set_byte(0, 9));
    }

    #[test]
    fn save_writes_the_edited_bytes_and_clears_dirty() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8, 2, 3]).unwrap();
        let mut doc = open_raw_doc(&p).unwrap();
        doc.enable_write();
        doc.set_byte(0, 9);
        doc.save().unwrap();
        assert!(!doc.is_dirty());
        assert_eq!(fs::read(&p).unwrap(), vec![9, 2, 3]);
    }

    #[test]
    fn save_without_edits_is_a_no_op() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8]).unwrap();
        let mut doc = open_raw_doc(&p).unwrap();
        assert!(doc.save().is_ok());
        assert_eq!(fs::read(&p).unwrap(), vec![1]);
    }
}
```

- [x] **Step 2: Ejecutar y ver que falla**

Añadir `mod raw_doc;` a `crates/notty-ui/src/lib.rs` para que compile el archivo:

```rust
mod raw_doc;
```

Run: `cargo test -p notty-ui raw_doc`
Expected: FAIL de compilación, `cannot find function open_raw_doc`.

- [x] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-ui/src/raw_doc.rs`:

```rust
use std::io;
use std::path::{Path, PathBuf};

use notty_io::{Opened, RawBytes};

pub struct RawDoc {
    path: PathBuf,
    view: RawBytes,
    writable_fs: bool,
    edit_buf: Option<Vec<u8>>,
    dirty: bool,
}

pub fn open_raw_doc(path: &Path) -> io::Result<RawDoc> {
    match notty_io::open_raw(path)? {
        Opened::Raw { bytes, writable } => {
            Ok(RawDoc { path: path.to_path_buf(), view: bytes, writable_fs: writable, edit_buf: None, dirty: false })
        }
        Opened::Text { .. } => Err(io::Error::new(io::ErrorKind::InvalidData, "open_raw debería devolver siempre Raw")),
    }
}

impl RawDoc {
    pub fn len(&self) -> usize {
        self.edit_buf.as_ref().map_or_else(|| self.view.len(), |v| v.len())
    }

    pub fn byte(&self, i: usize) -> u8 {
        self.edit_buf.as_ref().map_or_else(|| self.view[i], |v| v[i])
    }

    pub fn writable_fs(&self) -> bool {
        self.writable_fs
    }

    pub fn is_editing(&self) -> bool {
        self.edit_buf.is_some()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn enable_write(&mut self) -> bool {
        if !self.writable_fs {
            return false;
        }
        if self.edit_buf.is_none() {
            self.edit_buf = Some(self.view.to_vec());
        }
        true
    }

    pub fn set_byte(&mut self, i: usize, value: u8) -> bool {
        match &mut self.edit_buf {
            Some(buf) if i < buf.len() => {
                buf[i] = value;
                self.dirty = true;
                true
            }
            _ => false,
        }
    }

    /// Guarda los bytes editados. Suelta la vista mapeada del archivo *antes* de
    /// escribir, para no intentar nunca renombrar encima de un `Mmap` abierto.
    pub fn save(&mut self) -> io::Result<()> {
        let Some(buf) = self.edit_buf.take() else { return Ok(()) };
        self.view = RawBytes::Owned(Vec::new());
        notty_io::atomic_write(&self.path, &buf)?;
        self.edit_buf = Some(buf);
        self.dirty = false;
        Ok(())
    }
}
```

`RawBytes` necesita ser `Clone`-friendly para `.to_vec()`: comprobar que `notty_io::RawBytes` implementa `Deref<Target = [u8]>` (ya lo hace desde el Plan 1), lo que da `.to_vec()` gratis vía el `Deref` a `[u8]`. Si `RawBytes::Owned(Vec::new())` no es directamente construible por ser sus variantes privadas, exportar en `notty-io` un constructor `pub fn empty() -> RawBytes { RawBytes::Owned(Vec::new()) }` y usarlo aquí en su lugar (ajusta `notty-io/src/open.rs` si hace falta).

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, +7 tests sobre los de la Task 3.

- [x] **Step 5: Exportar el módulo**

`crates/notty-ui/src/lib.rs`: cambiar `mod raw_doc;` (Step 2) por:

```rust
pub use raw_doc::{RawDoc, open_raw_doc};
```

(mantén `mod raw_doc;` implícito en el propio `pub use`, no hace falta declararlo dos veces).

- [x] **Step 6: Commit**

```bash
git add crates/notty-io crates/notty-ui
git commit -m "feat(ui): RawDoc — vista hex de solo lectura con copia editable perezosa"
```

---

### Task 5: notty-ui::hex — formateo de la cuadrícula hexadecimal (lógica pura)

**Files:**
- Create: `crates/notty-ui/src/hex.rs`
- Modify: `crates/notty-ui/src/lib.rs`

**Interfaces:**
- Consumes: `RawDoc` (solo lectura, vía `len`/`byte`).
- Produces:
  - `pub fn hex_row(offset: usize, bytes: &[u8]) -> String`: una fila de hasta 16 bytes con el formato `"00000000   4D 5A 90 00 01 02 03 04  05 06 07 08 09 0A 0B 0C  MZ..........."` (offset de 8 dígitos hex en minúscula-o-mayúscula da igual con tal de ser consistente — usa mayúsculas para los bytes y el offset, como en la maqueta; separa los 8 primeros bytes de los 8 últimos con un espacio doble; la parte ASCII muestra el carácter si está entre `0x20` y `0x7E`, si no un `.`).
  - `pub fn hex_rows(doc: &RawDoc) -> Vec<String>`: una fila por cada bloque de 16 bytes de `doc` (la última puede ser más corta).
  - `pub fn hex_char(c: char) -> Option<u8>`: `'0'..='9'` → 0-9, `'a'..='f'`/`'A'..='F'` → 10-15, cualquier otra cosa `None` (para interpretar lo que el usuario teclea al escribir un byte).

- [x] **Step 1: Escribir los tests que fallan**

`crates/notty-ui/src/hex.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_a_short_row_with_ascii() {
        let row = hex_row(0, b"MZ\x90\x00");
        assert!(row.starts_with("00000000"));
        assert!(row.contains("4D 5A 90 00"));
        assert!(row.ends_with("MZ.."));
    }

    #[test]
    fn non_printable_bytes_become_dots() {
        let row = hex_row(0, &[0, 1, 65]);
        assert!(row.ends_with("..A"));
    }

    #[test]
    fn offset_is_eight_hex_digits() {
        let row = hex_row(0x1234, b"a");
        assert!(row.starts_with("00001234"));
    }

    #[test]
    fn hex_rows_splits_into_blocks_of_sixteen() {
        let bytes: Vec<u8> = (0..20).collect();
        struct Fake(Vec<u8>);
        // hex_rows recibe un RawDoc real en producción; aquí probamos la función
        // interna de bloques directamente para no depender de crear un archivo.
        let rows = hex_rows_from_bytes(&bytes);
        assert_eq!(rows.len(), 2);
        assert!(rows[1].starts_with("00000010"));
    }

    #[test]
    fn hex_char_parses_digits_and_letters() {
        assert_eq!(hex_char('0'), Some(0));
        assert_eq!(hex_char('9'), Some(9));
        assert_eq!(hex_char('a'), Some(10));
        assert_eq!(hex_char('F'), Some(15));
        assert_eq!(hex_char('g'), None);
    }
}
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui hex`
Expected: FAIL de compilación, `cannot find function hex_row`.

- [x] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-ui/src/hex.rs`:

```rust
pub fn hex_row(offset: usize, bytes: &[u8]) -> String {
    let mut hex = String::new();
    for (i, b) in bytes.iter().enumerate() {
        hex.push_str(&format!("{b:02X} "));
        if i == 7 {
            hex.push(' ');
        }
    }
    for i in bytes.len()..16 {
        hex.push_str("   ");
        if i == 7 {
            hex.push(' ');
        }
    }
    let ascii: String = bytes.iter().map(|&b| if (0x20..=0x7E).contains(&b) { b as char } else { '.' }).collect();
    format!("{offset:08X}   {hex} {ascii}")
}

/// Divide una secuencia de bytes en filas de 16 y las formatea. Función interna
/// reutilizada por `hex_rows`, testeable sin necesidad de un `RawDoc`/archivo real.
fn hex_rows_from_bytes(bytes: &[u8]) -> Vec<String> {
    bytes.chunks(16).enumerate().map(|(i, chunk)| hex_row(i * 16, chunk)).collect()
}

pub fn hex_rows(doc: &crate::RawDoc) -> Vec<String> {
    let all: Vec<u8> = (0..doc.len()).map(|i| doc.byte(i)).collect();
    hex_rows_from_bytes(&all)
}

pub fn hex_char(c: char) -> Option<u8> {
    c.to_digit(16).map(|d| d as u8)
}
```

(el test `hex_rows_splits_into_blocks_of_sixteen` llama directamente a `hex_rows_from_bytes`, que no es `pub`; como está en el mismo archivo, el `mod tests` interno sí puede verla vía `use super::*`. Borra la `struct Fake(Vec<u8>);` del test, que no se usa: era solo para ilustrar la idea en el enunciado y no debe quedar en el código final.)

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, +5 tests sobre los de la Task 4.

- [x] **Step 5: Exportar el módulo**

`crates/notty-ui/src/lib.rs` añade:

```rust
mod hex;

pub use hex::{hex_char, hex_row, hex_rows};
```

- [x] **Step 6: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): formateo de la cuadrícula hexadecimal"
```

---

### Task 6: window.rs — enrutar el teclado a vim y a raw

**Files:**
- Modify: `crates/notty-ui/src/window.rs`
- Modify: `crates/notty-ui/src/workspace.rs`

**Interfaces:**
- Consumes: `VimState`, `VimOutcome`, `VimCmd`, `parse_vim_cmd`, `RawDoc`, `open_raw_doc`, `EditorAction::{ToggleVim, ToggleRaw}`.
- Produces: cada `EditorState` (o el `Workspace` que los contiene) gana la posibilidad de estar en modo raw (`Option<RawDoc>` en vez de/además del `Document` de texto) y la posibilidad de tener vim activo (`vim: Option<VimState>`, `None` = modo normal de toda la vida).

- [x] **Step 1: Ampliar el estado del documento**

En `crates/notty-ui/src/editor.rs` (o donde viva `EditorState` desde el Plan 2), añadir dos campos:

```rust
pub struct EditorState {
    // ... campos ya existentes de los Planes 2-4 (doc, viewport, encoding, eol, path) ...
    pub vim: Option<crate::VimState>,
    pub raw: Option<crate::RawDoc>,
}
```

Actualiza `EditorState::new_empty()` y `EditorState::from_opened(..)` para inicializar ambos a `None`. Si `cfg.ui` tiene (desde un plan futuro) un ajuste de "vim siempre activo", este plan no lo consulta todavía al crear documentos nuevos — el toggle es siempre manual con `Ctrl+Alt+V`, y `Task 8` de este mismo plan añade la casilla de Ajustes sin conectarla aún a "crear ya en modo vim" (queda anotado como trabajo futuro en su propio Step).

- [x] **Step 2: `ToggleVim` y `ToggleRaw` en window.rs**

En el `match action` de `WM_KEYDOWN`:

```rust
crate::EditorAction::ToggleVim => {
    let st = ws.active_mut();
    st.vim = if st.vim.is_some() { None } else { Some(crate::VimState::default()) };
}
crate::EditorAction::ToggleRaw => {
    let st = ws.active_mut();
    if let Some(raw) = st.raw.take() {
        // Estaba en raw: si el texto sigue siendo válido UTF-8, vuelve a texto.
        let bytes: Vec<u8> = (0..raw.len()).map(|i| raw.byte(i)).collect();
        if let Ok(text) = String::from_utf8(bytes) {
            st.doc = notty_core::Document::new(&text, st.eol.as_str());
        } else {
            st.raw = Some(raw); // no era texto válido: se queda en raw
        }
    } else if let Some(path) = &st.path {
        if let Ok(raw) = crate::open_raw_doc(path) {
            st.raw = Some(raw);
        }
    }
}
```

- [x] **Step 3: Enrutar `WM_KEYDOWN`/`WM_CHAR` cuando hay vim activo**

Antes del `match` que traduce `vk` con `notty_ui::action_for_vk` (o justo después de descartar `ToggleVim`/`ToggleRaw`, que siempre deben funcionar aunque vim esté activo), si `ws.active().vim.is_some()` y `ws.active().raw.is_none()`:

```rust
let outcome = {
    let st = ws.active_mut();
    let mut vim = st.vim.take().unwrap();
    let out = vim.handle_key(&mut st.doc, vk, char_from_wm_char /* ver nota */, std::time::Instant::now());
    st.vim = Some(vim);
    out
};
match outcome {
    crate::VimOutcome::Handled => { /* ya se aplicó; solo falta InvalidateRect */ }
    crate::VimOutcome::OpenFind => ws.prompt = crate::Prompt::Find(crate::SearchState::default()),
    crate::VimOutcome::OpenCmdline => ws.prompt = crate::Prompt::Path(/* reutiliza el mecanismo de línea que ya tienes, o añade una variante Cmdline a Prompt si tu Plan 4 la modeló aparte */ todo!()),
    crate::VimOutcome::Bubble => { /* cae al camino normal: action_for_vk + editor de siempre */ }
}
```

Nota importante sobre `VimOutcome::OpenCmdline`: si en tu implementación del Plan 4 el `Prompt::Path`/`Prompt::Find` no encajan para una línea `:comando`, añade una variante nueva `Prompt::VimCmdline(String)` en `crates/notty-ui/src/prompt.rs` (un simple `String` que se va rellenando con lo que el usuario teclea tras `:`, sin necesidad de sugerencias ni normalización de ruta) y, al pulsar `Enter` sobre ella, llama a `notty_ui::parse_vim_cmd(&line)` y ejecuta el `VimCmd` resultante (`Save`/`Quit`/`SaveAndQuit` sobre `ws.active_mut()`, `Substitute` vía `ws.active_mut().doc.replace_all(...)`, `Unknown` se descarta sin más). Ajusta también `Renderer::paint` (Task 7) para dibujar esta variante como `":" + lo tecleado`, igual que la maqueta.

`char_from_wm_char`: como `WM_KEYDOWN` no trae el carácter Unicode (eso llega por separado en `WM_CHAR`), la forma más simple de encajar esto con el `VimState::handle_key(doc, vk, ch: Option<char>, now)` tal como está definido es **no** llamar a `handle_key` desde `WM_KEYDOWN` para las teclas que son letras, sino desde `WM_CHAR` (pasando `vk = 0` y `ch = Some(carácter)`), y reservar la llamada desde `WM_KEYDOWN` solo para `Esc` (`vk = 0x1B, ch = None`) y para las teclas de flecha si decides tratarlas como parte de vim en vez de dejar que `hjkl` (que sí son letras y llegan por `WM_CHAR`) cubra el movimiento — la maqueta y los tests de la Task 2 asumen que todo el movimiento vim es por letras (`hjkl`, no las flechas), así que en la práctica basta con: `WM_CHAR` con vim activo y modo `Normal`/`Visual` → `handle_key(doc, 0, Some(ch), now)`; `WM_CHAR` con vim activo y modo `Insert` → igual, se inserta el texto; `WM_KEYDOWN` con vim activo → solo mira `Esc`, `ToggleVim`/`ToggleRaw` (que deben funcionar siempre) y dejar pasar el resto (flechas, `Ctrl+S`, etc. dentro de vim si quieres que sigan funcionando como atajos "de escape hatch" además de los suyos propios — comportamiento razonable y ya cubierto por que `Bubble`/el camino normal siga disponible).

- [x] **Step 4: Enrutar el teclado cuando hay un `RawDoc` activo**

Cuando `ws.active().raw.is_some()`, en vez del camino de texto:
- `WM_CHAR` con un dígito hex (`notty_ui::hex_char(ch)`): si hay un byte seleccionado y `raw.is_editing()`, compón el nibble alto/bajo (mismo patrón que la maqueta: primera pulsación guarda el nibble alto, segunda escribe el byte completo con `raw.set_byte(idx, val)` y avanza la selección) y si `!raw.is_editing()`, muestra un aviso ("solo lectura" o "sin permiso de escritura" según `raw.writable_fs()`) sin escribir nada.
- Flechas: mueven el byte seleccionado (`±1` para izquierda/derecha, `±16` para arriba/abajo), sin salirse de `0..raw.len()`.
- `Ctrl+S`: `raw.save()`.
- Un nuevo comando (el lápiz): puede ser un botón dibujado en la barra de estado (Task 7) al que se hace hit-testing en `WM_LBUTTONDOWN`, que llama a `raw.enable_write()`.

- [x] **Step 5: Compilar**

Run: `cargo build --workspace`
Expected: compila. Ajusta la organización exacta de `window.rs` (nombres de las funciones internas, cómo se guarda el estado por ventana) a como haya quedado tras los Planes 2-4; lo que importa es que el comportamiento descrito en los Steps 2-4 quede implementado.

- [x] **Step 6: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): enruta el teclado a vim y a la vista raw"
```

---

### Task 7: Render: indicador de modo vim, cuadrícula hexadecimal y lápiz

**Files:**
- Modify: `crates/notty-ui/src/render.rs`

**Interfaces:**
- Consumes: `EditorState.vim`, `EditorState.raw`, `hex_rows`, `Prompt::VimCmdline` (si la añadiste en la Task 6).
- Produces: `Renderer::paint` dibuja, cuando `state.raw.is_some()`, la cuadrícula hexadecimal en vez del texto normal (una `IDWriteTextLayout` monoespaciada por fila de `hex_rows(raw)`, con el byte seleccionado resaltado con `sel_brush` y los bytes modificados (`raw.is_editing()` y distintos del valor original — si no guardas el original por separado, basta con resaltar todos los bytes mientras `is_editing()` sea `true` y aún no se haya guardado) en el color de acento); en la barra de estado, un icono de lápiz (dibuja un pequeño trazo con `FillRectangle`/`DrawLine`, no hace falta un glifo real) que aparece atenuado/deshabilitado si `!raw.writable_fs()` y resaltado si `raw.is_editing()`. Cuando `state.vim.is_some()`, la barra de estado muestra `"-- NORMAL --"` o `"-- INSERT --"` (o `"-- VISUAL --"`) según `state.vim.as_ref().unwrap().mode`, en el color de acento, a la izquierda de donde antes iba `status_line`.

- [x] **Step 1: Implementar**

Modifica `Renderer::paint`:

1. Si `ws.active().raw.is_some()`, sustituye el bucle que dibuja líneas de texto por uno que recorre `notty_ui::hex_rows(raw)` con el mismo `line_height`, dibujando cada fila completa (offset + hex + ascii) como una sola `IDWriteTextLayout`. El scroll (`Viewport`) se puede reutilizar tratando cada fila hex como si fuera una "línea" a efectos de `first_line`/`visible_lines` (16 bytes por fila).
2. Resalta el byte seleccionado calculando su fila (`selected / 16`) y columna dentro de la fila para pintar un rectángulo sobre esa posición de la parte hex (y opcionalmente sobre la parte ascii correspondiente), con `HitTestTextPosition` sobre el texto de esa fila igual que ya se hace para el caret de texto normal.
3. Si `ws.active().vim.is_some()`, antepone `"-- NORMAL --"`/`"-- INSERT --"`/`"-- VISUAL --"` (según el `mode`) a la izquierda de la barra de estado, con `accent`-ish color (reutiliza `fg_brush` con opacidad distinta si no quieres crear un pincel nuevo, o crea uno con el mismo tono de acento que ya usa la selección).
4. Si añadiste `Prompt::VimCmdline(String)` en la Task 6, dibújala en la franja de prompt como `":" + texto`, con el mismo estilo que el resto de prompts.

Ajusta cualquier firma de `windows` que haga falta al compilar.

- [x] **Step 2: Compilar y comprobar manualmente**

Run: `cargo build --workspace`
Expected: compila.

Preparar un binario de prueba pequeño (por ejemplo copiar cualquier `.exe` corto, o `printf '\x89PNG\r\n\x1a\n' > /tmp/prueba.bin`).

Run: `cargo run --bin notty -- /tmp/prueba.bin`
Expected (manual): se abre directamente en vista raw (por no ser texto), con offset/hex/ascii, solo lectura; el lápiz está activo o no según los permisos del archivo; al pulsarlo, escribir un byte con dígitos hex lo cambia en pantalla; `Ctrl+S` lo guarda (comprobar con `fc`/`certutil -hashfile` o abriendo el archivo con otra herramienta que los bytes cambiaron).

Run: `cargo run --bin notty -- /tmp/notas.txt` y pulsar `Ctrl+Alt+V`.
Expected (manual): la barra de estado muestra `-- NORMAL --`; `i` cambia a `-- INSERT --` y se puede escribir normalmente; `Esc` vuelve a `-- NORMAL --`; `hjkl`, `dd`, `x`, `gg`/`G`, `u` funcionan como en vim; `:w` guarda y `:q` (en esta primera versión, sin gestión de "cerrar ventana si es la última", puede simplemente cerrar la pestaña activa reutilizando `ws.close_active()`).

- [x] **Step 3: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): dibuja el indicador de modo vim y la cuadrícula hexadecimal"
```

---

### Task 8: Ajustes — "Modo vim siempre" y atajo de texto↔raw

**Files:**
- Modify: `crates/notty-config/src/model.rs`
- Modify: `crates/notty-ui/src/settings_window.rs`

**Interfaces:**
- Consumes: nada nuevo de otros crates.
- Produces: `UiConfig` gana `pub vim_always: bool` (`#[serde(default)]`, `false` por defecto). La ventana de Ajustes (Plan 3) gana una casilla más, "Modo vim siempre", en la misma sección donde ya estaban preset/tema/números de línea.

- [x] **Step 1: Escribir el test que falla**

Añadir a `mod tests` de `crates/notty-config/src/model.rs`:

```rust
    #[test]
    fn vim_always_defaults_to_false_and_round_trips() {
        assert!(!UiConfig::default().vim_always);
        let cfg = Config { ui: UiConfig { vim_always: true, ..UiConfig::default() } };
        let text = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert!(back.ui.vim_always);
    }
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-config model`
Expected: FAIL de compilación, `no field vim_always on type UiConfig`.

- [x] **Step 3: Implementar**

En `crates/notty-config/src/model.rs`, añadir el campo a `UiConfig` (junto a `hints_bar`/`status_bar`) y a su `Default`:

```rust
    pub hints_bar: bool,
    pub status_bar: bool,
    pub vim_always: bool,
```

```rust
            hints_bar: true,
            status_bar: true,
            vim_always: false,
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, +1 test.

- [x] **Step 5: Añadir la casilla a la ventana de Ajustes**

En `crates/notty-ui/src/settings_window.rs`, añadir un id nuevo y una llamada a `checkbox(...)` junto a la de `ID_LINE_NUMBERS` (mismo patrón que ya existe del Plan 3):

```rust
const ID_VIM_ALWAYS: usize = 121;
```

```rust
checkbox(hwnd, instance, "Modo vim siempre", 20, 240, ID_VIM_ALWAYS, cfg.borrow().ui.vim_always);
```

Y en `apply_control`:

```rust
        ID_VIM_ALWAYS => cfg.ui.vim_always = !cfg.ui.vim_always,
```

(Reposiciona en Y las filas que venían después, como el grupo de pestañas/buffers, para que no se solapen con la casilla nueva.)

Nota de alcance: este plan **no** conecta todavía `vim_always` con la creación de documentos nuevos (`EditorState::new_empty()`/`from_opened()` seguirían arrancando siempre con `vim: None`). Dejarlo así, documentado, es intencional: conectar la config con el arranque de cada documento es un cambio de una línea que encaja mejor junto al resto del trabajo de "instancia única y arranque" del Plan 6, para no tocar `window.rs` dos veces por lo mismo.

- [x] **Step 6: Compilar**

Run: `cargo build --workspace`
Expected: compila.

- [x] **Step 7: Commit**

```bash
git add crates/notty-config crates/notty-ui
git commit -m "feat(config): opción 'modo vim siempre' en Ajustes"
```

---

### Task 9: Comprobación final del workspace

**Files:** ninguno nuevo; solo verificación.

- [ ] **Step 1: Tests y lints de todo el workspace**

Run: `cargo test --workspace`
Expected: PASS. Cuenta los tests reales con el propio `cargo test` (los números de este plan son una guía, no una cifra exacta garantizada, igual que en planes anteriores).

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: sin avisos. Corregir cualquier aviso en el archivo que lo señale y repetir hasta que quede limpio.

- [ ] **Step 2: Build release**

Run: `cargo build --release --workspace`
Expected: compila sin errores.

- [ ] **Step 3: Commit (si hubo cambios de la revisión)**

```bash
git add -A
git commit -m "chore(ui): pasa clippy y build release tras vim y raw" --allow-empty
```

---

## Hoja de ruta: planes siguientes

| Plan | Contenido |
|---|---|
| **6 · Archivos vivos** | Temporales (`CLICKME`, borrador/volátil), instancia única, daemon/atajo global, autoguardado, conflictos, recuperación tras caída, resto de "Archivos" y "Atajo global" de Ajustes, conectar `vim_always` al crear documentos. |
| **7 · Pulido y garantías** | UI Automation, alto contraste/DPI, fuzzing, presupuesto de rendimiento en CI. |
