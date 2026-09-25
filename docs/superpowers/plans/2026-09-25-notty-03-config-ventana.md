# notty · Plan 3: Configuración, comandos y piezas de ventana Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `config.toml` con presets (Moderna/Clásica/Zen), varios documentos abiertos a la vez (pestañas o buffers, según configuración), barra de menús, barra de atajos y números de línea configurables, y una ventana de Ajustes mínima que lee y escribe ese mismo archivo.

**Architecture:** Dos crates nuevos. `notty-config` es puro (sin Win32): el `struct Config` con `serde`/`toml`, los presets como función pura sobre `Config`, y guardado atómico vía `notty_io::atomic_write`. `notty-input` también es puro: comandos de interfaz (`UiCommand`) con nombre, un parser de combinaciones de teclas en texto (`"Ctrl+Shift+H"`) y un mapa de teclas por defecto que `config.toml` puede sobrescribir. `notty-ui` gana un `Workspace` (varios `EditorState`, uno activo) y las piezas de ventana que faltaban: pestañas/buffers, menú, barra de atajos, números de línea y una ventana de Ajustes secundaria con controles Win32 nativos.

**Tech Stack:** Rust stable 1.96 (MSVC), `serde` + `toml` (config), `windows` (ya en el workspace desde el Plan 2, más controles Win32 nativos para Ajustes), `notty-core`/`notty-io`/`notty-ui` (Planes 1 y 2).

**Spec:** `docs/superpowers/specs/2026-09-24-notty-design.md`
**Planes anteriores:** `docs/superpowers/plans/2026-09-24-notty-01-nucleo.md`, `docs/superpowers/plans/2026-09-25-notty-02-ventana-editor.md` (implementados; 95 tests en verde, último commit `ec80a2e`)

## Global Constraints

- Solo Windows 10/11; toolchain `stable-x86_64-pc-windows-msvc`.
- `notty-config` y `notty-input` no dependen de `windows-rs`: son lógica pura, testable sin ventana.
- `config.toml` es la única fuente de verdad: la ventana de Ajustes lee y escribe ese mismo archivo, nunca guarda su propio estado en otro sitio.
- Un `config.toml` roto (TOML inválido) hace que notty arranque con la configuración por defecto y **nunca sobrescribe** el archivo del usuario hasta que él lo corrija o lo guarde explícitamente desde Ajustes.
- Textos visibles para el usuario en español.
- Un commit por tarea terminada.
- **Alcance de la ventana de Ajustes en este plan:** implementa los controles de la sección "Apariencia" (preset, tema, números de línea) y "Ventana → Varios archivos" (pestañas/buffers) de la maqueta (`docs/mockups/notty-ui.html`), con controles Win32 nativos reales. El resto de secciones de la maqueta (Teclado, Archivos, Atajo global) quedan para un plan de pulido posterior una vez estén implementadas las funciones que configuran (vim, temporales, atajo global son de los Planes 5 y 6); no las construyas todavía, ni dejes botones o pestañas vacíos para ellas.
- **Sobre el código Win32 de este plan** (ventana de Ajustes, controles nativos, menú, hit-testing de pestañas): se da la arquitectura y las llamadas por su nombre. Compílalo, y si una firma no coincide con la versión de `windows` instalada, ajústala consultando `cargo doc -p windows --open` o el error del compilador, sin cambiar el comportamiento descrito. Los pasos marcados como "lógica pura" son código exacto.

## File Structure

```
crates/notty-config/Cargo.toml
crates/notty-config/src/lib.rs       re-exports
crates/notty-config/src/model.rs     Config, UiConfig, Preset (serde)
crates/notty-config/src/preset.rs    apply_preset (lógica pura)
crates/notty-config/src/storage.rs   default_path, load, save (usa notty_io::atomic_write)
crates/notty-input/Cargo.toml
crates/notty-input/src/lib.rs        re-exports
crates/notty-input/src/keyspec.rs    Modifiers, parse_key_spec (lógica pura)
crates/notty-input/src/command.rs    UiCommand, default_ui_keymap, apply_overrides
crates/notty-ui/src/workspace.rs     Workspace: varios EditorState (nuevo)
crates/notty-ui/src/gutter.rs        gutter_width (lógica pura, nuevo)
crates/notty-ui/src/hints.rs         hints_text (lógica pura, nuevo)
crates/notty-ui/src/render.rs        (modificado) tab bar, menú, barra de atajos, gutter
crates/notty-ui/src/window.rs        (modificado) Workspace, config, Ajustes, Alt-menú, clic en pestañas
crates/notty-ui/src/settings_window.rs  ventana de Ajustes (nuevo)
crates/notty/src/main.rs             (modificado) carga config.toml antes de crear la ventana
```

---

### Task 1: notty-config: modelo y presets (lógica pura)

**Files:**
- Create: `crates/notty-config/Cargo.toml`
- Create: `crates/notty-config/src/lib.rs`
- Create: `crates/notty-config/src/model.rs`
- Create: `crates/notty-config/src/preset.rs`
- Modify: `Cargo.toml` (workspace, añadir `serde`/`toml` a `[workspace.dependencies]`)

**Interfaces:**
- Consumes: nada.
- Produces:
  - `pub enum Preset { Moderna, Clasica, Zen }` (serde `rename_all = "lowercase"`, `Default` = `Moderna`).
  - `pub enum TabsPosition { Title, Below, Auto, Hidden }`, `pub enum MenuBar { Hidden, Visible, Alt }`, `pub enum Files { Tabs, Buffers }`, `pub enum Theme { System, Light, Dark }` (todas serde `rename_all = "lowercase"`, con `Default` razonable: `TabsPosition::Title`, `MenuBar::Hidden`, `Files::Tabs`, `Theme::System`).
  - `pub struct UiConfig { pub preset: Preset, pub theme: Theme, pub line_numbers: bool, pub files: Files, pub tabs_position: TabsPosition, pub menubar: MenuBar, pub hints_bar: bool, pub status_bar: bool }` con `Default`.
  - `pub struct Config { pub ui: UiConfig }` con `Default` (serde `Deserialize`/`Serialize`, todos los campos con `#[serde(default)]` para que un `config.toml` parcial no falle).
  - `pub fn apply_preset(ui: &mut UiConfig, preset: Preset)`: fija `ui.preset` y las piezas de cada preset según la spec (Moderna: `tabs_position=Title, menubar=Hidden, hints_bar=true, status_bar=true`; Clásica: `tabs_position=Below, menubar=Visible, hints_bar=false, status_bar=true`; Zen: `tabs_position=Auto, menubar=Hidden, hints_bar=false, status_bar=true`). No toca `theme`, `line_numbers` ni `files` (son independientes del preset).

- [x] **Step 1: Crear el crate y añadir dependencias al workspace**

`Cargo.toml` (raíz), añadir a `[workspace.dependencies]`:

```toml
serde = { version = "1", features = ["derive"] }
toml = "0.8"
```

`crates/notty-config/Cargo.toml`:

```toml
[package]
name = "notty-config"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
notty-io = { path = "../notty-io" }
serde.workspace = true
toml.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

`crates/notty-config/src/lib.rs`:

```rust
//! notty-config: config.toml, presets y guardado. Sin nada de Windows.

mod model;
mod preset;
mod storage;

