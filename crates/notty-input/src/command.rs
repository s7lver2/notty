use std::collections::HashMap;

use crate::{Modifiers, format_key_spec, parse_key_spec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiCommand {
    OpenSettings,
    NewTab,
    NewTempTab,
    NextTab,
    PrevTab,
    CloseTab,
}

/// Todo comando con atajo reasignable desde Ajustes → Teclado o `[keys]`. Incluye los
/// de `UiCommand` y los dos interruptores del editor (vim/raw), que `window.rs`
/// resuelve aparte de `ui_keymap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Command {
    OpenSettings,
    NewTab,
    NewTempTab,
    NextTab,
    PrevTab,
    CloseTab,
    ToggleVim,
    ToggleRaw,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    SplitPane,
    ClosePane,
    FocusPaneLeft,
    FocusPaneRight,
}

impl Command {
    pub const ALL: &'static [Command] = &[
        Command::NewTab,
        Command::NewTempTab,
        Command::CloseTab,
        Command::NextTab,
        Command::PrevTab,
        Command::OpenSettings,
        Command::ToggleVim,
        Command::ToggleRaw,
        Command::ZoomIn,
        Command::ZoomOut,
        Command::ZoomReset,
        Command::SplitPane,
        Command::ClosePane,
        Command::FocusPaneLeft,
        Command::FocusPaneRight,
    ];

    /// Clave en `[keys]` de config.toml.
    pub fn name(self) -> &'static str {
        match self {
            Command::OpenSettings => "open_settings",
            Command::NewTab => "new_tab",
            Command::NewTempTab => "new_temp_tab",
            Command::NextTab => "next_tab",
            Command::PrevTab => "prev_tab",
            Command::CloseTab => "close_tab",
            Command::ToggleVim => "toggle_vim",
            Command::ToggleRaw => "toggle_raw",
            Command::ZoomIn => "zoom_in",
            Command::ZoomOut => "zoom_out",
            Command::ZoomReset => "zoom_reset",
            Command::SplitPane => "split_pane",
            Command::ClosePane => "close_pane",
            Command::FocusPaneLeft => "focus_pane_left",
            Command::FocusPaneRight => "focus_pane_right",
        }
    }

    pub fn from_name(name: &str) -> Option<Command> {
        Command::ALL.iter().copied().find(|c| c.name() == name)
    }

    pub fn title(self) -> &'static str {
        match self {
            Command::OpenSettings => "Abrir Ajustes",
            Command::NewTab => "Nueva pestaña",
            Command::NewTempTab => "Nueva pestaña temporal",
            Command::NextTab => "Pestaña siguiente",
            Command::PrevTab => "Pestaña anterior",
            Command::CloseTab => "Cerrar pestaña",
            Command::ToggleVim => "Alternar vim en esta ventana",
            Command::ToggleRaw => "Ver como raw",
            Command::ZoomIn => "Aumentar tamaño del texto",
            Command::ZoomOut => "Reducir tamaño del texto",
            Command::ZoomReset => "Restablecer tamaño del texto",
            Command::SplitPane => "Dividir panel",
            Command::ClosePane => "Cerrar panel",
            Command::FocusPaneLeft => "Panel de la izquierda",
            Command::FocusPaneRight => "Panel de la derecha",
        }
    }

    pub fn default_spec(self) -> &'static str {
        match self {
            Command::OpenSettings => "Ctrl+,",
            Command::NewTab => "Ctrl+N",
            Command::NewTempTab => "Ctrl+Shift+N",
            Command::NextTab => "Ctrl+Tab",
            Command::PrevTab => "Ctrl+Shift+Tab",
            Command::CloseTab => "Ctrl+W",
            Command::ToggleVim => "Ctrl+Alt+V",
            Command::ToggleRaw => "Ctrl+Shift+H",
            Command::ZoomIn => "Ctrl+=",
            Command::ZoomOut => "Ctrl+-",
            Command::ZoomReset => "Ctrl+0",
            Command::SplitPane => "Ctrl+Shift+Enter",
            Command::ClosePane => "Ctrl+Shift+W",
            Command::FocusPaneLeft => "Ctrl+Shift+Left",
            Command::FocusPaneRight => "Ctrl+Shift+Right",
        }
    }

    pub fn ui(self) -> Option<UiCommand> {
        match self {
            Command::OpenSettings => Some(UiCommand::OpenSettings),
            Command::NewTab => Some(UiCommand::NewTab),
            Command::NewTempTab => Some(UiCommand::NewTempTab),
            Command::NextTab => Some(UiCommand::NextTab),
            Command::PrevTab => Some(UiCommand::PrevTab),
            Command::CloseTab => Some(UiCommand::CloseTab),
            Command::ToggleVim
            | Command::ToggleRaw
            | Command::ZoomIn
            | Command::ZoomOut
            | Command::ZoomReset
            | Command::SplitPane
            | Command::ClosePane
            | Command::FocusPaneLeft
            | Command::FocusPaneRight => None,
        }
    }

    /// Solo actúan con `Files::Splits`; en otro modo la tecla sigue su camino normal.
    pub fn is_pane(self) -> bool {
        matches!(self, Command::SplitPane | Command::ClosePane | Command::FocusPaneLeft | Command::FocusPaneRight)
    }
}

/// El atajo en vigor de `cmd` como texto: el de `[keys]` si lo hay (vacío = sin
/// atajo), si no el de fábrica. Siempre normalizado ("ctrl+n" → "Ctrl+N").
pub fn binding_spec(cfg: &notty_config::Config, cmd: Command) -> String {
    match binding(cfg, cmd) {
        Some((vk, m)) => format_key_spec(vk, m).unwrap_or_default(),
        None => String::new(),
    }
}

