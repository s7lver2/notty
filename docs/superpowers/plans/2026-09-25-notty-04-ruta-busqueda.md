# notty · Plan 4: Línea de ruta y búsqueda Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `Ctrl+O` (abrir) y `Ctrl+S` sin ruta activan una línea de ruta en la barra de estado con normalización, sugerencia fantasma, hasta 5 candidatos navegables (`↑`/`↓`/`Tab`/clic) y creación automática de carpetas. `Ctrl+F`/`Ctrl+H` activan buscar/reemplazar con resaltado en vivo, contador, `Alt+C/W/R` y `F3`/`Shift+F3`.

**Architecture:** La lógica de rutas (normalizar, sugerir, detectar "nuevo"/"existe"/carpeta) vive en `notty-io::pathline`, testeada contra un sistema de archivos real de prueba (`tempdir`), igual que el resto de `notty-io`. La lógica de búsqueda reutiliza `notty_core::Document::find_all`/`replace_all` (ya implementados en el Plan 1) envuelta en un `SearchState` que recuerda la coincidencia actual. Ambas viven en `notty-ui` como estado de un único `Prompt` guardado en el `Workspace` (Plan 3), y solo la parte de dibujar la línea y las sugerencias en pantalla toca Win32/Direct2D.

**Tech Stack:** Rust stable 1.96 (MSVC), `notty-core`/`notty-io`/`notty-ui`/`notty-config`/`notty-input` (Planes 1-3), `windows` (ya en el workspace).

**Spec:** `docs/superpowers/specs/2026-09-24-notty-design.md`
**Planes anteriores:** Plan 1 (`2026-09-24-notty-01-nucleo.md`), Plan 2 (`2026-09-25-notty-02-ventana-editor.md`), Plan 3 (`2026-09-25-notty-03-config-ventana.md`)

## Global Constraints

- Solo Windows 10/11; toolchain `stable-x86_64-pc-windows-msvc`.
- `notty-io::pathline` no depende de Win32 ni de `notty-ui`; se testea con `tempfile::tempdir()` como el resto de `notty-io`.
- La búsqueda reutiliza `notty_core::Document::find_all`/`replace_all`; no se reimplementa el matching aquí.
- **Fuera de alcance de este plan:** el concepto `CLICKME` como etiqueta de la barra de estado y la creación de archivos temporales/permanentes son del Plan 6 ("Archivos vivos"). Aquí solo se construye el widget genérico de línea de ruta y se conecta a `Ctrl+O` (abrir) y a `Ctrl+S` cuando el documento activo no tiene ruta (en vez de fallar como hacía el Plan 2, abre este prompt con el propósito de guardar).
- Textos visibles para el usuario en español.
- Un commit por tarea terminada.
- **Sobre el código Win32/Direct2D de este plan** (dibujar la línea de prompt, la lista de sugerencias, el resaltado de coincidencias): se da la arquitectura y las llamadas por su nombre. Compílalo, y si una firma no coincide con la versión de `windows` instalada, ajústala consultando `cargo doc -p windows --open` o el error del compilador, sin cambiar el comportamiento descrito. Los pasos marcados como "lógica pura" son código exacto.

## File Structure

```
crates/notty-io/src/pathline.rs         normalizar, sugerencias, "nuevo"/"existe", salto difuso (nuevo)
crates/notty-ui/src/path_prompt.rs      PathPromptState: envuelve pathline + índice de selección (nuevo)
crates/notty-ui/src/search_prompt.rs    SearchState: envuelve Document::find_all/replace_all (nuevo)
crates/notty-ui/src/prompt.rs           enum Prompt { None, Path(..), Find(..), Replace(..) } (nuevo)
crates/notty-ui/src/keymap.rs           (modificado) Replace, FindNext, FindPrev, OpenPathPrompt
crates/notty-ui/src/workspace.rs        (modificado) campo `prompt: Prompt`
crates/notty-ui/src/render.rs           (modificado) dibuja la línea de prompt y el resaltado de búsqueda
crates/notty-ui/src/window.rs           (modificado) conecta teclado a los prompts
```

---

### Task 1: keymap: Replace, FindNext, FindPrev, OpenPathPrompt (lógica pura)

**Files:**
- Modify: `crates/notty-ui/src/keymap.rs`

**Interfaces:**
- Consumes: nada nuevo.
- Produces: `EditorAction` gana las variantes `Replace, FindNext, FindPrev, OpenPathPrompt`. `action_for_vk` mapea `Ctrl+H` (`0x48`) → `Replace`, `F3` (`0x72`) sin modificadores → `FindNext`, `F3` con `Shift` → `FindPrev`, `Ctrl+O` (`0x4F`) → `OpenPathPrompt`.

- [ ] **Step 1: Escribir los tests que fallan**

Añadir a `mod tests` en `crates/notty-ui/src/keymap.rs`:

```rust
    #[test]
    fn ctrl_h_is_replace() {
        assert_eq!(action_for_vk(0x48, m(true, false)), EditorAction::Replace);
    }

    #[test]
    fn f3_navigates_matches() {
        assert_eq!(action_for_vk(0x72, m(false, false)), EditorAction::FindNext);
        assert_eq!(action_for_vk(0x72, m(false, true)), EditorAction::FindPrev);
    }

    #[test]
    fn ctrl_o_opens_path_prompt() {
        assert_eq!(action_for_vk(0x4F, m(true, false)), EditorAction::OpenPathPrompt);
    }
```

- [ ] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui keymap`
Expected: FAIL de compilación, `no variant named Replace found for enum EditorAction`.

- [ ] **Step 3: Implementar**

En `crates/notty-ui/src/keymap.rs`, ampliar el `enum EditorAction`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    MoveLeft, MoveRight, MoveUp, MoveDown, MoveHome, MoveEnd, MoveDocStart, MoveDocEnd,
    ExtendLeft, ExtendRight, ExtendUp, ExtendDown, ExtendHome, ExtendEnd,
    Backspace, DeleteForward, InsertNewline,
    Undo, Redo, SelectAll, Copy, Cut, Paste, Save, Find, Replace, FindNext, FindPrev, OpenPathPrompt,
    None,
}
```