pub use model::{Config, Files, MenuBar, Preset, TabsPosition, Theme, UiConfig};
pub use preset::apply_preset;
pub use storage::{LoadResult, default_path, load, save};
```

(`storage` se implementa en la Task 3; declararlo ya evita tener que volver a tocar `lib.rs`.)

- [x] **Step 2: Escribir los tests que fallan**

`crates/notty-config/src/model.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_moderna() {
        let ui = UiConfig::default();
        assert_eq!(ui.preset, Preset::Moderna);
        assert_eq!(ui.tabs_position, TabsPosition::Title);
        assert_eq!(ui.menubar, MenuBar::Hidden);
        assert!(ui.hints_bar);
        assert!(ui.status_bar);
        assert!(ui.line_numbers == true || ui.line_numbers == false); // solo comprueba que existe el campo
    }

    #[test]
    fn serializes_and_parses_back() {
        let cfg = Config::default();
        let text = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.ui.preset, cfg.ui.preset);
    }

    #[test]
    fn parses_partial_toml_with_defaults() {
        let cfg: Config = toml::from_str("[ui]\npreset = \"zen\"\n").unwrap();
        assert_eq!(cfg.ui.preset, Preset::Zen);
        assert_eq!(cfg.ui.theme, Theme::System);
    }

    #[test]
    fn empty_toml_is_full_defaults() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn invalid_preset_value_is_a_parse_error() {
        assert!(toml::from_str::<Config>("[ui]\npreset = \"no-existe\"\n").is_err());
    }
}
```

`crates/notty-config/src/preset.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MenuBar, TabsPosition, UiConfig};

    #[test]
    fn moderna_has_tabs_in_title_and_no_menubar() {
        let mut ui = UiConfig::default();
        apply_preset(&mut ui, Preset::Moderna);
        assert_eq!(ui.tabs_position, TabsPosition::Title);
        assert_eq!(ui.menubar, MenuBar::Hidden);
        assert!(ui.hints_bar);
    }

    #[test]
    fn clasica_shows_menubar_and_tabs_below() {
        let mut ui = UiConfig::default();
        apply_preset(&mut ui, Preset::Clasica);
        assert_eq!(ui.tabs_position, TabsPosition::Below);
        assert_eq!(ui.menubar, MenuBar::Visible);
        assert!(!ui.hints_bar);
    }

    #[test]
    fn zen_hides_hints_and_shows_tabs_only_if_several() {
        let mut ui = UiConfig::default();
        apply_preset(&mut ui, Preset::Zen);
        assert_eq!(ui.tabs_position, TabsPosition::Auto);
        assert_eq!(ui.menubar, MenuBar::Hidden);
        assert!(!ui.hints_bar);
    }

    #[test]
    fn applying_a_preset_does_not_touch_theme_or_line_numbers() {
        let mut ui = UiConfig { theme: crate::Theme::Dark, line_numbers: false, ..UiConfig::default() };
        apply_preset(&mut ui, Preset::Clasica);
        assert_eq!(ui.theme, crate::Theme::Dark);
        assert!(!ui.line_numbers);
    }

    #[test]
    fn preset_field_itself_is_updated() {
        let mut ui = UiConfig::default();
        apply_preset(&mut ui, Preset::Zen);
        assert_eq!(ui.preset, Preset::Zen);
    }
}
```

- [x] **Step 3: Ejecutar y ver que falla**

Run: `cargo test -p notty-config`
Expected: FAIL de compilación, `cannot find type UiConfig` (el crate aún no tiene contenido antes de los tests).

- [x] **Step 4: Implementar el modelo**

Añadir **encima** del módulo de tests en `crates/notty-config/src/model.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    #[default]
    Moderna,
    Clasica,
    Zen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Files {
    #[default]
    Tabs,
    Buffers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabsPosition {
    #[default]
    Title,
    Below,
    Auto,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MenuBar {
    #[default]
    Hidden,
    Visible,
    Alt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub preset: Preset,
    pub theme: Theme,
    pub line_numbers: bool,
    pub files: Files,
    pub tabs_position: TabsPosition,
    pub menubar: MenuBar,
    pub hints_bar: bool,
    pub status_bar: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        let mut ui = Self {
            preset: Preset::default(),
            theme: Theme::default(),
            line_numbers: true,
            files: Files::default(),
            tabs_position: TabsPosition::default(),
            menubar: MenuBar::default(),
            hints_bar: true,
            status_bar: true,
        };
        crate::apply_preset(&mut ui, ui.preset);
        ui
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ui: UiConfig,
}
```

`crates/notty-config/src/preset.rs`, añadir **encima** del módulo de tests:

```rust
use crate::{MenuBar, Preset, TabsPosition, UiConfig};

/// Aplica las piezas fijas de un preset. No toca `theme`, `line_numbers` ni `files`:
/// son independientes del preset, tal como describe la spec.
pub fn apply_preset(ui: &mut UiConfig, preset: Preset) {
    ui.preset = preset;
    match preset {
        Preset::Moderna => {
            ui.tabs_position = TabsPosition::Title;
            ui.menubar = MenuBar::Hidden;
            ui.hints_bar = true;
            ui.status_bar = true;
        }
        Preset::Clasica => {
            ui.tabs_position = TabsPosition::Below;
            ui.menubar = MenuBar::Visible;
            ui.hints_bar = false;
            ui.status_bar = true;
        }
        Preset::Zen => {
            ui.tabs_position = TabsPosition::Auto;
            ui.menubar = MenuBar::Hidden;
            ui.hints_bar = false;
            ui.status_bar = true;
        }
    }
}
```

- [x] **Step 5: Ejecutar y ver que pasa**

Run: `cargo test -p notty-config`
Expected: FAIL todavía, porque `lib.rs` declara `mod storage;` y ese archivo no existe. Crear un `crates/notty-config/src/storage.rs` vacío temporal:

```rust
// Se implementa en la Task 3.
```

Volver a ejecutar: `cargo test -p notty-config`
Expected: PASS, 10 tests.

- [x] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/notty-config
git commit -m "feat(config): modelo de Config/UiConfig y presets"
```

---

### Task 2: notty-input: parser de teclas y comandos de interfaz (lógica pura)

**Files:**
- Create: `crates/notty-input/Cargo.toml`
- Create: `crates/notty-input/src/lib.rs`
- Create: `crates/notty-input/src/keyspec.rs`
- Create: `crates/notty-input/src/command.rs`

**Interfaces:**
- Consumes: `notty_config::Config` (solo para `apply_overrides`).
- Produces:
  - `pub struct Modifiers { pub ctrl: bool, pub shift: bool, pub alt: bool }` (mismo significado que `notty_ui::Modifiers`; se duplica a propósito para que `notty-input` no dependa de `notty-ui`, ver nota más abajo).
  - `pub fn parse_key_spec(s: &str) -> Option<(u32, Modifiers)>`: interpreta cadenas como `"Ctrl+S"`, `"Ctrl+Shift+H"`, `"F3"`, `"Ctrl+,"`, separadas por `+`, sin distinguir mayúsculas en los nombres de modificador (`Ctrl`/`Alt`/`Shift`) ni en las letras. Usa las mismas constantes `VK_*` numéricas que `notty_ui::keymap` (letras A-Z = su código ASCII mayúscula, F1-F12 = `0x70..=0x7B`, `,` = `0xBC`, `.` = `0xBE`).
  - `pub enum UiCommand { OpenSettings, NewTab, NextTab, PrevTab, CloseTab }`.
  - `pub fn default_ui_keymap() -> std::collections::HashMap<(u32, Modifiers), UiCommand>`: `Ctrl+,` → `OpenSettings`, `Ctrl+N` → `NewTab`, `Ctrl+Tab` → `NextTab`, `Ctrl+Shift+Tab` (Tab con Ctrl+Shift) → `PrevTab`, `Ctrl+W` → `CloseTab`.
  - `pub fn apply_overrides(map: &mut HashMap<(u32, Modifiers), UiCommand>, cfg: &notty_config::Config)`: por ahora, como `Config` de la Task 1 no tiene todavía una tabla `[keys]` (se añade en un plan posterior si hace falta remapear), esta función no hace nada salvo devolver el mapa igual; existe para que `window.rs` ya llame al punto de extensión correcto y no haga falta tocarlo cuando se añadan overrides de verdad.

Nota de diseño: `Modifiers` se duplica entre `notty-ui` y `notty-input` a propósito (tres booleanos, coste de mantenimiento mínimo) para que ninguno de los dos dependa del otro; es `notty-ui::window` quien construye ambos tipos desde el mismo `WM_KEYDOWN` y los usa cada uno con su propio mapa.

- [x] **Step 1: Crear el crate**

`crates/notty-input/Cargo.toml`:

```toml
[package]
name = "notty-input"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
notty-config = { path = "../notty-config" }
```

`crates/notty-input/src/lib.rs`:

```rust
//! notty-input: comandos de interfaz y su mapa de teclas. Sin nada de Windows.

mod command;
mod keyspec;

pub use command::{UiCommand, apply_overrides, default_ui_keymap};
pub use keyspec::{Modifiers, parse_key_spec};
```

- [x] **Step 2: Escribir los tests que fallan**

`crates/notty-input/src/keyspec.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn m(ctrl: bool, shift: bool, alt: bool) -> Modifiers {
        Modifiers { ctrl, shift, alt }
    }

    #[test]
    fn parses_single_letter_with_ctrl() {
        assert_eq!(parse_key_spec("Ctrl+S"), Some((0x53, m(true, false, false))));
    }

    #[test]
    fn parses_three_modifiers() {
        assert_eq!(parse_key_spec("Ctrl+Alt+V"), Some((0x56, m(true, false, true))));
    }

    #[test]
    fn is_case_insensitive() {
        assert_eq!(parse_key_spec("ctrl+shift+h"), parse_key_spec("CTRL+SHIFT+H"));
    }

    #[test]
    fn parses_function_keys() {
        assert_eq!(parse_key_spec("F3"), Some((0x72, m(false, false, false))));
        assert_eq!(parse_key_spec("Shift+F3"), Some((0x72, m(false, true, false))));
    }

    #[test]
    fn parses_comma_key() {
        assert_eq!(parse_key_spec("Ctrl+,"), Some((0xBC, m(true, false, false))));
    }

    #[test]
    fn parses_tab_key() {
        assert_eq!(parse_key_spec("Ctrl+Tab"), Some((0x09, m(true, false, false))));
    }

    #[test]
    fn unknown_key_name_is_none() {
        assert_eq!(parse_key_spec("Ctrl+Nope"), None);
    }

    #[test]
    fn empty_string_is_none() {
        assert_eq!(parse_key_spec(""), None);
    }
}
```

`crates/notty-input/src/command.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Modifiers;

    fn m(ctrl: bool, shift: bool, alt: bool) -> Modifiers {
        Modifiers { ctrl, shift, alt }
    }

    #[test]
    fn default_keymap_has_the_five_commands() {
        let map = default_ui_keymap();
        assert_eq!(map.get(&(0xBC, m(true, false, false))), Some(&UiCommand::OpenSettings));
        assert_eq!(map.get(&(0x4E, m(true, false, false))), Some(&UiCommand::NewTab));
        assert_eq!(map.get(&(0x09, m(true, false, false))), Some(&UiCommand::NextTab));
        assert_eq!(map.get(&(0x09, m(true, true, false))), Some(&UiCommand::PrevTab));
        assert_eq!(map.get(&(0x57, m(true, false, false))), Some(&UiCommand::CloseTab));
    }

    #[test]
    fn apply_overrides_keeps_defaults_when_config_has_none() {
        let mut map = default_ui_keymap();
        let before = map.len();
        apply_overrides(&mut map, &notty_config::Config::default());
        assert_eq!(map.len(), before);
    }
}
```

- [x] **Step 3: Ejecutar y ver que falla**

Run: `cargo test -p notty-input`
Expected: FAIL de compilación, `cannot find function parse_key_spec`.

- [x] **Step 4: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-input/src/keyspec.rs`:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// Traduce "Ctrl+Shift+H", "F3", "Ctrl+," etc. a (código de tecla virtual, modificadores).
/// Usa los mismos códigos VK_* que `notty_ui::keymap`.
pub fn parse_key_spec(s: &str) -> Option<(u32, Modifiers)> {
    if s.is_empty() {
        return None;
    }
    let mut m = Modifiers::default();
    let mut key = None;
    for part in s.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "" => continue,
            "ctrl" => m.ctrl = true,
            "shift" => m.shift = true,
            "alt" => m.alt = true,
            other => key = Some(key_code(other, part)?),
        }
    }
    key.map(|vk| (vk, m))
}

fn key_code(lower: &str, original: &str) -> Option<u32> {
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=12).contains(&n) {
            return Some(0x6F + n);
        }
    }
    if original.len() == 1 {
        let c = original.chars().next()?;
        return match c.to_ascii_uppercase() {
            'A'..='Z' => Some(c.to_ascii_uppercase() as u32),
            '0'..='9' => Some(c as u32),
            ',' => Some(0xBC),
            '.' => Some(0xBE),
            _ => None,
        };
    }
    match lower.as_ref() {
        "tab" => Some(0x09),
        "enter" | "return" => Some(0x0D),
        "esc" | "escape" => Some(0x1B),
        "space" => Some(0x20),
        "home" => Some(0x24),
        "end" => Some(0x23),
        "left" => Some(0x25),
        "right" => Some(0x27),
        "up" => Some(0x26),
        "down" => Some(0x28),
        "backspace" => Some(0x08),
        "delete" | "del" => Some(0x2E),
        _ => None,
    }
}
```

`crates/notty-input/src/command.rs`, añadir **encima** del módulo de tests:

```rust
use std::collections::HashMap;

use crate::{Modifiers, parse_key_spec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiCommand {
    OpenSettings,
    NewTab,
    NextTab,
    PrevTab,
    CloseTab,
}

pub fn default_ui_keymap() -> HashMap<(u32, Modifiers), UiCommand> {
    let bindings: &[(&str, UiCommand)] = &[
        ("Ctrl+,", UiCommand::OpenSettings),
        ("Ctrl+N", UiCommand::NewTab),
        ("Ctrl+Tab", UiCommand::NextTab),
        ("Ctrl+Shift+Tab", UiCommand::PrevTab),
        ("Ctrl+W", UiCommand::CloseTab),
    ];
    bindings
        .iter()
        .filter_map(|(spec, cmd)| parse_key_spec(spec).map(|key| (key, *cmd)))
        .collect()
}

/// Punto de extensión: cuando `config.toml` tenga una tabla `[keys]` con overrides
/// para comandos de interfaz, se aplican aquí. Por ahora `Config` no la tiene, así
/// que esta función es un no-op deliberado.
pub fn apply_overrides(_map: &mut HashMap<(u32, Modifiers), UiCommand>, _cfg: &notty_config::Config) {}
```

- [x] **Step 5: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS. `notty-input` pasa 10 tests; total del workspace 105 (95 anteriores + 10).

- [x] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/notty-input
git commit -m "feat(input): parser de teclas y comandos de interfaz"
```

---

### Task 3: notty-config: cargar y guardar (guardado atómico, config rota → defaults)

**Files:**
- Modify: `crates/notty-config/src/storage.rs`

**Interfaces:**
- Consumes: `Config`, `notty_io::atomic_write`.
- Produces:
  - `pub fn default_path() -> std::path::PathBuf` (`%APPDATA%\notty\config.toml`, vía la variable de entorno `APPDATA`; si no existe —caso de pruebas en otra plataforma—, cae a `./notty-config.toml` relativo al directorio actual).
  - `pub enum LoadResult { Loaded(Config), Defaulted(Config, String), Missing(Config) }` — `Loaded`: el archivo existía y era válido; `Defaulted`: existía pero no se pudo parsear (trae el mensaje de error); `Missing`: no existía. En los tres casos el `Config` devuelto es utilizable de inmediato (el de `Defaulted`/`Missing` es `Config::default()`).
  - `pub fn load(path: &Path) -> LoadResult`.
  - `pub fn save(cfg: &Config, path: &Path) -> std::io::Result<()>` (serializa con `toml::to_string_pretty` y usa `notty_io::atomic_write`; nunca se llama si `load` devolvió `Defaulted`, salvo que el usuario guarde explícitamente desde Ajustes, que es justo cuando SÍ se debe sobrescribir).

- [x] **Step 1: Escribir los tests que fallan**

Sustituir el contenido de `crates/notty-config/src/storage.rs` (que hoy solo tiene el comentario provisional) por:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn missing_file_returns_defaults() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        match load(&p) {
            LoadResult::Missing(cfg) => assert_eq!(cfg, Config::default()),
            other => panic!("esperaba Missing, fue {other:?}"),
        }
    }

    #[test]
    fn valid_file_loads_its_values() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        fs::write(&p, "[ui]\npreset = \"zen\"\n").unwrap();
        match load(&p) {
            LoadResult::Loaded(cfg) => assert_eq!(cfg.ui.preset, Preset::Zen),
            other => panic!("esperaba Loaded, fue {other:?}"),
        }
    }

    #[test]
    fn broken_file_falls_back_to_defaults_with_error_message() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        fs::write(&p, "esto no es toml válido [[[").unwrap();
        match load(&p) {
            LoadResult::Defaulted(cfg, msg) => {
                assert_eq!(cfg, Config::default());
                assert!(!msg.is_empty());
            }
            other => panic!("esperaba Defaulted, fue {other:?}"),
        }
    }

    #[test]
    fn broken_file_is_never_overwritten_by_load() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        fs::write(&p, "roto [[[").unwrap();
        load(&p);
        assert_eq!(fs::read_to_string(&p).unwrap(), "roto [[[");
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.ui.preset = Preset::Clasica;
        apply_preset(&mut cfg.ui, Preset::Clasica);
        save(&cfg, &p).unwrap();
        match load(&p) {
            LoadResult::Loaded(loaded) => assert_eq!(loaded.ui.preset, Preset::Clasica),
            other => panic!("esperaba Loaded, fue {other:?}"),
        }
    }

    #[test]
    fn default_path_ends_with_notty_config_toml() {
        let p = default_path();
        assert_eq!(p.file_name().unwrap(), "config.toml");
        assert_eq!(p.parent().unwrap().file_name().unwrap(), "notty");
    }
}
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-config storage`
Expected: FAIL de compilación, `cannot find function load`.

- [x] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-config/src/storage.rs`:

```rust
use std::fmt;
use std::path::{Path, PathBuf};

use crate::Config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadResult {
    Loaded(Config),
    Defaulted(Config, String),
    Missing(Config),
}

impl fmt::Display for LoadResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loaded(_) => write!(f, "Loaded"),
            Self::Defaulted(_, msg) => write!(f, "Defaulted({msg})"),
            Self::Missing(_) => write!(f, "Missing"),
        }
    }
}

