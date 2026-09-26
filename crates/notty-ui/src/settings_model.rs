//! Modelo puro de la ventana de Ajustes (`SECTIONS` de la maqueta, líneas 993-1027):
//! qué secciones y filas hay, y qué le pasa a `Config` cuando se toca una. No sabe
//! dibujar ni de Win32/Direct2D.

use notty_config::{Config, Files, HotkeyMechanism, MenuBar, Preset, TabsPosition, TempMode, Theme};

/// Qué ajuste toca una fila interactiva (`Seg`/`Select`/`Toggle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKey {
    Preset,
    Theme,
    LineNumbers,
    Files,
    TabsPosition,
    MenuBar,
    HintsBar,
    StatusBar,
    MergedCommandLine,
    VimAlways,
    TempMode,
    Autosave,
    HotkeyMechanism,
    StartWithWindows,
    UpdatesCheck,
}

/// El valor elegido; `apply` decide qué campo de `Config` toca según `SettingKey`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingValue {
    Preset(Preset),
    Theme(Theme),
    Files(Files),
    TabsPosition(TabsPosition),
    MenuBar(MenuBar),
    Bool(bool),
    TempMode(TempMode),
    HotkeyMechanism(HotkeyMechanism),
}

#[derive(Debug, Clone, Copy)]
pub enum Row {
    /// Control segmentado (varias opciones, una marcada): `.seg` de la maqueta.
    Seg { title: &'static str, desc: &'static str, key: SettingKey, options: &'static [(&'static str, SettingValue)] },
    /// Desplegable de una sola opción a la vez: `.select`.
    Select { title: &'static str, desc: &'static str, key: SettingKey, options: &'static [(&'static str, SettingValue)] },
    Toggle { title: &'static str, desc: &'static str, key: SettingKey },
    /// Atajo de solo lectura (no se puede reasignar desde aquí): `.kbd`.
    Kbd { title: &'static str, keys: &'static str },
    /// Enlace de acción, no de ajuste: `.link`.
    Link { title: &'static str, desc: &'static str, label: &'static str },
    /// Cabecera de subgrupo dentro de una sección: `.sgroup`.
    Group(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct Section {
    pub id: &'static str,
    pub name: &'static str,
    pub rows: &'static [Row],
}

const APARIENCIA: &[Row] = &[
    Row::Seg {
        title: "Preset",
        desc: "Punto de partida. Cambiar cualquier pieza lo convierte en Personalizado.",
        key: SettingKey::Preset,
        options: &[
            ("Moderna", SettingValue::Preset(Preset::Moderna)),
            ("Clásica", SettingValue::Preset(Preset::Clasica)),
            ("Zen", SettingValue::Preset(Preset::Zen)),
        ],
    },
    Row::Seg {
        title: "Tema",
        desc: "Por defecto sigue al de Windows.",
        key: SettingKey::Theme,
        options: &[
            ("Sistema", SettingValue::Theme(Theme::System)),
            ("Claro", SettingValue::Theme(Theme::Light)),
            ("Oscuro", SettingValue::Theme(Theme::Dark)),
        ],
    },
    Row::Toggle { title: "Números de línea", desc: "", key: SettingKey::LineNumbers },
];

const VENTANA: &[Row] = &[
    Row::Seg {
        title: "Varios archivos",
        desc: "Pestañas visibles o buffers estilo vim (Ctrl+Tab, :b).",
        key: SettingKey::Files,
        options: &[("Pestañas", SettingValue::Files(Files::Tabs)), ("Buffers", SettingValue::Files(Files::Buffers))],
    },
    Row::Select {
        title: "Posición de las pestañas",
        desc: "",
        key: SettingKey::TabsPosition,
        options: &[
            ("En la barra de título", SettingValue::TabsPosition(TabsPosition::Title)),
            ("Bajo el menú", SettingValue::TabsPosition(TabsPosition::Below)),
            ("Solo si hay más de 1", SettingValue::TabsPosition(TabsPosition::Auto)),
            ("Ocultas", SettingValue::TabsPosition(TabsPosition::Hidden)),
        ],
    },
    Row::Select {
        title: "Barra de menús",
        desc: "",
        key: SettingKey::MenuBar,
        options: &[
            ("Oculta", SettingValue::MenuBar(MenuBar::Hidden)),
            ("Visible", SettingValue::MenuBar(MenuBar::Visible)),
            ("Aparece con Alt", SettingValue::MenuBar(MenuBar::Alt)),
        ],
    },
    Row::Toggle { title: "Barra de atajos", desc: "Estilo nano. Cambia según lo que estés haciendo.", key: SettingKey::HintsBar },
    Row::Toggle {
        title: "Barra de estado",
        desc: "Si la ocultas, reaparece para rutas, búsquedas y avisos.",
        key: SettingKey::StatusBar,
    },
    Row::Toggle {
        title: "Línea de comandos fusionada",
        desc: "Una sola línea abajo para estado y comandos, como en Zen.",
        key: SettingKey::MergedCommandLine,
    },
];

const TECLADO: &[Row] = &[
    Row::Toggle { title: "Modo vim siempre", desc: "Cada ventana arranca en modo vim.", key: SettingKey::VimAlways },
    Row::Kbd { title: "Alternar vim en esta ventana", keys: "Ctrl+Alt+V" },
    Row::Kbd { title: "Ver como raw", keys: "Ctrl+Shift+H" },
    Row::Link {
        title: "Todos los atajos",
        desc: "Cada acción es un comando con nombre. Reasigna cualquiera en config.toml, sección [keys].",
        label: "Abrir [keys]",
    },
];

const ARCHIVOS: &[Row] = &[
    Row::Seg {
        title: "Archivos temporales",
        desc: "Borrador: se guarda solo y se borra al cerrar si no le das ruta. Volátil: nunca toca el disco.",
        key: SettingKey::TempMode,
        options: &[
            ("Borrador", SettingValue::TempMode(TempMode::Draft)),
            ("Volátil", SettingValue::TempMode(TempMode::Volatile)),
        ],
    },
    Row::Toggle {
        title: "Autoguardado",
        desc: "Guarda sola tras dejar de escribir. Solo si el archivo ya tiene ruta.",
        key: SettingKey::Autosave,
    },
];

const ATAJO_GLOBAL: &[Row] = &[
    Row::Seg {
        title: "Cómo se escucha el atajo",
        desc: "Segundo plano: ~1 MB de RAM, cualquier combinación, instantáneo. Acceso directo: nada residente, solo Ctrl+Alt+letra.",
        key: SettingKey::HotkeyMechanism,
        options: &[
            ("Segundo plano", SettingValue::HotkeyMechanism(HotkeyMechanism::Daemon)),
            ("Acceso directo", SettingValue::HotkeyMechanism(HotkeyMechanism::Lnk)),
        ],
    },
    Row::Kbd { title: "Nuevo temporal", keys: "Win+Alt+N" },
    Row::Kbd { title: "Nuevo permanente", keys: "Win+Alt+Shift+N" },
    Row::Toggle { title: "Iniciar con Windows", desc: "", key: SettingKey::StartWithWindows },
];

const ACERCA_DE: &[Row] = &[
    Row::Toggle {
        title: "Buscar actualizaciones",
        desc: "Comprueba una vez al día contra GitHub Releases. Nunca se activa solo.",
        key: SettingKey::UpdatesCheck,
    },
    Row::Link {
        title: "Comprobar ahora",
        desc: "Fuerza el chequeo ya mismo, sin esperar al de una vez al día.",
        label: "Buscar ahora",
    },
];

pub const SECTIONS: &[Section] = &[
    Section { id: "apariencia", name: "Apariencia", rows: APARIENCIA },
    Section { id: "ventana", name: "Ventana", rows: VENTANA },
    Section { id: "teclado", name: "Teclado", rows: TECLADO },
    Section { id: "archivos", name: "Archivos", rows: ARCHIVOS },
    Section { id: "atajo_global", name: "Atajo global", rows: ATAJO_GLOBAL },
    Section { id: "acerca_de", name: "Acerca de", rows: ACERCA_DE },
];

/// Las secciones son estáticas (no dependen de `cfg`): el parámetro está para que la
/// firma cuadre con la maqueta y por si una sección futura sí dependiera de `Config`.
pub fn sections(_cfg: &Config) -> &'static [Section] {
    SECTIONS
}

/// Aplica el ajuste `key`/`value` a `cfg`, como `setSetting` de la maqueta (línea 1054):
/// elegir un preset aplica sus piezas fijas; tocar cualquier otra pieza deja el preset
/// en `Custom`.
pub fn apply(cfg: &mut Config, key: SettingKey, value: SettingValue) {
    if let (SettingKey::Preset, SettingValue::Preset(p)) = (key, value) {
        notty_config::apply_preset(&mut cfg.ui, p);
        return;
    }
    // Archivos/Atajo global no son piezas de un preset de Apariencia: se resuelven
    // aparte y no tocan `cfg.ui.preset`.
    match (key, value) {
        (SettingKey::TempMode, SettingValue::TempMode(m)) => {
            cfg.files.temp_mode = m;
            return;
        }
        (SettingKey::Autosave, SettingValue::Bool(b)) => {
            cfg.files.autosave = b;
            return;
        }
        (SettingKey::HotkeyMechanism, SettingValue::HotkeyMechanism(m)) => {
            cfg.hotkey.mechanism = m;
            return;
        }
        (SettingKey::StartWithWindows, SettingValue::Bool(b)) => {
            cfg.hotkey.start_with_windows = b;
            return;
        }
        (SettingKey::UpdatesCheck, SettingValue::Bool(b)) => {
            cfg.updates.check = b;
            return;
        }
        _ => {}
    }
    match (key, value) {
        (SettingKey::Theme, SettingValue::Theme(t)) => cfg.ui.theme = t,
        (SettingKey::LineNumbers, SettingValue::Bool(b)) => cfg.ui.line_numbers = b,
        (SettingKey::Files, SettingValue::Files(f)) => cfg.ui.files = f,
        (SettingKey::TabsPosition, SettingValue::TabsPosition(t)) => cfg.ui.tabs_position = t,
        (SettingKey::MenuBar, SettingValue::MenuBar(m)) => cfg.ui.menubar = m,
        (SettingKey::HintsBar, SettingValue::Bool(b)) => cfg.ui.hints_bar = b,
        (SettingKey::StatusBar, SettingValue::Bool(b)) => cfg.ui.status_bar = b,
        (SettingKey::MergedCommandLine, SettingValue::Bool(b)) => cfg.ui.merged_command_line = b,
        (SettingKey::VimAlways, SettingValue::Bool(b)) => cfg.ui.vim_always = b,
        _ => return, // combinación key/value que no tiene sentido: no hace nada
    }
    cfg.ui.preset = Preset::Custom;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archivos_and_atajo_global_sections_exist() {
        let names: Vec<&str> = SECTIONS.iter().map(|s| s.name).collect();
        assert_eq!(names, ["Apariencia", "Ventana", "Teclado", "Archivos", "Atajo global", "Acerca de"]);
    }

    #[test]
    fn applying_updates_check_touches_updates_not_ui() {
        let mut cfg = Config::default();
        let preset_before = cfg.ui.preset;
        apply(&mut cfg, SettingKey::UpdatesCheck, SettingValue::Bool(true));
        assert!(cfg.updates.check);
        assert_eq!(cfg.ui.preset, preset_before);
    }

    #[test]
    fn apariencia_has_preset_theme_and_line_numbers() {
        assert_eq!(APARIENCIA.len(), 3);
        assert!(matches!(APARIENCIA[0], Row::Seg { key: SettingKey::Preset, .. }));
        assert!(matches!(APARIENCIA[2], Row::Toggle { key: SettingKey::LineNumbers, .. }));
    }

    #[test]
    fn ventana_has_six_rows() {
        assert_eq!(VENTANA.len(), 6);
    }

    #[test]
    fn teclado_ends_with_the_shortcuts_link() {
        assert!(matches!(TECLADO.last(), Some(Row::Link { label: "Abrir [keys]", .. })));
    }

    #[test]
    fn applying_a_preset_sets_its_fixed_pieces_and_keeps_the_preset_itself() {
        let mut cfg = Config::default();
        apply(&mut cfg, SettingKey::Preset, SettingValue::Preset(Preset::Zen));
        assert_eq!(cfg.ui.preset, Preset::Zen);
        assert!(!cfg.ui.line_numbers);
        assert!(cfg.ui.merged_command_line);
    }

    #[test]
    fn touching_a_loose_piece_marks_the_preset_as_custom() {
        let mut cfg = Config::default();
        assert_eq!(cfg.ui.preset, Preset::Moderna);
        apply(&mut cfg, SettingKey::TabsPosition, SettingValue::TabsPosition(TabsPosition::Below));
        assert_eq!(cfg.ui.tabs_position, TabsPosition::Below);
        assert_eq!(cfg.ui.preset, Preset::Custom);
    }

    #[test]
    fn toggle_applies_the_boolean_and_marks_custom() {
        let mut cfg = Config::default();
        apply(&mut cfg, SettingKey::HintsBar, SettingValue::Bool(false));
        assert!(!cfg.ui.hints_bar);
        assert_eq!(cfg.ui.preset, Preset::Custom);
    }

    #[test]
    fn mismatched_key_and_value_does_nothing() {
        let mut cfg = Config::default();
        let before = cfg.clone();
        apply(&mut cfg, SettingKey::Theme, SettingValue::Bool(true));
        assert_eq!(cfg, before);
    }

    #[test]
    fn applying_temp_mode_and_autosave_touches_files_not_ui_preset() {
        let mut cfg = Config::default();
        let preset_before = cfg.ui.preset;
        apply(&mut cfg, SettingKey::TempMode, SettingValue::TempMode(notty_config::TempMode::Volatile));
        apply(&mut cfg, SettingKey::Autosave, SettingValue::Bool(true));
        assert_eq!(cfg.files.temp_mode, notty_config::TempMode::Volatile);
        assert!(cfg.files.autosave);
        // Archivos/Atajo global no son piezas de un preset de Apariencia: no lo tocan.
        assert_eq!(cfg.ui.preset, preset_before);
    }

    #[test]
    fn applying_hotkey_settings_touches_hotkey_not_ui() {
        let mut cfg = Config::default();
        apply(&mut cfg, SettingKey::HotkeyMechanism, SettingValue::HotkeyMechanism(notty_config::HotkeyMechanism::Lnk));
        apply(&mut cfg, SettingKey::StartWithWindows, SettingValue::Bool(false));
        assert_eq!(cfg.hotkey.mechanism, notty_config::HotkeyMechanism::Lnk);
        assert!(!cfg.hotkey.start_with_windows);
    }
}