Y añadir estos brazos al `match` de `action_for_vk` (junto a los de `Save`/`Find` ya existentes):

```rust
        (0x48, true, _) => Replace,
        (0x72, false, false) => FindNext,
        (0x72, false, true) => FindPrev,
        (0x4F, true, _) => OpenPathPrompt,
```

(`0x72` es `F3`; como no lleva `Ctrl`, el patrón `(vk, ctrl, shift)` usa `false` en la posición de `ctrl` explícitamente en vez de `_`, para no chocar con el resto de combinaciones de `F3`.)

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS. `notty-ui` gana 3 tests sobre los que tenía.

- [ ] **Step 5: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): acciones de teclado para reemplazar, F3 y abrir la línea de ruta"
```

---

### Task 2: notty-io::pathline — normalizar (lógica pura)

**Files:**
- Create: `crates/notty-io/src/pathline.rs`
- Modify: `crates/notty-io/src/lib.rs`

**Interfaces:**
- Consumes: nada.
- Produces:
  - `pub struct PathContext { pub home: std::path::PathBuf, pub current_dir: Option<std::path::PathBuf> }`.
  - `pub fn normalize(raw: &str, ctx: &PathContext) -> String`: convierte `/` en `\`, colapsa barras repetidas, recorta espacios iniciales; si el resultado empieza por `~`, lo sustituye por `ctx.home`; si empieza por `.\` o es exactamente `.`, lo sustituye por `ctx.current_dir` (si es `None`, dejamos el `.` tal cual); expande `%NOMBRE%` con `std::env::var` cuando la variable existe (si no existe, se deja el texto tal cual, sin fallar).
  - `pub fn home_dir() -> std::path::PathBuf`: lee `%USERPROFILE%`; si no existe (solo en pruebas fuera de Windows), cae a `.`.

- [ ] **Step 1: Preparar el módulo y los tests que fallan**

`crates/notty-io/src/pathline.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx() -> PathContext {
        PathContext { home: PathBuf::from(r"C:\Users\ana"), current_dir: Some(PathBuf::from(r"C:\Users\ana\Documentos\notty")) }
    }

    #[test]
    fn converts_forward_slashes() {
        assert_eq!(normalize("Documentos/proyectos", &ctx()), r"Documentos\proyectos");
    }

    #[test]
    fn collapses_double_backslashes() {
        assert_eq!(normalize(r"Documentos\\proyectos", &ctx()), r"Documentos\proyectos");
    }

    #[test]
    fn trims_leading_spaces() {
        assert_eq!(normalize("   nota.txt", &ctx()), "nota.txt");
    }

    #[test]
    fn expands_home_tilde() {
        assert_eq!(normalize(r"~\Documentos", &ctx()), r"C:\Users\ana\Documentos");
    }

    #[test]
    fn expands_dot_to_current_dir() {
        assert_eq!(normalize(r".\config.toml", &ctx()), r"C:\Users\ana\Documentos\notty\config.toml");
    }

    #[test]
    fn expands_known_env_var() {
        // TEMP siempre existe en Windows; si el valor difiere entre máquinas, solo
        // comprobamos que ya no queda el literal "%TEMP%" en el resultado.
        let out = normalize("%TEMP%\\a.txt", &ctx());
        assert!(!out.contains("%TEMP%"));
    }

    #[test]
    fn unknown_env_var_is_left_as_is() {
        assert_eq!(normalize("%NO_EXISTE_ESTA_VAR%\\a.txt", &ctx()), r"%NO_EXISTE_ESTA_VAR%\a.txt");
    }
}
```

- [ ] **Step 2: Ejecutar y ver que falla**

Añadir `mod pathline;` a `crates/notty-io/src/lib.rs` (sin re-export todavía) para que el archivo se compile:

```rust
mod pathline;
```

Run: `cargo test -p notty-io pathline`
Expected: FAIL de compilación, `cannot find type PathContext`.

- [ ] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-io/src/pathline.rs`:

```rust
use std::path::PathBuf;

pub struct PathContext {
    pub home: PathBuf,
    pub current_dir: Option<PathBuf>,
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Normaliza lo que el usuario va escribiendo en la línea de ruta: separadores,
/// espacios sobrantes, `~`, `.` (carpeta del archivo actual) y `%VARIABLES%`.
pub fn normalize(raw: &str, ctx: &PathContext) -> String {
    let mut s = raw.trim_start().replace('/', "\\");
    while s.contains(r"\\") {
        s = s.replace(r"\\", r"\");
    }
    if s == "." || s.starts_with(r".\") {
        if let Some(dir) = &ctx.current_dir {
            let rest = s.strip_prefix('.').unwrap_or("");
            s = format!("{}{}", dir.display(), rest);
        }
    } else if s == "~" || s.starts_with(r"~\") {
        let rest = s.strip_prefix('~').unwrap_or("");
        s = format!("{}{}", ctx.home.display(), rest);
    }
    expand_env(&s)
}

fn expand_env(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(val) => out.push_str(&val),
                    Err(_) => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}
```

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test -p notty-io`
Expected: PASS, 27 + 7 = 34 tests en `notty-io`.

- [ ] **Step 5: Commit**

```bash
git add crates/notty-io
git commit -m "feat(io): normaliza la línea de ruta (barras, ~, ., %VAR%)"
```

---

### Task 3: notty-io::pathline — sugerencias y "nuevo"/"existe" (tempdir)

**Files:**
- Modify: `crates/notty-io/src/pathline.rs`
- Modify: `crates/notty-io/src/lib.rs`

**Interfaces:**
- Consumes: `std::fs::read_dir`, `std::fs::metadata`.
- Produces:
  - `pub struct Entry { pub name: String, pub is_dir: bool }`.
  - `pub fn suggestions(typed: &str, max: usize) -> Vec<Entry>`: separa `typed` en `(carpeta_padre, último_segmento)` por el último `\`; si la carpeta padre no existe o no es un directorio, devuelve `vec![]`; si existe, lista sus entradas cuyo nombre empiece por `último_segmento` (sin distinguir mayúsculas), ordenadas carpetas primero y luego alfabéticamente, recortadas a `max`. Con `último_segmento` vacío, no lista nada (igual que en la maqueta: las sugerencias solo aparecen al escribir algo).
  - `pub enum Hint { Empty, New, Exists, Dir, DirNew }`: `Empty` si `typed` está vacío; si el camino completo ya existe como carpeta → `Dir`; si existe como archivo → `Exists`; si no existe pero su carpeta padre sí → `New`; si ni la carpeta padre existe → `DirNew`.
  - `pub fn hint_for(typed: &str) -> Hint`.
  - `pub const INVALID_CHARS: &[char] = &[':', '*', '?', '"', '<', '>', '|']` y `pub fn has_invalid_chars(segment: &str) -> bool` (comprueba solo el último segmento, para no marcar como inválido el `:` de una unidad tipo `C:`).

- [ ] **Step 1: Escribir los tests que fallan**

Añadir a `mod tests` en `crates/notty-io/src/pathline.rs`:

```rust
    use tempfile::tempdir;

    fn setup() -> tempfile::TempDir {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join("proyectos")).unwrap();
        std::fs::create_dir(dir.path().join("proyectos-viejos")).unwrap();
        std::fs::write(dir.path().join("presupuesto.txt"), "x").unwrap();
        dir
    }

    #[test]
    fn suggestions_are_prefix_filtered_dirs_first() {
        let dir = setup();
        let typed = dir.path().join("pro").to_string_lossy().to_string();
        let s = suggestions(&typed, 5);
        assert_eq!(s.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["proyectos", "proyectos-viejos"]);
        assert!(s.iter().all(|e| e.is_dir));
    }

    #[test]
    fn suggestions_are_capped_at_max() {
        let dir = setup();
        let typed = dir.path().join("p").to_string_lossy().to_string();
        assert_eq!(suggestions(&typed, 2).len(), 2);
    }

    #[test]
    fn empty_last_segment_has_no_suggestions() {
        let dir = setup();
        let typed = format!("{}\\", dir.path().display());
        assert!(suggestions(&typed, 5).is_empty());
    }

    #[test]
    fn missing_parent_has_no_suggestions() {
        assert!(suggestions(r"Z:\no-existe-nunca\algo", 5).is_empty());
    }

    #[test]
    fn hint_empty_for_empty_input() {
        assert_eq!(hint_for(""), Hint::Empty);
    }

    #[test]
    fn hint_dir_for_existing_folder() {
        let dir = setup();
        assert_eq!(hint_for(&dir.path().join("proyectos").to_string_lossy()), Hint::Dir);
    }

    #[test]
    fn hint_exists_for_existing_file() {
        let dir = setup();
        assert_eq!(hint_for(&dir.path().join("presupuesto.txt").to_string_lossy()), Hint::Exists);
    }

    #[test]
    fn hint_new_when_parent_exists_but_file_does_not() {
        let dir = setup();
        assert_eq!(hint_for(&dir.path().join("nueva.txt").to_string_lossy()), Hint::New);
    }

    #[test]
    fn hint_dirnew_when_parent_is_missing_too() {
        let dir = setup();
        let typed = dir.path().join("nueva").join("sub").join("a.txt").to_string_lossy().to_string();
        assert_eq!(hint_for(&typed), Hint::DirNew);
    }

    #[test]
    fn invalid_chars_are_detected_in_last_segment_only() {
        assert!(!has_invalid_chars("C:"));
        assert!(has_invalid_chars("nota?.txt"));
        assert!(has_invalid_chars("a<b>.txt"));
        assert!(!has_invalid_chars("nota.txt"));
    }
```

- [ ] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-io pathline`
Expected: FAIL de compilación, `cannot find function suggestions`.

- [ ] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-io/src/pathline.rs` (después de `expand_env`):

```rust
use std::path::Path;

pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

fn split_last(typed: &str) -> (&str, &str) {
    match typed.rfind('\\') {
        Some(i) => (&typed[..i], &typed[i + 1..]),
        None => ("", typed),
    }
}

pub fn suggestions(typed: &str, max: usize) -> Vec<Entry> {
    let (parent, last) = split_last(typed);
    if last.is_empty() {
        return Vec::new();
    }
    let Ok(read) = std::fs::read_dir(parent) else { return Vec::new() };
    let mut entries: Vec<Entry> = read
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.to_lowercase().starts_with(&last.to_lowercase()).then(|| {
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                Entry { name, is_dir }
            })
        })
        .collect();
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    entries.truncate(max);
    entries
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    Empty,
    New,
    Exists,
    Dir,
    DirNew,
}

pub fn hint_for(typed: &str) -> Hint {
    if typed.is_empty() {
        return Hint::Empty;
    }
    let path = Path::new(typed);
    if let Ok(meta) = std::fs::metadata(path) {
        return if meta.is_dir() { Hint::Dir } else { Hint::Exists };
    }
    let (parent, _) = split_last(typed);
    if parent.is_empty() || Path::new(parent).is_dir() { Hint::New } else { Hint::DirNew }
}

pub const INVALID_CHARS: &[char] = &[':', '*', '?', '"', '<', '>', '|'];

pub fn has_invalid_chars(segment: &str) -> bool {
    segment.chars().any(|c| INVALID_CHARS.contains(&c))
}
```

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS. `notty-io` pasa a 44 tests; total del workspace 126 (de antes) + 17 (Tasks 2-3) = 143.

- [ ] **Step 5: Exportar el módulo**

`crates/notty-io/src/lib.rs` queda (añadiendo el `pub use` que faltaba):

```rust
//! notty-io: detección texto/raw, codificaciones, fin de línea y guardado atómico.

mod encoding;
mod eol;
mod fsutil;
mod open;
mod pathline;