pub fn default_path() -> PathBuf {
    match std::env::var_os("APPDATA") {
        Some(appdata) => PathBuf::from(appdata).join("notty").join("config.toml"),
        None => PathBuf::from("notty-config.toml"),
    }
}

pub fn load(path: &Path) -> LoadResult {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return LoadResult::Missing(Config::default()),
    };
    match toml::from_str::<Config>(&text) {
        Ok(cfg) => LoadResult::Loaded(cfg),
        Err(e) => LoadResult::Defaulted(Config::default(), e.to_string()),
    }
}

pub fn save(cfg: &Config, path: &Path) -> std::io::Result<()> {
    let text = toml::to_string_pretty(cfg).expect("Config siempre serializa");
    notty_io::create_parent_dirs(path)?;
    notty_io::atomic_write(path, text.as_bytes())
}
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, 111 tests (105 anteriores + 6 de esta tarea).

- [x] **Step 5: Commit**

```bash
git add crates/notty-config
git commit -m "feat(config): cargar y guardar config.toml de forma atómica"
```

---

### Task 4: Workspace de documentos, gutter y barra de atajos (lógica pura)

**Files:**
- Create: `crates/notty-ui/src/workspace.rs`
- Create: `crates/notty-ui/src/gutter.rs`
- Create: `crates/notty-ui/src/hints.rs`
- Modify: `crates/notty-ui/src/lib.rs`
- Modify: `crates/notty-ui/Cargo.toml`

