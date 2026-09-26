use std::collections::BTreeMap;

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
    /// Paneles lado a lado estilo kitty (hasta 3), además de las pestañas.
    Splits,
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

/// Fuente monoespaciada del editor: cualquier familia instalada (Ajustes → Apariencia
/// → Fuentes las lista todas). `AUTO` es el comportamiento de siempre ("Cascadia
/// Mono"); si la familia elegida no está instalada, el renderer cae a Cascadia Mono.
///
/// El nombre se guarda internado (`&'static str`) para que `UiConfig` siga siendo
/// `Copy`: solo hay tantos nombres distintos como fuentes elija el usuario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FontFamily(Option<&'static str>);

impl FontFamily {
    pub const AUTO: FontFamily = FontFamily(None);

    pub fn named(name: &str) -> FontFamily {
        let name = name.trim();
        if name.is_empty() || name.eq_ignore_ascii_case("auto") {
            return FontFamily::AUTO;
        }
        // Valores que guardaba la versión con la lista fija de 6 fuentes.
        let legacy = match name.to_ascii_lowercase().as_str() {
            "consolas" => Some("Consolas"),
            "jetbrainsmono" => Some("JetBrains Mono"),
            "firacode" => Some("Fira Code"),
            "couriernew" => Some("Courier New"),
            "lucidaconsole" => Some("Lucida Console"),
            _ => None,
        };
        FontFamily(Some(legacy.unwrap_or_else(|| intern(name))))
    }

    pub fn is_auto(self) -> bool {
        self.0.is_none()
    }

    /// Familia a pedirle a DirectWrite.
    pub fn primary_name(self) -> &'static str {
        self.0.unwrap_or("Cascadia Mono")
    }
}

fn intern(name: &str) -> &'static str {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(n) = names.iter().find(|n| **n == name) {
        return n;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    names.push(leaked);
    leaked
}

impl Serialize for FontFamily {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.0.unwrap_or("auto"))
    }
}