pub use encoding::{CodecError, Detected, TextEncoding, decode, decode_lossy, detect, encode};
pub use eol::{LineEnding, convert, detect_eol};
pub use fsutil::{atomic_write, can_write, create_parent_dirs};
pub use open::{Opened, RawBytes, open, open_raw};
pub use pathline::{Entry, Hint, INVALID_CHARS, PathContext, has_invalid_chars, hint_for, home_dir, normalize, suggestions};
```

- [ ] **Step 6: Commit**

```bash
git add crates/notty-io
git commit -m "feat(io): sugerencias de ruta y detección nuevo/existe/carpeta"
```

---

### Task 4: notty-ui::path_prompt — estado de la línea de ruta (tempdir)

**Files:**
- Create: `crates/notty-ui/src/path_prompt.rs`
- Modify: `crates/notty-ui/src/lib.rs`

**Interfaces:**
- Consumes: `notty_io::{normalize, suggestions, hint_for, has_invalid_chars, Hint, Entry, PathContext}`.
- Produces:
  - `pub enum Purpose { Open, Save }`.
  - `pub struct PathPromptState { pub value: String, pub purpose: Purpose, pub selected: usize }` con:
    - `PathPromptState::new(purpose: Purpose, initial: String) -> Self`.
    - `type_text(&mut self, raw: &str, ctx: &notty_io::PathContext)`: normaliza y guarda en `value`, resetea `selected` a `0`.
    - `suggestions(&self) -> Vec<notty_io::Entry>` (máximo 5, delega en `notty_io::suggestions(&self.value, 5)`).
    - `hint(&self) -> notty_io::Hint`.
    - `is_invalid(&self) -> bool` (usa `has_invalid_chars` sobre el último segmento de `value`).
    - `ghost(&self) -> String`: si hay sugerencias, la diferencia entre `self.value` y la sugerencia seleccionada (con `\` final si es carpeta); si no hay sugerencias pero el hint es `New`/`DirNew` y el último segmento no tiene extensión, `".txt"`.
    - `move_selection(&mut self, delta: i32)`: mueve `selected` cíclicamente entre `0..suggestions().len()` (no hace nada si no hay sugerencias).
    - `accept(&mut self)`: sustituye el último segmento de `value` por la sugerencia seleccionada (añadiendo `\` si es carpeta); si ya coincidía exactamente con esa sugerencia y hay más de una, avanza a la siguiente antes de sustituir (mismo comportamiento que la maqueta: "Tab otra vez pasa a la siguiente").

- [ ] **Step 1: Escribir los tests que fallan**

`crates/notty-ui/src/path_prompt.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx(dir: &std::path::Path) -> notty_io::PathContext {
        notty_io::PathContext { home: dir.to_path_buf(), current_dir: None }
    }

    fn setup() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("proyectos")).unwrap();
        std::fs::create_dir(dir.path().join("proyectos-viejos")).unwrap();
        std::fs::write(dir.path().join("presupuesto.txt"), "x").unwrap();
        dir
    }

    #[test]
    fn typing_normalizes_and_resets_selection() {
        let mut p = PathPromptState::new(Purpose::Open, String::new());
        p.selected = 2;
        p.type_text("a/b", &ctx(&PathBuf::from(".")));
        assert_eq!(p.value, r"a\b");
        assert_eq!(p.selected, 0);
    }

    #[test]
    fn ghost_shows_rest_of_top_suggestion() {
        let dir = setup();
        let mut p = PathPromptState::new(Purpose::Open, String::new());
        p.type_text(&format!("{}\\pro", dir.path().display()), &ctx(dir.path()));
        assert_eq!(p.ghost(), r"yectos\");
    }

    #[test]
    fn arrow_moves_selection_and_changes_ghost() {
        let dir = setup();
        let mut p = PathPromptState::new(Purpose::Open, String::new());
        p.type_text(&format!("{}\\pro", dir.path().display()), &ctx(dir.path()));
        p.move_selection(1);
        assert_eq!(p.ghost(), r"yectos-viejos\");
    }

    #[test]
    fn accept_appends_the_selected_suggestion() {
        let dir = setup();
        let mut p = PathPromptState::new(Purpose::Open, String::new());
        p.type_text(&format!("{}\\pro", dir.path().display()), &ctx(dir.path()));
        p.accept();
        assert_eq!(p.value, format!("{}\\proyectos\\", dir.path().display()));
    }

    #[test]
    fn accepting_twice_cycles_to_next_sibling() {
        let dir = setup();
        let mut p = PathPromptState::new(Purpose::Open, String::new());
        p.type_text(&format!("{}\\pro", dir.path().display()), &ctx(dir.path()));
        p.accept();
        p.accept();
        assert_eq!(p.value, format!("{}\\proyectos-viejos\\", dir.path().display()));
    }

    #[test]
    fn hint_new_gets_default_extension_ghost_when_no_candidates() {
        let dir = setup();
        let mut p = PathPromptState::new(Purpose::Save, String::new());
        p.type_text(&format!("{}\\idea", dir.path().display()), &ctx(dir.path()));
        assert_eq!(p.hint(), notty_io::Hint::New);
        assert_eq!(p.ghost(), ".txt");
    }

    #[test]
    fn invalid_last_segment_is_detected() {
        let mut p = PathPromptState::new(Purpose::Save, String::new());
        p.type_text("a\\nota?.txt", &ctx(&PathBuf::from(".")));
        assert!(p.is_invalid());
    }
}
```

- [ ] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui path_prompt`
Expected: FAIL de compilación, `cannot find type PathPromptState`.

- [ ] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-ui/src/path_prompt.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Open,
    Save,
}

pub struct PathPromptState {
    pub value: String,
    pub purpose: Purpose,
    pub selected: usize,
}

impl PathPromptState {
    pub fn new(purpose: Purpose, initial: String) -> Self {
        Self { value: initial, purpose, selected: 0 }
    }

    pub fn type_text(&mut self, raw: &str, ctx: &notty_io::PathContext) {
        self.value = notty_io::normalize(raw, ctx);
        self.selected = 0;
    }

    pub fn suggestions(&self) -> Vec<notty_io::Entry> {
        notty_io::suggestions(&self.value, 5)
    }

    pub fn hint(&self) -> notty_io::Hint {
        notty_io::hint_for(&self.value)
    }

    fn last_segment(&self) -> &str {
        self.value.rsplit('\\').next().unwrap_or(&self.value)
    }

    pub fn is_invalid(&self) -> bool {
        notty_io::has_invalid_chars(self.last_segment())
    }