**Interfaces:**
- Consumes: `EditorState` (Plan 2).
- Produces:
  - `pub struct Workspace { docs: Vec<EditorState> }` (privado, no `Vec` público) con `Workspace::new() -> Self` (un `EditorState::new_empty()`), `len() -> usize`, `active_index() -> usize`, `active() -> &EditorState`, `active_mut() -> &mut EditorState`, `open(EditorState)` (añade y activa), `activate(usize)` (clamped), `close_active() -> bool` (true si cerró una pestaña existente y quedan más; si era la última, la sustituye por un `EditorState::new_empty()` y devuelve `false`), `next()`, `prev()` (ambos cíclicos).
  - `pub fn gutter_width(total_lines: usize, digit_width_px: f32) -> f32`: ancho en píxeles necesario para mostrar el número de línea más largo, más un margen fijo de `12.0`.
  - `pub fn hints_text(vim: bool, raw: bool) -> &'static str`: la línea de la barra de atajos. Sin vim ni raw: `"^S Guardar   ^F Buscar   ^H Reemplazar   ^O Abrir   ^Alt+V Vim"`. Con `raw = true`: `"^S Guardar   ^Shift+H Ver como texto   ←→↑↓ Moverse"`. Con `vim = true` (y `raw = false`): `"i Insertar   hjkl Moverse   /Buscar   :w Guardar   ^Alt+V Salir de vim"`.

- [x] **Step 1: Escribir los tests que fallan**

`crates/notty-ui/src/workspace.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_one_empty_doc() {
        let w = Workspace::new();
        assert_eq!(w.len(), 1);
        assert_eq!(w.active_index(), 0);
    }

    #[test]
    fn open_adds_and_activates() {
        let mut w = Workspace::new();
        w.open(EditorState::new_empty());
        assert_eq!(w.len(), 2);
        assert_eq!(w.active_index(), 1);
    }

    #[test]
    fn next_and_prev_wrap_around() {
        let mut w = Workspace::new();
        w.open(EditorState::new_empty());
        w.open(EditorState::new_empty());
        w.activate(0);
        w.prev();
        assert_eq!(w.active_index(), 2);
        w.next();
        assert_eq!(w.active_index(), 0);
    }

    #[test]
    fn close_active_removes_it_and_keeps_a_valid_index() {
        let mut w = Workspace::new();
        w.open(EditorState::new_empty());
        w.activate(0);
        assert!(w.close_active());
        assert_eq!(w.len(), 1);
        assert_eq!(w.active_index(), 0);
    }

    #[test]
    fn closing_the_last_doc_replaces_it_instead_of_leaving_zero() {
        let mut w = Workspace::new();
        assert!(!w.close_active());
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn activate_clamps_out_of_range() {
        let mut w = Workspace::new();
        w.activate(99);
        assert_eq!(w.active_index(), 0);
    }
}
```

`crates/notty-ui/src/gutter.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_digit_lines() {
        assert_eq!(gutter_width(9, 10.0), 22.0);
    }

    #[test]
    fn three_digit_lines() {
        assert_eq!(gutter_width(120, 10.0), 42.0);
    }

    #[test]
    fn zero_lines_still_shows_one_digit() {
        assert_eq!(gutter_width(0, 10.0), 22.0);
    }
}
```

