use std::collections::HashMap;

use crate::{Modifiers, parse_key_spec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiCommand {
    OpenSettings,
    NewTab,
    NewTempTab,
    NextTab,
    PrevTab,
    CloseTab,
}

pub fn default_ui_keymap() -> HashMap<(u32, Modifiers), UiCommand> {
    let bindings: &[(&str, UiCommand)] = &[
        ("Ctrl+,", UiCommand::OpenSettings),
        ("Ctrl+N", UiCommand::NewTab),
        ("Ctrl+Shift+N", UiCommand::NewTempTab),
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
}