    pub fn ghost(&self) -> String {
        let sugs = self.suggestions();
        if !sugs.is_empty() {
            let idx = self.selected.min(sugs.len() - 1);
            let chosen = &sugs[idx];
            let rest = chosen.name.strip_prefix(self.last_segment()).unwrap_or("");
            return format!("{rest}{}", if chosen.is_dir { "\\" } else { "" });
        }
        let has_ext = self.last_segment().rsplit_once('.').is_some();
        match self.hint() {
            (notty_io::Hint::New | notty_io::Hint::DirNew) if !has_ext && !self.last_segment().is_empty() => ".txt".to_string(),
            _ => String::new(),
        }
    }

    pub fn move_selection(&mut self, delta: i32) {
        let n = self.suggestions().len();
        if n == 0 {
            return;
        }
        self.selected = ((self.selected as i64 + delta as i64).rem_euclid(n as i64)) as usize;
    }

    pub fn accept(&mut self) {
        let sugs = self.suggestions();
        if sugs.is_empty() {
            return;
        }
        let last = self.last_segment().to_string();
        if last == sugs[self.selected.min(sugs.len() - 1)].name && sugs.len() > 1 {
            self.selected = (self.selected + 1) % sugs.len();
        }
        let chosen = &sugs[self.selected.min(sugs.len() - 1)];
        let base = &self.value[..self.value.len() - last.len()];
        self.value = format!("{base}{}{}", chosen.name, if chosen.is_dir { "\\" } else { "" });
    }
}
```

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, 150 tests (143 anteriores + 7).

- [ ] **Step 5: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): estado de la línea de ruta (sugerencias, fantasma, Tab)"
```

---

### Task 5: notty-ui::search_prompt — estado de buscar/reemplazar (lógica pura sobre Document)

**Files:**
- Create: `crates/notty-ui/src/search_prompt.rs`
- Modify: `crates/notty-ui/src/lib.rs`

**Interfaces:**
- Consumes: `notty_core::{Document, SearchOptions, SearchError}`.
- Produces:
  - `pub struct SearchState { pub query: String, pub replacement: String, pub opts: notty_core::SearchOptions, pub current: usize }` (`Default`).
  - `set_query(&mut self, query: String)`: guarda `query` y resetea `current` a `0`.
  - `matches(&self, doc: &Document) -> Result<Vec<std::ops::Range<usize>>, notty_core::SearchError>`.
  - `count_label(&self, doc: &Document) -> String`: `""` si `query` vacío; `"regex ✕"` si `matches` da error; si no, `"{actual}/{total}"` con `actual = min(current+1, total)` (o `"0/0"` si no hay coincidencias).
  - `next(&mut self, doc: &Document)` / `prev(&mut self, doc: &Document)`: avanzan/retroceden `current` cíclicamente sobre `matches(doc)` (no hacen nada si el resultado es `Err` o está vacío).
  - `toggle_case(&mut self)` / `toggle_word(&mut self)` / `toggle_regex(&mut self)`: invierten el campo correspondiente de `opts` y resetean `current` a `0`.

- [ ] **Step 1: Escribir los tests que fallan**

`crates/notty-ui/src/search_prompt.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use notty_core::Document;

    fn doc() -> Document {
        Document::new("Juan y juan y JUAN", "\n")
    }

    #[test]
    fn empty_query_has_empty_label() {
        let s = SearchState::default();
        assert_eq!(s.count_label(&doc()), "");
    }

    #[test]
    fn default_search_is_case_insensitive() {
        let mut s = SearchState::default();
        s.set_query("juan".into());
        assert_eq!(s.matches(&doc()).unwrap().len(), 3);
        assert_eq!(s.count_label(&doc()), "1/3");
    }

    #[test]
    fn no_matches_shows_zero_of_zero() {
        let mut s = SearchState::default();
        s.set_query("nada-de-esto".into());
        assert_eq!(s.count_label(&doc()), "0/0");
    }

    #[test]
    fn bad_regex_shows_error_label() {
        let mut s = SearchState::default();
        s.opts.regex = true;
        s.set_query("(".into());
        assert_eq!(s.count_label(&doc()), "regex ✕");
    }

    #[test]
    fn next_and_prev_wrap_and_update_label() {
        let mut s = SearchState::default();
        s.set_query("juan".into());
        s.next(&doc());
        assert_eq!(s.count_label(&doc()), "2/3");
        s.next(&doc());
        s.next(&doc());
        assert_eq!(s.count_label(&doc()), "1/3");
        s.prev(&doc());
        assert_eq!(s.count_label(&doc()), "3/3");
    }

    #[test]
    fn toggle_case_narrows_matches_and_resets_current() {
        let mut s = SearchState::default();
        s.set_query("juan".into());
        s.next(&doc());
        s.toggle_case();
        assert_eq!(s.matches(&doc()).unwrap().len(), 1);
        assert_eq!(s.current, 0);
    }

    #[test]
    fn toggle_word_and_regex_flip_flags() {
        let mut s = SearchState::default();
        assert!(!s.opts.whole_word);
        s.toggle_word();
        assert!(s.opts.whole_word);
        assert!(!s.opts.regex);
        s.toggle_regex();
        assert!(s.opts.regex);
    }
}
```

- [ ] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui search_prompt`
Expected: FAIL de compilación, `cannot find type SearchState`.

- [ ] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-ui/src/search_prompt.rs`:

```rust
use std::ops::Range;

use notty_core::{Document, SearchError, SearchOptions};

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub replacement: String,
    pub opts: SearchOptions,
    pub current: usize,
}

impl SearchState {
    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.current = 0;
    }

    pub fn matches(&self, doc: &Document) -> Result<Vec<Range<usize>>, SearchError> {
        if self.query.is_empty() {
            return Ok(Vec::new());
        }
        doc.find_all(&self.query, self.opts)
    }

    pub fn count_label(&self, doc: &Document) -> String {
        if self.query.is_empty() {
            return String::new();
        }
        match self.matches(doc) {
            Err(_) => "regex ✕".to_string(),
            Ok(m) if m.is_empty() => "0/0".to_string(),
            Ok(m) => format!("{}/{}", (self.current + 1).min(m.len()), m.len()),
        }
    }

    pub fn next(&mut self, doc: &Document) {
        if let Ok(m) = self.matches(doc) {
            if !m.is_empty() {
                self.current = (self.current + 1) % m.len();
            }
        }
    }

    pub fn prev(&mut self, doc: &Document) {
        if let Ok(m) = self.matches(doc) {
            if !m.is_empty() {
                self.current = (self.current + m.len() - 1) % m.len();
            }
        }
    }

    pub fn toggle_case(&mut self) {
        self.opts.case_sensitive = !self.opts.case_sensitive;
        self.current = 0;
    }

    pub fn toggle_word(&mut self) {
        self.opts.whole_word = !self.opts.whole_word;
        self.current = 0;
    }

    pub fn toggle_regex(&mut self) {
        self.opts.regex = !self.opts.regex;
        self.current = 0;
    }
}
```

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, 157 tests (150 anteriores + 7).

