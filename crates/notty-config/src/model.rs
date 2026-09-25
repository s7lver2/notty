use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    #[default]
    Moderna,
    Clasica,
    Zen,
    /// Se llega aquí tocando una pieza suelta (p.ej. la posición de las pestañas)
    /// sin que coincida con ninguno de los tres presets fijos; como `setSetting`
    /// en la maqueta (línea 1054). No se elige nunca a mano: `apply_preset` no lo
    /// contempla como entrada.
    #[serde(rename = "custom")]
    Custom,
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
    pub merged_command_line: bool,
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
            merged_command_line: false,
        };
        let preset = ui.preset;
        crate::apply_preset(&mut ui, preset);
        ui
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ui: UiConfig,
    pub files: FilesConfig,
    pub hotkey: HotkeyConfig,
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
    fn custom_preset_round_trips_as_the_string_custom() {
        let mut ui = UiConfig { preset: Preset::Custom, ..UiConfig::default() };
        ui.tabs_position = TabsPosition::Below;
        let text = toml::to_string(&ui).unwrap();
        assert!(text.contains("preset = \"custom\""));
        let back: UiConfig = toml::from_str(&text).unwrap();
        assert_eq!(back.preset, Preset::Custom);
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
        let cfg = Config { ui: UiConfig { vim_always: true, ..UiConfig::default() }, ..Config::default() };
        let text = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert!(back.ui.vim_always);
    }

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
        // toml no permite serializar un enum "pelado" como documento de nivel
        // superior (tiene que ser una tabla), así que se pasa por `toml::Value`.
        assert_eq!(toml::Value::try_from(TempMode::Draft).unwrap().as_str(), Some("borrador"));
        assert_eq!(toml::Value::try_from(TempMode::Volatile).unwrap().as_str(), Some("volatil"));
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
}