impl<'de> Deserialize<'de> for FontFamily {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(FontFamily::named(&s))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
    /// Zoom del texto del editor (Ctrl+=/Ctrl+-/Ctrl+0), como multiplicador de
    /// `layout::FONT_MONO`/`LINE_H`. No afecta a la letra del resto de la interfaz
    /// (pestañas, barra de estado, Ajustes), solo al cuerpo del documento y la vista raw.
    pub font_scale: f32,
    /// Icono de carpeta/archivo delante de cada sugerencia de la línea de ruta.
    pub suggestion_icons: bool,
    /// Abrir/Guardar como usan el selector nativo de Windows en vez de la línea de
    /// ruta de abajo. `^O` dentro de la línea de ruta ya lo ofrece siempre como
    /// alternativa puntual; esto lo hace el camino por defecto.
    pub native_file_dialog: bool,
    pub font_family: FontFamily,
    /// Sustituye visualmente secuencias como "->" por una flecha, sin tocar el texto
    /// guardado. El conjunto fijo vive en `notty_ui::ligature::BUILTIN`; `Config::
    /// ligature_overrides` deja añadir o pisar entradas.
    pub ligatures: bool,
    /// Colorea el código (tree-sitter) en los archivos con extensión conocida.
    /// Encendido por defecto: en un `.txt` no cambia nada, y en código es lo que
    /// cualquiera espera ver al abrirlo.
    pub syntax_highlight: bool,
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
            font_scale: 1.0,
            suggestion_icons: true,
            native_file_dialog: false,
            font_family: FontFamily::AUTO,
            ligatures: false,
            syntax_highlight: true,
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

/// Si notty consulta (una vez al día, sin identificadores) si hay versión nueva
/// en GitHub. Apagado por defecto: la única forma de encenderlo son el paso
/// Privacidad de `welcome_window` o Ajustes → Acerca de.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatesConfig {
    pub check: bool,
    pub last_check: u64,
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self { check: false, last_check: 0 }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ui: UiConfig,
    pub files: FilesConfig,
    pub hotkey: HotkeyConfig,
    pub updates: UpdatesConfig,
    /// Si la ventana de bienvenida (`welcome_window`) ya se mostró una vez. No se
    /// repite sola nunca más; solo vuelve a verse desde Ajustes → Ayuda si el usuario
    /// lo pide expresamente (eso arranca el recorrido directamente, no esta ventana).
    #[serde(default)]
    pub first_run_done: bool,
    /// Sección `[keys]`: nombre de comando (`notty_input::Command::name`) → atajo
    /// ("Ctrl+Shift+N"). Solo guarda lo que el usuario cambió; una cadena vacía
    /// deja el comando sin atajo.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, String>,
    /// Sección `[ligature_overrides]`: secuencia → carácter de sustitución, para
    /// añadir a `notty_ui::ligature::BUILTIN` o pisar alguna de sus entradas. Solo
    /// tiene efecto si `ui.ligatures` está activo.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ligature_overrides: BTreeMap<String, String>,
    /// Secuencias (de serie o propias) apagadas una a una en Ajustes → Ligaduras.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ligature_disabled: Vec<String>,
    /// Lenguajes (ids de `notty_ui::syntax::LANGS`) que no se colorean aunque el
    /// resaltado esté activo: Ajustes → Apariencia → Sintaxis.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub syntax_disabled: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_config_defaults_to_check_disabled() {
        assert!(!UpdatesConfig::default().check);
        assert!(!Config::default().updates.check);
    }

    #[test]
    fn updates_check_round_trips_through_toml() {
        let cfg = Config { updates: UpdatesConfig { check: true, ..UpdatesConfig::default() }, ..Config::default() };
        let s = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert!(back.updates.check);
    }

    #[test]
    fn first_run_done_defaults_to_false() {
        assert!(!Config::default().first_run_done);
    }

    #[test]
    fn first_run_done_round_trips_through_toml() {
        let cfg = Config { first_run_done: true, ..Config::default() };
        let s = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert!(back.first_run_done);
    }

    #[test]
    fn keys_default_to_empty_and_round_trip() {
        assert!(Config::default().keys.is_empty());
        let mut cfg = Config::default();
        cfg.keys.insert("new_tab".to_string(), "Ctrl+T".to_string());
        cfg.keys.insert("close_tab".to_string(), String::new());
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert!(text.contains("[keys]"));
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.keys, cfg.keys);
    }

    #[test]
    fn empty_keys_are_not_written() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(!text.contains("[keys]"));
    }

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
    fn font_scale_defaults_to_one_and_round_trips() {
        assert_eq!(UiConfig::default().font_scale, 1.0);
        let cfg = Config { ui: UiConfig { font_scale: 1.3, ..UiConfig::default() }, ..Config::default() };
        let text = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.ui.font_scale, 1.3);
    }

    #[test]
    fn ligatures_default_off_and_overrides_round_trip() {
        assert!(!UiConfig::default().ligatures);
        let mut cfg = Config { ui: UiConfig { ligatures: true, ..UiConfig::default() }, ..Config::default() };
        cfg.ligature_overrides.insert("~>".to_string(), "↝".to_string());
        let text = toml::to_string(&cfg).unwrap();
        assert!(text.contains("[ligature_overrides]"));
        let back: Config = toml::from_str(&text).unwrap();
        assert!(back.ui.ligatures);
        assert_eq!(back.ligature_overrides.get("~>").map(String::as_str), Some("↝"));
    }

    #[test]
    fn empty_ligature_overrides_are_not_written() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(!text.contains("[ligature_overrides]"));
    }

    #[test]
    fn font_family_defaults_to_auto_and_round_trips() {
        assert_eq!(UiConfig::default().font_family, FontFamily::AUTO);
        let cfg = Config { ui: UiConfig { font_family: FontFamily::named("Iosevka Term"), ..UiConfig::default() }, ..Config::default() };
        let text = toml::to_string(&cfg).unwrap();
        assert!(text.contains("font_family = \"Iosevka Term\""));
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.ui.font_family.primary_name(), "Iosevka Term");
        let auto = toml::to_string(&Config::default()).unwrap();
        assert!(auto.contains("font_family = \"auto\""));
    }

    #[test]
    fn old_fixed_font_values_still_load() {
        for (old, name) in [("auto", "Cascadia Mono"), ("consolas", "Consolas"), ("jetbrainsmono", "JetBrains Mono"), ("firacode", "Fira Code"), ("couriernew", "Courier New"), ("lucidaconsole", "Lucida Console")] {
            let cfg: Config = toml::from_str(&format!("[ui]
font_family = \"{old}\"
")).unwrap();
            assert_eq!(cfg.ui.font_family.primary_name(), name, "{old}");
        }
    }

    #[test]
    fn same_font_name_is_the_same_value() {
        assert_eq!(FontFamily::named("Hack"), FontFamily::named("Hack"));
        assert_ne!(FontFamily::named("Hack"), FontFamily::AUTO);
    }

    #[test]
    fn disabled_ligatures_round_trip_and_are_omitted_when_empty() {
        assert!(!toml::to_string(&Config::default()).unwrap().contains("ligature_disabled"));
        let mut cfg = Config { ligature_disabled: vec!["->".to_string()], ..Config::default() };
        cfg.ligature_overrides.insert("|>".to_string(), "▷".to_string());
        let back: Config = toml::from_str(&toml::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(back.ligature_disabled, vec!["->".to_string()]);
    }

    #[test]
    fn syntax_disabled_round_trips_and_is_omitted_when_empty() {
        assert!(!toml::to_string(&Config::default()).unwrap().contains("syntax_disabled"));
        let cfg = Config { syntax_disabled: vec!["go".to_string()], ..Config::default() };
        let back: Config = toml::from_str(&toml::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(back.syntax_disabled, vec!["go".to_string()]);
    }

    #[test]
    fn syntax_highlight_defaults_to_on_and_round_trips() {
        assert!(UiConfig::default().syntax_highlight);
        let cfg = Config { ui: UiConfig { syntax_highlight: false, ..UiConfig::default() }, ..Config::default() };
        let text = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert!(!back.ui.syntax_highlight);
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
    fn updates_config_defaults_to_opt_out() {
        let cfg = Config::default();
        assert!(!cfg.updates.check);
        assert_eq!(cfg.updates.last_check, 0);
    }

    #[test]
    fn partial_toml_with_updates_section_parses() {
        let toml_str = "[updates]\ncheck = true\nlast_check = 1234567890\n";
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert!(cfg.updates.check);
        assert_eq!(cfg.updates.last_check, 1234567890);
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