- [ ] **Step 5: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): estado de buscar/reemplazar sobre Document"
```

---

### Task 6: Prompt unificado en Workspace (lógica pura)

**Files:**
- Create: `crates/notty-ui/src/prompt.rs`
- Modify: `crates/notty-ui/src/workspace.rs`
- Modify: `crates/notty-ui/src/lib.rs`

**Interfaces:**
- Consumes: `PathPromptState`, `SearchState`.
- Produces: `pub enum Prompt { None, Path(PathPromptState), Find(SearchState), Replace(SearchState) }` (`Default` = `None`). `Workspace` gana el campo público `pub prompt: Prompt` (inicializado a `Prompt::None` en `new()`) y el método `close_prompt(&mut self)` (`self.prompt = Prompt::None`).

- [ ] **Step 1: Escribir los tests que fallan**

Añadir a `mod tests` en `crates/notty-ui/src/workspace.rs`:

```rust
    #[test]
    fn starts_without_a_prompt() {
        assert!(matches!(Workspace::new().prompt, Prompt::None));
    }

    #[test]
    fn close_prompt_clears_it() {
        let mut w = Workspace::new();
        w.prompt = Prompt::Find(crate::search_prompt::SearchState::default());
        w.close_prompt();
        assert!(matches!(w.prompt, Prompt::None));
    }
```

- [ ] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui workspace`
Expected: FAIL de compilación, `no field prompt on type Workspace`.

- [ ] **Step 3: Implementar**

`crates/notty-ui/src/prompt.rs` (archivo completo, sin tests propios: es una enumeración sin lógica, ya cubierta por los tests de `PathPromptState`/`SearchState` y de `Workspace`):

```rust
use crate::path_prompt::PathPromptState;
use crate::search_prompt::SearchState;

#[derive(Default)]
pub enum Prompt {
    #[default]
    None,
    Path(PathPromptState),
    Find(SearchState),
    Replace(SearchState),
}
```

En `crates/notty-ui/src/workspace.rs`, añadir el campo y actualizar `new`:

```rust
pub struct Workspace {
    docs: Vec<EditorState>,
    active: usize,
    pub prompt: crate::Prompt,
}
```

```rust
impl Workspace {
    pub fn new() -> Self {
        Self { docs: vec![EditorState::new_empty()], active: 0, prompt: crate::Prompt::None }
    }
    // ... el resto de métodos ya existentes no cambian ...

    pub fn close_prompt(&mut self) {
        self.prompt = crate::Prompt::None;
    }
}
```

`crates/notty-ui/src/lib.rs` queda:

```rust
//! notty-ui: ventana Win32, render Direct2D/DirectWrite y lógica de edición en pantalla.

pub mod clipboard;
mod doc_io;
pub mod editor;
mod gutter;
mod hints;
mod keymap;
mod path_prompt;
mod prompt;
pub mod render;
mod search_prompt;
pub mod settings_window;
mod status;
mod viewport;
pub mod window;
pub mod workspace;

pub use doc_io::{OpenedDoc, open_as_document, save_document};
pub use editor::EditorState;
pub use gutter::gutter_width;
pub use hints::hints_text;
pub use keymap::{EditorAction, Modifiers, action_for_vk};
pub use path_prompt::{Purpose, PathPromptState};
pub use prompt::Prompt;
pub use render::Renderer;
pub use search_prompt::SearchState;
pub use status::status_line;
pub use viewport::Viewport;
pub use workspace::Workspace;
```

(añade los `mod`/`pub use` nuevos a los que ya había del Plan 3; no borres el resto.)

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, 159 tests (157 anteriores + 2).

