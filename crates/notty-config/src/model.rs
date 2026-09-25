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
    pub vim_always: bool,
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
            vim_always: false,
        };
        let preset = ui.preset;
        crate::apply_preset(&mut ui, preset);
        ui
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ui: UiConfig,
}

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
        let _ = ui.line_numbers; // solo comprueba que existe el campo
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

    #[test]
    fn vim_always_defaults_to_false_and_round_trips() {
        assert!(!UiConfig::default().vim_always);
        let cfg = Config { ui: UiConfig { vim_always: true, ..UiConfig::default() } };
        let text = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert!(back.ui.vim_always);
    }
}