`crates/notty-ui/src/hints.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_mode_hints() {
        assert!(hints_text(false, false).contains("^S Guardar"));
        assert!(hints_text(false, false).contains("^Alt+V Vim"));
    }

    #[test]
    fn raw_mode_hints_mention_moving_bytes() {
        assert!(hints_text(false, true).contains("Ver como texto"));
    }

    #[test]
    fn vim_mode_hints_mention_insert() {
        assert!(hints_text(true, false).contains("Insertar"));
    }

    #[test]
    fn raw_takes_priority_over_vim() {
        assert_eq!(hints_text(true, true), hints_text(false, true));
    }
}
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui workspace gutter hints`
Expected: FAIL de compilación, `cannot find type Workspace` (y las otras dos funciones).

- [x] **Step 3: Implementar**

Añadir **encima** del módulo de tests en `crates/notty-ui/src/workspace.rs`:

```rust
use crate::EditorState;

/// Varios documentos abiertos a la vez. Siempre tiene al menos uno.
pub struct Workspace {
    docs: Vec<EditorState>,
    active: usize,
}

impl Workspace {
    pub fn new() -> Self {
        Self { docs: vec![EditorState::new_empty()], active: 0 }
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> &EditorState {
        &self.docs[self.active]
    }

    pub fn active_mut(&mut self) -> &mut EditorState {
        &mut self.docs[self.active]
    }

    pub fn iter(&self) -> impl Iterator<Item = &EditorState> {
        self.docs.iter()
    }

    pub fn open(&mut self, state: EditorState) {
        self.docs.push(state);
        self.active = self.docs.len() - 1;
    }

    pub fn activate(&mut self, idx: usize) {
        self.active = idx.min(self.docs.len() - 1);
    }

    pub fn next(&mut self) {
        self.active = (self.active + 1) % self.docs.len();
    }

    pub fn prev(&mut self) {
        self.active = (self.active + self.docs.len() - 1) % self.docs.len();
    }

    /// Cierra la pestaña activa. Devuelve `true` si de verdad quedó una lista más
    /// corta; si era la última, la sustituye por un documento vacío y devuelve `false`.
    pub fn close_active(&mut self) -> bool {
        if self.docs.len() == 1 {
            self.docs[0] = EditorState::new_empty();
            return false;
        }
        self.docs.remove(self.active);
        self.active = self.active.min(self.docs.len() - 1);
        true
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}
```

`crates/notty-ui/src/gutter.rs`, añadir **encima** del módulo de tests:

```rust
/// Ancho en píxeles de la columna de números de línea: dígitos del número más
/// largo (mínimo 1) por el ancho de un dígito, más 12px de margen a cada lado.
pub fn gutter_width(total_lines: usize, digit_width_px: f32) -> f32 {
    let digits = total_lines.max(1).to_string().len().max(1) as f32;
    digits * digit_width_px + 12.0
}
```

(el test `one_digit_lines` espera `22.0` para `total_lines=9, digit_width_px=10.0`: 1 dígito × 10 + 12 = 22; `three_digit_lines` espera `42.0` para 120 líneas: `"120".len() == 3`, 3×10+12=42.)

`crates/notty-ui/src/hints.rs`, añadir **encima** del módulo de tests:

```rust
/// Texto de la barra de atajos estilo nano. `raw` tiene prioridad sobre `vim`
/// porque la vista raw usa su propio conjunto de teclas, sea cual sea el modo de texto.
pub fn hints_text(vim: bool, raw: bool) -> &'static str {
    if raw {
        "^S Guardar   ^Shift+H Ver como texto   ←→↑↓ Moverse"
    } else if vim {
        "i Insertar   hjkl Moverse   /Buscar   :w Guardar   ^Alt+V Salir de vim"
    } else {
        "^S Guardar   ^F Buscar   ^H Reemplazar   ^O Abrir   ^Alt+V Vim"
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
pub mod render;
mod status;
mod viewport;
pub mod window;
pub mod workspace;

pub use doc_io::{OpenedDoc, open_as_document, save_document};
pub use editor::EditorState;
pub use gutter::gutter_width;
pub use hints::hints_text;
pub use keymap::{EditorAction, Modifiers, action_for_vk};
pub use render::Renderer;
pub use status::status_line;
pub use viewport::Viewport;
pub use workspace::Workspace;
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, 126 tests (111 anteriores + 15 de esta tarea).

- [x] **Step 5: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): Workspace multi-documento, gutter y texto de la barra de atajos"
```

---

### Task 5: Renderer: pestañas, menú, barra de atajos y números de línea

**Files:**
- Modify: `crates/notty-ui/src/render.rs`
- Modify: `crates/notty-ui/src/window.rs`

**Interfaces:**
- Consumes: `notty_config::UiConfig`, `Workspace`, `gutter_width`, `hints_text`.
- Produces: `Renderer::paint` gana un parámetro `ui: &notty_config::UiConfig` y ahora también recibe el `Workspace` completo (no un único `EditorState`) para poder dibujar la tira de pestañas: `pub fn paint(&mut self, ws: &Workspace, ui: &UiConfig)`. Devuelve además `pub fn tab_rects(&self, ws: &Workspace) -> Vec<D2D_RECT_F>` (los rectángulos de cada pestaña dibujada la última vez, para que `window.rs` pueda hacer hit-testing del clic; se recalculan en cada `paint` y se guardan en el propio `Renderer`).

- [x] **Step 1: Añadir la pestaña, el menú y la barra de atajos al render**

Modificar `crates/notty-ui/src/render.rs`:

1. Añadir un campo `tab_rects: Vec<(f32, f32, f32, f32)>` a `struct Renderer` (inicializado a `Vec::new()` en `new`).
2. Cambiar la firma de `paint` a `pub fn paint(&mut self, ws: &crate::Workspace, ui: &notty_config::UiConfig)`.
3. Calcular tres franjas antes de dibujar las líneas de texto:
   - **Tira de pestañas** (alto = `self.line_height`), solo si `ui.files == Files::Tabs` y (`ui.tabs_position != Hidden`) y (`ui.tabs_position != Auto || ws.len() > 1`). Recorre `ws.iter()`, dibuja cada nombre de pestaña (usa el nombre de archivo de `state.path`, o `"sin título"` si `None`, con un `•` si `state.doc.is_dirty()`) con un ancho fijo de 160px cada una, resaltando la del índice `ws.active_index()` con un fondo `sel_brush` ya existente. Guarda los rectángulos calculados en `self.tab_rects`.
   - **Barra de menús** (alto = `self.line_height`), solo si `ui.menubar == MenuBar::Visible` (el modo `Alt` se gestiona en `window.rs`, que decide cuándo pasar `Visible` o no a esta llamada — ver Step 3): dibuja el texto `"Archivo   Editar   Buscar   Ver   Ayuda"` alineado a la izquierda.
   - **Barra de atajos** (alto = `self.line_height`), en la parte inferior, justo encima de la barra de estado, solo si `ui.hints_bar`: dibuja `crate::hints_text(false, false)` (el modo vim/raw llega en el Plan 5; por ahora siempre `false, false`).
4. Ajustar el punto de partida `y` de las líneas de texto para dejar hueco a las franjas superiores activas, y el hueco inferior para sumar la barra de atajos (si está activa) además de la barra de estado que ya existía.
5. Números de línea: si `ui.line_numbers`, calcular `let gw = crate::gutter_width(ws.active().doc.buffer().len_lines(), self.digit_width());` (añade un método `digit_width(&self) -> f32` que mida el ancho de `"0"` con `IDWriteTextLayout` de una sola línea, o usa una aproximación fija `FONT_SIZE * 0.6` si medirlo es complicado de compilar) y desplaza el `PADDING_X` del texto y del caret en `gw`, dibujando el número de cada línea visible alineado a la derecha del hueco con `fg_brush` a menor opacidad.
6. `pub fn tab_rects(&self) -> &[(f32, f32, f32, f32)] { &self.tab_rects }`.