- [ ] **Step 5: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): Prompt unificado (ruta/buscar/reemplazar) en Workspace"
```

---

### Task 7: window.rs — conectar teclado a los prompts

**Files:**
- Modify: `crates/notty-ui/src/window.rs`

**Interfaces:**
- Consumes: `EditorAction::{OpenPathPrompt, Find, Replace, FindNext, FindPrev, Save}`, `Prompt`, `PathPromptState`, `SearchState`.
- Produces: el bucle de mensajes de `window.rs` reconoce cuándo hay un prompt activo (`!matches!(ws.prompt, Prompt::None)`) y, en ese caso, **todas** las teclas van al prompt en vez de al editor (igual que en la maqueta: mientras escribes una ruta o una búsqueda, las flechas y las letras no mueven el cursor del texto).

- [ ] **Step 1: Abrir/cerrar prompts desde `EditorAction`**

En el `match action` de `WM_KEYDOWN` (el que ya trata `Copy/Cut/Paste/Save` desde el Plan 2), añadir, **antes** de mirar si hay un prompt abierto:

```rust
match action {
    crate::EditorAction::OpenPathPrompt => {
        let initial = ws.active().path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        ws.prompt = crate::Prompt::Path(crate::PathPromptState::new(crate::Purpose::Open, initial));
    }
    crate::EditorAction::Find => ws.prompt = crate::Prompt::Find(crate::SearchState::default()),
    crate::EditorAction::Replace => ws.prompt = crate::Prompt::Replace(crate::SearchState::default()),
    crate::EditorAction::Save if ws.active().path.is_none() => {
        ws.prompt = crate::Prompt::Path(crate::PathPromptState::new(crate::Purpose::Save, String::new()));
    }
    crate::EditorAction::FindNext | crate::EditorAction::FindPrev => {
        // Fuera de un prompt de búsqueda activo, F3 repite la última búsqueda si la hay;
        // como `SearchState` vive dentro de `ws.prompt`, si no hay una búsqueda abierta
        // no hace nada (se abrirá una con Ctrl+F si se quiere buscar).
        if let crate::Prompt::Find(s) | crate::Prompt::Replace(s) = &mut ws.prompt {
            if matches!(action, crate::EditorAction::FindNext) { s.next(&ws.active().doc) } else { s.prev(&ws.active().doc) }
        }
    }
    other => { /* el resto sigue como hasta ahora (ver Step 2) */ }
}
```

(La rama `Save if ws.active().path.is_none()` debe comprobarse **antes** de que el `match` llegue al brazo genérico de `Save`, que sigue llamando a `ws.active_mut().save()` cuando sí hay ruta — reorganiza el `match` existente para que esta guarda tenga prioridad, sin duplicar el resto de brazos ya escritos en los Planes 2 y 3.)

- [ ] **Step 2: Redirigir el teclado al prompt cuando hay uno abierto**

Justo al principio del manejador de `WM_KEYDOWN` y de `WM_CHAR` (antes de construir `action`/`ch` para el editor), comprobar si `ws.prompt` no es `None` y, en ese caso, despachar a una función nueva en vez de al camino normal:

```rust
fn handle_prompt_keydown(ws: &mut crate::Workspace, vk: u32, mods: notty_ui_modifiers, hwnd: HWND) {
    // Esc cierra cualquier prompt.
    if vk == 0x1B {
        ws.close_prompt();
        return;
    }
    match &mut ws.prompt {
        crate::Prompt::Path(p) => handle_path_key(p, vk, mods, ws /* ver Step 3 */),
        crate::Prompt::Find(s) | crate::Prompt::Replace(s) => handle_search_key(s, vk, mods),
        crate::Prompt::None => {}
    }
}
```

(Ajusta la firma exacta a como esté organizado el estado por ventana en tu `window.rs`: si `EditorState`/`Workspace` ya viven dentro de una `struct` capturada por el `WndProc`, añade estas funciones como métodos de esa `struct` en vez de funciones sueltas — lo importante es el comportamiento, no la forma exacta de pasar el contexto.)

- [ ] **Step 3: Teclas de la línea de ruta**

```rust
fn handle_path_key(p: &mut crate::PathPromptState, vk: u32, mods: Modifiers, ctx: &notty_io::PathContext) -> Option<PathAction> {
    match vk {
        0x09 => { p.accept(); None } // Tab
        0x26 => { p.move_selection(-1); None } // ArrowUp
        0x28 => { p.move_selection(1); None } // ArrowDown
        0x0D => Some(PathAction::Commit), // Enter
        0x4F if mods.ctrl => Some(PathAction::OpenWindowsDialog), // Ctrl+O dentro de la línea
        _ => None,
    }
}
```

`WM_CHAR` cuando hay un `Prompt::Path` abierto: en vez de insertar el carácter en el documento, añadirlo a `p.value` (o, más simple y fiel a la maqueta, mantener un `String` de "lo tecleado hasta ahora" y llamar a `p.type_text(&typed, &ctx)` en cada pulsación, dejando que `normalize` limpie el resultado — esto evita tener que reimplementar borrado de caracteres a mano; usa el propio texto del control de edición si `window.rs` ya tiene un campo de texto nativo, o un `String` en el estado del prompt si no lo tiene).

`PathAction::Commit` (Enter): si `p.is_invalid()`, no hacer nada (queda marcado en rojo al pintar, Task 8); si no:
- si `p.value` termina en `\`, es una carpeta que se acaba de aceptar: no cerrar el prompt, seguir escribiendo dentro.
- si el hint es `Dir`: si `purpose == Open`, no tiene sentido "abrir" una carpeta — no hacer nada (o, más simple, entrar en ella igual que con `Tab`).
- si el hint es `Exists` (archivo): `purpose == Open` → `notty_ui::open_as_document(Path::new(&p.value))` y `ws.open(EditorState::from_opened(...))`, cerrar el prompt; `purpose == Save` → tratarlo igual que `New` (sobrescribir es válido).
- si el hint es `New`/`DirNew`: crear las carpetas que falten con `notty_io::create_parent_dirs` y, según `purpose`, o bien abrir un documento nuevo vacío en esa ruta (`Open`, caso raro: el usuario pidió abrir algo que no existe — trátalo como crear un documento nuevo con esa ruta) o bien asignar `ws.active_mut().path = Some(PathBuf::from(&p.value))` y llamar a `ws.active_mut().save()` (`Save`).
- en cualquier caso que termine con éxito: `ws.close_prompt()`.

`PathAction::OpenWindowsDialog`: por ahora, deja un comentario `// TODO(plan futuro): diálogo nativo de Windows` y no hagas nada más (no es parte del alcance de este plan; la línea de ruta ya cubre el caso principal).

- [ ] **Step 4: Teclas de buscar/reemplazar**

```rust
fn handle_search_key(s: &mut crate::SearchState, vk: u32, mods: Modifiers, doc: &notty_core::Document) -> Option<SearchAction> {
    if mods.alt {
        match vk {
            0x43 => { s.toggle_case(); return None; } // Alt+C
            0x57 => { s.toggle_word(); return None; } // Alt+W
            0x52 => { s.toggle_regex(); return None; } // Alt+R
            _ => {}
        }
    }
    match vk {
        0x0D if mods.shift => { s.prev(doc); None } // Shift+Enter
        0x0D => { s.next(doc); None } // Enter
        _ => None,
    }
}
```

`WM_CHAR` con un `Prompt::Find`/`Replace` abierto: añade el carácter a `s.query` (o a `s.replacement` si el foco lógico está en el segundo campo del prompt de reemplazo — para esta primera versión, basta con un único campo activo a la vez; si quieres distinguir "estoy escribiendo la búsqueda" de "estoy escribiendo el reemplazo", usa una bandera `editing_replacement: bool` dentro del propio `Prompt::Replace` o cambia su variante a `Replace(SearchState, bool)`) y llama a `s.set_query(...)`.