/// El atajo en vigor de `cmd` (ver `binding_spec`). `None` = sin atajo.
pub fn binding(cfg: &notty_config::Config, cmd: Command) -> Option<(u32, Modifiers)> {
    match cfg.keys.get(cmd.name()) {
        Some(spec) => parse_key_spec(spec),
        None => parse_key_spec(cmd.default_spec()),
    }
}

/// Qué comando reasignable tiene ahora mismo `(vk, m)`, leyendo `[keys]` en vivo.
pub fn command_for_key(cfg: &notty_config::Config, vk: u32, m: Modifiers) -> Option<Command> {
    Command::ALL.iter().copied().find(|&c| binding(cfg, c) == Some((vk, m)))
}

/// Otro comando que ya usa `key` (el que no es `cmd`), para avisar antes de pisarlo.
pub fn conflict(cfg: &notty_config::Config, cmd: Command, key: (u32, Modifiers)) -> Option<Command> {
    Command::ALL.iter().copied().find(|&c| c != cmd && binding(cfg, c) == Some(key))
}

/// Fija el atajo de `cmd` en `[keys]`: `None` lo deja sin atajo, y si coincide con el
/// de fábrica se borra la entrada (config.toml solo guarda lo cambiado).
pub fn set_binding(cfg: &mut notty_config::Config, cmd: Command, key: Option<(u32, Modifiers)>) {
    let default = parse_key_spec(cmd.default_spec());
    if key == default {
        cfg.keys.remove(cmd.name());
        return;
    }
    let spec = key.and_then(|(vk, m)| format_key_spec(vk, m)).unwrap_or_default();
    cfg.keys.insert(cmd.name().to_string(), spec);
}

pub fn default_ui_keymap() -> HashMap<(u32, Modifiers), UiCommand> {
    Command::ALL
        .iter()
        .filter_map(|c| Some((parse_key_spec(c.default_spec())?, c.ui()?)))
        .collect()
}

/// Aplica los atajos de `[keys]` a los comandos de interfaz: cada comando con entrada
/// pierde su atajo de fábrica y toma el nuevo (o ninguno, si la entrada está vacía).
pub fn apply_overrides(map: &mut HashMap<(u32, Modifiers), UiCommand>, cfg: &notty_config::Config) {
    for &cmd in Command::ALL {
        let (Some(ui), Some(spec)) = (cmd.ui(), cfg.keys.get(cmd.name())) else { continue };
        map.retain(|_, v| *v != ui);
        if let Some(key) = parse_key_spec(spec) {
            map.insert(key, ui);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Modifiers;

    fn m(ctrl: bool, shift: bool, alt: bool) -> Modifiers {
        Modifiers { ctrl, shift, alt }
    }

    #[test]
    fn default_keymap_has_the_six_commands() {
        let map = default_ui_keymap();
        assert_eq!(map.len(), 6);
        assert_eq!(map.get(&(0xBC, m(true, false, false))), Some(&UiCommand::OpenSettings));
        assert_eq!(map.get(&(0x4E, m(true, false, false))), Some(&UiCommand::NewTab));
        assert_eq!(map.get(&(0x4E, m(true, true, false))), Some(&UiCommand::NewTempTab));
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

    #[test]
    fn apply_overrides_moves_and_unbinds() {
        let mut cfg = notty_config::Config::default();
        cfg.keys.insert("new_tab".into(), "Ctrl+T".into());
        cfg.keys.insert("close_tab".into(), String::new());
        let mut map = default_ui_keymap();
        apply_overrides(&mut map, &cfg);
        assert_eq!(map.get(&(0x54, m(true, false, false))), Some(&UiCommand::NewTab));
        assert_eq!(map.get(&(0x4E, m(true, false, false))), None);
        assert!(!map.values().any(|c| *c == UiCommand::CloseTab));
    }

    #[test]
    fn names_round_trip_and_are_unique() {
        for &c in Command::ALL {
            assert_eq!(Command::from_name(c.name()), Some(c));
            assert!(parse_key_spec(c.default_spec()).is_some(), "{c:?}");
        }
    }

    #[test]
    fn set_binding_stores_only_changes() {
        let mut cfg = notty_config::Config::default();
        set_binding(&mut cfg, Command::ToggleVim, parse_key_spec("Ctrl+Alt+M"));
        assert_eq!(cfg.keys.get("toggle_vim").map(String::as_str), Some("Ctrl+Alt+M"));
        assert_eq!(binding_spec(&cfg, Command::ToggleVim), "Ctrl+Alt+M");
        set_binding(&mut cfg, Command::ToggleVim, parse_key_spec("Ctrl+Alt+V"));
        assert!(cfg.keys.is_empty());
        set_binding(&mut cfg, Command::CloseTab, None);
        assert_eq!(binding(&cfg, Command::CloseTab), None);
        assert_eq!(binding_spec(&cfg, Command::CloseTab), "");
    }

    #[test]
    fn conflicts_and_lookup_use_live_bindings() {
        let mut cfg = notty_config::Config::default();
        let ctrl_w = parse_key_spec("Ctrl+W").unwrap();
        assert_eq!(conflict(&cfg, Command::NewTab, ctrl_w), Some(Command::CloseTab));
        assert_eq!(conflict(&cfg, Command::CloseTab, ctrl_w), None);
        set_binding(&mut cfg, Command::ToggleRaw, Some(ctrl_w));
        set_binding(&mut cfg, Command::CloseTab, None);
        assert_eq!(command_for_key(&cfg, ctrl_w.0, ctrl_w.1), Some(Command::ToggleRaw));
    }
}