Ajusta cualquier firma de `windows` que haga falta al compilar, manteniendo el comportamiento descrito.

- [x] **Step 2: Actualizar los llamadores existentes de `paint`**

En `crates/notty-ui/src/window.rs`, todo lo que hasta ahora llamaba a `renderer.paint(&state)` con un único `EditorState` pasa a usar un `Workspace` y a leer la config actual (ver Task 8 de este plan, que hace el resto de la integración de `window.rs`; en este Step 2 basta con dejarlo compilando con una config por defecto de marcador de posición mínima, por ejemplo `notty_config::UiConfig::default()`, que la Task 8 sustituye por la de verdad).

- [x] **Step 3: Comprobar que compila**

Run: `cargo build --workspace`
Expected: compila (con los ajustes de firma que hayan hecho falta).

- [x] **Step 4: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): render de pestañas, menú, barra de atajos y números de línea"
```

---

### Task 6: window.rs: Workspace, config al arrancar y piezas de ventana

**Files:**
- Modify: `crates/notty-ui/src/window.rs`
- Modify: `crates/notty/src/main.rs`

**Interfaces:**
- Consumes: `notty_config::{load, default_path, Config}`, `notty_input::{default_ui_keymap, apply_overrides, UiCommand}`, `Workspace`, `Renderer::paint(ws, ui)`, `Renderer::tab_rects`.
- Produces: `pub fn run(initial_path: Option<&str>, cfg: notty_config::Config) -> windows::core::Result<()>` (nueva firma; `main.rs` carga la config antes de llamar). La ventana guarda ahora `Workspace`, `notty_config::Config` y el `HashMap` de `notty-input` en su estado por ventana (mismo mecanismo que ya guardaba `EditorState`/`Renderer` desde el Plan 2).

- [x] **Step 1: Cargar la configuración en `main.rs`**

`crates/notty/src/main.rs`:

```rust
//! Punto de entrada de notty: interpreta argv, carga la configuración y abre la ventana.

fn main() -> windows::core::Result<()> {
    let path = std::env::args().nth(1);
    let cfg = match notty_config::load(&notty_config::default_path()) {
        notty_config::LoadResult::Loaded(cfg) | notty_config::LoadResult::Missing(cfg) => cfg,
        notty_config::LoadResult::Defaulted(cfg, _msg) => cfg, // el aviso se muestra en pantalla en la Task 9
    };
    notty_ui::window::run(path.as_deref(), cfg)
}
```

`crates/notty/Cargo.toml`, añadir dependencia:

```toml
notty-config = { path = "../notty-config" }
```

- [x] **Step 2: Sustituir `EditorState` por `Workspace` en `window.rs`**

Cambios en `crates/notty-ui/src/window.rs`:

1. La firma de `run` pasa a `pub fn run(path: Option<&str>, cfg: notty_config::Config) -> Result<()>`.
2. Donde antes se creaba `let mut state = EditorState::new_empty()` o `EditorState::from_opened(...)`, ahora se crea `let mut ws = Workspace::new();` y, si `path` es `Some`, se hace `ws.active_mut() = ...` sustituyendo el documento inicial por el abierto (reutiliza el mismo patrón que la Task 5 del Plan 2 usaba para `EditorState`, aplicado ahora al `Workspace` recién creado con un único documento vacío).
3. Guardar también `cfg: notty_config::Config` y `let mut ui_keymap = { let mut m = notty_input::default_ui_keymap(); notty_input::apply_overrides(&mut m, &cfg); m };` junto al resto del estado por ventana.
4. Cada llamada a `renderer.paint(&state)` pasa a `renderer.paint(&ws, &cfg.ui)`.
5. `WM_KEYDOWN`: antes de traducir con `notty_ui::action_for_vk`, comprobar primero `ui_keymap.get(&(vk, notty_input::Modifiers { ctrl: mods.ctrl, shift: mods.shift, alt: mods.alt }))` (construir el `notty_input::Modifiers` a partir del mismo `mods: notty_ui::Modifiers` que ya se calculaba en el Plan 2). Si hay un `UiCommand`, manejarlo (ver Step 3); si no, seguir como hasta ahora con `EditorAction` sobre `ws.active_mut()`.
6. Todo lo que en el Plan 2 leía o mutaba `state` (guardar, portapapeles, ratón, `WM_CHAR`...) pasa a leer/mutar `ws.active_mut()`; recalcular el título de la ventana a partir de `ws.active()`.

- [x] **Step 2: Ejecutar los tests existentes**

Run: `cargo test --workspace`
Expected: PASS (los cambios de esta tarea son solo de integración en `window.rs`/`main.rs`, que no tienen tests automáticos propios; deben seguir pasando los 126 tests anteriores sin cambios).

- [x] **Step 3: Manejar los `UiCommand`**

Dentro del `match` de `WM_KEYDOWN` en `window.rs`, tras resolver un `UiCommand`:

```rust
match cmd {
    notty_input::UiCommand::NewTab => ws.open(crate::EditorState::new_empty()),
    notty_input::UiCommand::NextTab => ws.next(),
    notty_input::UiCommand::PrevTab => ws.prev(),
    notty_input::UiCommand::CloseTab => { ws.close_active(); }
    notty_input::UiCommand::OpenSettings => { /* se conecta en la Task 9 */ }
}
```

- [x] **Step 4: Hit-testing de clic en pestañas**

En el manejador de `WM_LBUTTONDOWN`, antes de tratar el clic como "mover el caret en el texto", comprobar si `(x, y)` cae dentro de alguno de `renderer.tab_rects()`; si es así, `ws.activate(i)` y `InvalidateRect`, sin propagar el clic al editor.

- [x] **Step 5: Menú que aparece con Alt**

Si `cfg.ui.menubar == notty_config::MenuBar::Alt`, guardar un `bool` `menu_visible` en el estado de la ventana (inicialmente `false`), alternarlo en `WM_SYSKEYDOWN`/`WM_KEYDOWN` cuando `vk == VK_MENU` (0x12) sin otros modificadores, y pasar a `renderer.paint` una copia de `cfg.ui` con `menubar` forzado a `Visible`/`Hidden` según ese `bool` (el propio `cfg.ui.menubar` en disco se queda como `Alt`; solo se ajusta la copia usada para pintar).

- [x] **Step 6: Compilar y comprobar manualmente**

Run: `cargo build --workspace`
Expected: compila.

Run: `cargo run --bin notty -- /tmp/notas.txt` (o el archivo de prueba del Plan 2)
Expected (manual): la ventana arranca con el preset Moderna (pestañas en la barra de título, sin menú, con barra de atajos abajo). `Ctrl+N` abre una pestaña nueva vacía; `Ctrl+Tab`/`Ctrl+Shift+Tab` cambian de pestaña; `Ctrl+W` cierra la activa; clic en una pestaña la activa.

- [x] **Step 7: Commit**

```bash
git add crates/notty crates/notty-ui
git commit -m "feat(ui): Workspace, config al arrancar y comandos de interfaz en la ventana"
```

---

### Task 7: Presets y pestañas/buffers desde config.toml

**Files:**
- Modify: `crates/notty-ui/src/window.rs`

**Interfaces:**
- Consumes: `cfg.ui.files` (`Files::Tabs`/`Files::Buffers`).
- Produces: cuando `cfg.ui.files == Files::Buffers`, la tira de pestañas nunca se dibuja (ya lo cubre la condición de la Task 5, `ui.files == Files::Tabs`) y el cambio de documento activo se hace solo con `Ctrl+Tab`/`Ctrl+Shift+Tab` (ya implementado en la Task 6): no hace falta código nuevo de lógica, solo la comprobación manual de que las tres combinaciones (preset × modo de archivos) se ven como en la maqueta.

- [x] **Step 1: Comprobación manual de los tres presets**

Run: `cargo run --bin notty -- /tmp/notas.txt` con, cada vez, un `config.toml` distinto en `%APPDATA%\notty\config.toml` (créalo a mano para esta comprobación):

```toml
[ui]
preset = "moderna"
```

Expected (manual): pestañas en la barra de título, sin menú, barra de atajos visible.

```toml
[ui]
preset = "clasica"
```

Expected (manual): barra de menús visible arriba, pestañas debajo, sin barra de atajos.

```toml
[ui]
preset = "zen"
files = "buffers"
```

Expected (manual): sin pestañas (con un solo documento abierto no se verían de todas formas por ser `Auto`; abre un segundo con `Ctrl+N` y comprueba que, al ser `Buffers`, tampoco aparecen pestañas aunque haya dos documentos), sin menú, sin barra de atajos.

- [x] **Step 2: Commit (solo si hiciste algún ajuste durante la comprobación)**

```bash
git add -A
git commit -m "fix(ui): ajustes tras comprobar los tres presets" --allow-empty
```

---

### Task 8: Ventana de Ajustes: estructura y controles nativos

**Files:**
- Create: `crates/notty-ui/src/settings_window.rs`
- Modify: `crates/notty-ui/src/lib.rs`
- Modify: `crates/notty-ui/Cargo.toml`

**Interfaces:**
- Consumes: `notty_config::{Config, Preset, Theme, apply_preset, save, default_path}`.
- Produces: `pub fn open(parent: HWND, cfg: Rc<RefCell<notty_config::Config>>, on_change: Box<dyn Fn()>) -> windows::core::Result<()>` en `notty-ui::settings_window`: crea una ventana hija/propietaria de 420×360 con:
  - Un grupo de 3 `BUTTON` con estilo `BS_AUTORADIOBUTTON` para el preset (Moderna/Clásica/Zen).
  - Un grupo de 3 `BUTTON` con estilo `BS_AUTORADIOBUTTON` para el tema (Sistema/Claro/Oscuro).
  - Un `BUTTON` con estilo `BS_AUTOCHECKBOX` para números de línea.
  - Un grupo de 2 `BUTTON` con estilo `BS_AUTORADIOBUTTON` para pestañas/buffers.
  - Cada control, al recibir `WM_COMMAND` (`BN_CLICKED`), actualiza `cfg.borrow_mut()`, llama a `notty_config::save(&cfg.borrow(), &notty_config::default_path())` y luego a `on_change()` (para que la ventana principal recargue y repinte).

- [x] **Step 1: Añadir controles Win32**

Run:

```bash
cargo add windows -p notty-ui --features Win32_UI_Controls
```

- [x] **Step 2: Implementar la ventana de Ajustes**

`crates/notty-ui/src/settings_window.rs`. Arquitectura: una clase de ventana nueva (`"NottySettingsClass"`), hijos `BUTTON` creados con `CreateWindowExW` usando la clase predefinida `w!("BUTTON")` y los estilos `WS_CHILD | WS_VISIBLE | BS_AUTORADIOBUTTON` (o `BS_AUTOCHECKBOX`), cada uno con un id numérico distinto (`HMENU` construido desde el id con `HMENU(id as _)`). El estado (`Config` compartido y el callback) se guarda en `GWLP_USERDATA` de la ventana de Ajustes, igual que ya se hace para la ventana principal.

```rust
use std::cell::RefCell;
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BM_GETCHECK, BM_SETCHECK, BST_CHECKED, BST_UNCHECKED,
    CreateWindowExW, DefWindowProcW, RegisterClassExW, SendMessageW, ShowWindow, SW_SHOW,
    WM_COMMAND, WM_DESTROY, WNDCLASSEXW, WS_CHILD, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};