En `Prompt::Replace`, `Enter` sin `Ctrl+Alt` reemplaza la coincidencia actual: usa `doc.replace_all` no es lo que hace falta aquí (reemplaza *todas*); para "reemplazar solo esta" haz `let m = s.matches(doc)?; let r = m[s.current].clone(); ws.active_mut().doc.replace_range(r, &s.replacement, Instant::now());` y luego `s.next(&ws.active().doc)`. `Ctrl+Alt+Enter` reemplaza todas: `ws.active_mut().doc.replace_all(&s.query, &s.replacement, s.opts, Instant::now())`.

- [ ] **Step 5: Compilar**

Run: `cargo build --workspace`
Expected: compila (con los ajustes de organización de `window.rs` que hagan falta para que todo lo anterior encaje con cómo esté escrito el estado por ventana tras los Planes 2 y 3).

- [ ] **Step 6: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): conecta el teclado a los prompts de ruta y búsqueda"
```

---

### Task 8: Render: dibujar la línea de prompt, sugerencias y resaltado de búsqueda

**Files:**
- Modify: `crates/notty-ui/src/render.rs`

**Interfaces:**
- Consumes: `Workspace::prompt`, `PathPromptState`, `SearchState`.
- Produces: cuando `ws.prompt` no es `None`, la franja de la barra de estado se sustituye por la línea del prompt (igual que en la maqueta: nunca coexisten estado normal y prompt). Además:
  - **Ruta:** el texto tecleado + el `ghost()` en gris a continuación, en monoespaciado; si `is_invalid()`, el texto en rojo (`--danger` del tema, aproximado a `D2D1_COLOR_F { r:0.85,g:0.35,b:0.35,a:1.0 }` en modo oscuro); si hay sugerencias, una caja pequeña justo encima de la barra de estado con hasta 5 filas (la fila `selected` resaltada con `sel_brush`), carpetas con `\` final, archivos sin marca.
  - **Buscar/reemplazar:** `"buscar: " + query` (y, si es `Replace`, una segunda línea o segmento `"por: " + replacement`), el contador de `count_label`, y tres indicadores `Aa` / `ab` / `.*` resaltados según `opts.case_sensitive`/`whole_word`/`regex`. Las coincidencias de `matches(doc)` visibles en pantalla se resaltan con un `FillRectangle` semitransparente por cada una que caiga en una línea visible (reutiliza el mismo mecanismo que ya pinta el fondo de la selección desde el Plan 2), con la actual (`current`) en un tono más marcado.

- [ ] **Step 1: Implementar**

Modifica `Renderer::paint` (la firma ya recibe `ws: &Workspace` desde el Plan 3) para, tras dibujar el texto y antes/junto con la barra de estado:

1. Calcular `let prompt_active = !matches!(ws.prompt, crate::Prompt::None);` y, si es `true`, reservar la franja inferior habitual para el prompt en vez de para el contenido normal de la barra de estado.
2. Con un `match &ws.prompt { ... }`, dibujar según el caso descrito arriba. Usa `IDWriteTextLayout` de una sola línea por cada pieza de texto (igual que ya se hace para las líneas del documento), y `FillRectangle`/`DrawRectangle` para la caja de sugerencias y los resaltados.
3. Para el resaltado de coincidencias de búsqueda: por cada línea visible (mismo bucle que ya recorre `state.viewport.range(...)`), intersecar su rango de chars `[buf.line_start(line), fin_de_línea)` con cada rango de `matches`, y si hay solape, calcular su X de inicio/fin con `HitTestTextPosition` (igual que ya hace `Renderer::char_index_at` del Plan 2) y pintar un rectángulo semitransparente antes de dibujar el texto de esa línea (para que el texto quede encima, legible).

Ajusta cualquier firma de `windows` que haga falta al compilar.

- [ ] **Step 2: Compilar y comprobar manualmente**

Run: `cargo build --workspace`
Expected: compila.

Run: `cargo run --bin notty -- /tmp/notas.txt` (u otro archivo de prueba con varias líneas y alguna palabra repetida).
Expected (manual):
- `Ctrl+O` activa la línea de ruta; escribir una ruta real de tu disco muestra hasta 5 sugerencias, `Tab` las acepta, `↑`/`↓` cambian la resaltada, `Enter` sobre un archivo existente lo abre.
- Con el documento activo sin ruta (pestaña nueva con `Ctrl+N`), `Ctrl+S` abre la línea de ruta en modo guardar; escribir una ruta que no existe y pulsar `Enter` crea las carpetas que falten y guarda.
- `Ctrl+F` resalta en vivo todas las coincidencias de lo que escribas, con el contador a la derecha; `Enter`/`Shift+Enter` saltan siguiente/anterior; `Alt+C`, `Alt+W`, `Alt+R` cambian el resultado.
- `Ctrl+H` añade el campo de reemplazo; `Enter` reemplaza una, `Ctrl+Alt+Enter` todas.
- `Esc` cierra cualquier prompt y devuelve el foco al texto.

- [ ] **Step 3: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): dibuja la línea de prompt, sugerencias y resaltado de búsqueda"
```

---

### Task 9: Comprobación final del workspace

**Files:** ninguno nuevo; solo verificación.

- [ ] **Step 1: Tests y lints de todo el workspace**

Run: `cargo test --workspace`
Expected: PASS, 159 tests (las Tasks 7-8 son integración Win32 sin tests automáticos nuevos).

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: sin avisos. Corregir cualquier aviso en el archivo que lo señale y repetir hasta que quede limpio.

- [ ] **Step 2: Build release**

Run: `cargo build --release --workspace`
Expected: compila sin errores.

- [ ] **Step 3: Commit (si hubo cambios de la revisión)**

```bash
git add -A
git commit -m "chore(ui): pasa clippy y build release tras ruta y búsqueda" --allow-empty
```

---

## Hoja de ruta: planes siguientes

| Plan | Contenido |
|---|---|
| **5 · Vim y raw** | Keymap vim, vista hexadecimal sobre `RawBytes`, resto de la sección "Teclado" de Ajustes. |
| **6 · Archivos vivos** | Temporales (`CLICKME`, borrador/volátil), instancia única, daemon/atajo global, autoguardado, conflictos, recuperación tras caída, resto de "Archivos" y "Atajo global" de Ajustes. |
| **7 · Pulido y garantías** | UI Automation, alto contraste/DPI, fuzzing, presupuesto de rendimiento en CI. |