use windows::core::{Result, w};

use notty_config::{Config, Files, Preset, Theme, apply_preset};

const ID_PRESET_MODERNA: usize = 100;
const ID_PRESET_CLASICA: usize = 101;
const ID_PRESET_ZEN: usize = 102;
const ID_THEME_SYSTEM: usize = 110;
const ID_THEME_LIGHT: usize = 111;
const ID_THEME_DARK: usize = 112;
const ID_LINE_NUMBERS: usize = 120;
const ID_FILES_TABS: usize = 130;
const ID_FILES_BUFFERS: usize = 131;

struct State {
    cfg: Rc<RefCell<Config>>,
    on_change: Box<dyn Fn()>,
}

/// Abre la ventana de Ajustes. `on_change` se llama cada vez que el usuario
/// cambia algo (ya guardado en disco), para que la ventana principal repinte.
pub fn open(parent: HWND, cfg: Rc<RefCell<Config>>, on_change: Box<dyn Fn()>) -> Result<()> {
    unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
        let class_name = w!("NottySettingsClass");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            w!("Ajustes · notty"),
            WS_OVERLAPPEDWINDOW,
            windows::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
            windows::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
            420,
            360,
            parent,
            None,
            instance,
            None,
        )?;

        let preset = cfg.borrow().ui.preset;
        let theme = cfg.borrow().ui.theme;
        let line_numbers = cfg.borrow().ui.line_numbers;
        let files = cfg.borrow().ui.files;

        radio(hwnd, instance, "Moderna", 20, 20, ID_PRESET_MODERNA, preset == Preset::Moderna);
        radio(hwnd, instance, "Clásica", 20, 48, ID_PRESET_CLASICA, preset == Preset::Clasica);
        radio(hwnd, instance, "Zen", 20, 76, ID_PRESET_ZEN, preset == Preset::Zen);

        radio(hwnd, instance, "Tema: sistema", 20, 116, ID_THEME_SYSTEM, theme == Theme::System);
        radio(hwnd, instance, "Tema: claro", 20, 144, ID_THEME_LIGHT, theme == Theme::Light);
        radio(hwnd, instance, "Tema: oscuro", 20, 172, ID_THEME_DARK, theme == Theme::Dark);

        checkbox(hwnd, instance, "Números de línea", 20, 212, ID_LINE_NUMBERS, line_numbers);

        radio(hwnd, instance, "Pestañas", 20, 252, ID_FILES_TABS, files == Files::Tabs);
        radio(hwnd, instance, "Buffers", 160, 252, ID_FILES_BUFFERS, files == Files::Buffers);

        let state = Box::new(State { cfg, on_change });
        windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            Box::into_raw(state) as isize,
        );

        let _ = ShowWindow(hwnd, SW_SHOW);
    }
    Ok(())
}

unsafe fn radio(parent: HWND, instance: windows::Win32::Foundation::HMODULE, text: &str, x: i32, y: i32, id: usize, checked: bool) {
    make_button(parent, instance, text, x, y, id, BS_AUTORADIOBUTTON.0, checked);
}

unsafe fn checkbox(parent: HWND, instance: windows::Win32::Foundation::HMODULE, text: &str, x: i32, y: i32, id: usize, checked: bool) {
    make_button(parent, instance, text, x, y, id, BS_AUTOCHECKBOX.0, checked);
}

unsafe fn make_button(parent: HWND, instance: windows::Win32::Foundation::HMODULE, text: &str, x: i32, y: i32, id: usize, style_bits: u32, checked: bool) {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let style = WS_CHILD.0 | WS_VISIBLE.0 | style_bits;
    if let Ok(btn) = CreateWindowExW(
        Default::default(),
        w!("BUTTON"),
        windows::core::PCWSTR(wide.as_ptr()),
        windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(style),
        x,
        y,
        200,
        24,
        parent,
        windows::Win32::UI::WindowsAndMessaging::HMENU(id as _),
        instance,
        None,
    ) {
        if checked {
            let _ = SendMessageW(btn, BM_SETCHECK, Some(WPARAM(BST_CHECKED.0 as usize)), None);
        }
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_COMMAND => {
                let id = (wparam.0 & 0xFFFF) as usize;
                let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                ) as *mut State;
                if !ptr.is_null() {
                    let state = &mut *ptr;
                    apply_control(state, id);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                ) as *mut State;
                if !ptr.is_null() {
                    drop(Box::from_raw(ptr));
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn apply_control(state: &mut State, id: usize) {
    let mut cfg = state.cfg.borrow_mut();
    match id {
        ID_PRESET_MODERNA => apply_preset(&mut cfg.ui, Preset::Moderna),
        ID_PRESET_CLASICA => apply_preset(&mut cfg.ui, Preset::Clasica),
        ID_PRESET_ZEN => apply_preset(&mut cfg.ui, Preset::Zen),
        ID_THEME_SYSTEM => cfg.ui.theme = Theme::System,
        ID_THEME_LIGHT => cfg.ui.theme = Theme::Light,
        ID_THEME_DARK => cfg.ui.theme = Theme::Dark,
        ID_LINE_NUMBERS => cfg.ui.line_numbers = !cfg.ui.line_numbers,
        ID_FILES_TABS => cfg.ui.files = Files::Tabs,
        ID_FILES_BUFFERS => cfg.ui.files = Files::Buffers,
        _ => return,
    }
    let _ = notty_config::save(&cfg, &notty_config::default_path());
    drop(cfg);
    (state.on_change)();
}
```

Ajusta las firmas exactas (`HMENU(id as _)`, `WINDOW_STYLE`, si `SendMessageW` pide `Option<WPARAM>`/`Option<LPARAM>` u otra forma) al compilar, manteniendo: cada control refleja el valor actual al abrir, y al pulsarlo actualiza `Config`, lo guarda en disco de inmediato y avisa a la ventana principal.

`crates/notty-ui/src/lib.rs` añade:

```rust
pub mod settings_window;
```

(en la lista de `mod`/`pub mod`, junto a `pub mod window;`).

- [x] **Step 3: Comprobar que compila**

Run: `cargo build --workspace`
Expected: compila.

- [x] **Step 4: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): ventana de Ajustes con controles Win32 nativos"
```

---

### Task 9: Conectar Ajustes con la ventana principal y avisos de config rota

**Files:**
- Modify: `crates/notty-ui/src/window.rs`

**Interfaces:**
- Consumes: `settings_window::open`, `notty_config::LoadResult`.
- Produces: `run` pasa a aceptar `notty_config::LoadResult` en vez de `Config` directamente (para poder mostrar el aviso de config rota); `Ctrl+,` (`UiCommand::OpenSettings`) abre la ventana de Ajustes sobre la principal.

- [x] **Step 1: Cambiar la firma de entrada para poder avisar de config rota**

En `crates/notty-ui/src/window.rs`, cambiar `pub fn run(path: Option<&str>, cfg: Config)` por `pub fn run(path: Option<&str>, load: notty_config::LoadResult)`, extrayendo dentro la `Config` y, si es la variante `Defaulted(_, msg)`, guardando `msg` para mostrarlo como primer mensaje de la barra de estado nada más arrancar (reutiliza el mecanismo de aviso ya usado para otros mensajes transitorios de la barra de estado del Plan 2, o simplemente antepón `format!("config.toml roto: {msg}")` al título de la ventana si no existe todavía un mecanismo de mensaje temporal — lo que compile más simple).

`crates/notty/src/main.rs` pasa a:

```rust
fn main() -> windows::core::Result<()> {
    let path = std::env::args().nth(1);
    let load = notty_config::load(&notty_config::default_path());
    notty_ui::window::run(path.as_deref(), load)
}
```

- [x] **Step 2: Envolver `Config` en `Rc<RefCell<_>>` y compartirla con Ajustes**

En `window.rs`, la variable `cfg` pasa a ser `let cfg = Rc::new(RefCell::new(cfg));`. Cada lectura existente de `cfg.ui.*` para pintar pasa a `cfg.borrow().ui`.

- [x] **Step 3: Abrir Ajustes con Ctrl+,**

En el `match cmd` de `UiCommand` (Task 6, Step 3), sustituir el comentario `/* se conecta en la Task 9 */` por:

```rust
notty_input::UiCommand::OpenSettings => {
    let cfg_for_settings = cfg.clone();
    let hwnd_copy = hwnd;
    let _ = crate::settings_window::open(
        hwnd,
        cfg_for_settings,
        Box::new(move || {
            let _ = windows::Win32::UI::WindowsAndMessaging::InvalidateRect(Some(hwnd_copy), None, false);
        }),
    );
}
```

(El closure solo pide repintar; como `cfg` ya es el `Rc<RefCell<_>>` compartido, `renderer.paint(&ws, &cfg.borrow().ui)` en `WM_PAINT` ya lee el valor actualizado sin más cambios.)

- [x] **Step 4: Compilar y comprobar manualmente**

Run: `cargo build --workspace`
Expected: compila.

Run: `cargo run --bin notty -- /tmp/notas.txt`
Expected (manual): `Ctrl+,` abre una ventana "Ajustes · notty" con los radios y el checkbox reflejando el `config.toml` actual. Cambiar el preset a Clásica hace que, al volver a la ventana principal (sin cerrarla), aparezca la barra de menús. Comprobar que `%APPDATA%\notty\config.toml` se actualizó con el nuevo preset.

Probar también con un `config.toml` roto a propósito (`echo "roto [[[" > "%APPDATA%\notty\config.toml"`): notty arranca igualmente con los valores por defecto y el archivo roto sigue en disco sin tocar.

- [x] **Step 5: Commit**

```bash
git add crates/notty crates/notty-ui
git commit -m "feat(ui): conecta Ajustes con la ventana principal y avisa de config rota"
```

---

### Task 10: Comprobación final del workspace

**Files:** ninguno nuevo; solo verificación.

- [ ] **Step 1: Tests y lints de todo el workspace**

Run: `cargo test --workspace`
Expected: PASS, 126 tests (ninguna tarea de este plan a partir de la 5 añade tests automáticos nuevos, son integración Win32).

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: sin avisos. Corregir cualquier aviso en el archivo que lo señale y repetir hasta que quede limpio.

- [ ] **Step 2: Build release**

Run: `cargo build --release --workspace`
Expected: compila sin errores.

- [ ] **Step 3: Commit (si hubo cambios de la revisión)**

```bash
git add -A
git commit -m "chore(ui): pasa clippy y build release tras config/ventana" --allow-empty
```

---

## Hoja de ruta: planes siguientes

| Plan | Contenido |
|---|---|
| **4 · Línea de ruta y búsqueda** | Prompts en la barra de estado: ruta con sugerencias/`Tab`, buscar/reemplazar con `Ctrl+F`/`Ctrl+H`, `F3`. |
| **5 · Vim y raw** | Keymap vim, vista hexadecimal sobre `RawBytes`, resto de la sección "Teclado" de Ajustes. |
| **6 · Archivos vivos** | Temporales, instancia única, daemon/atajo global, autoguardado, conflictos, recuperación tras caída, resto de la sección "Archivos" y "Atajo global" de Ajustes. |
| **7 · Pulido y garantías** | UI Automation, alto contraste/DPI, fuzzing, presupuesto de rendimiento en CI. |
