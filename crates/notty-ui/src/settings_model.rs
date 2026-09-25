//! Modelo puro de la ventana de Ajustes (`SECTIONS` de la maqueta, líneas 993-1027):
//! qué secciones y filas hay, y qué le pasa a `Config` cuando se toca una. No sabe
//! dibujar ni de Win32/Direct2D.

use notty_config::{Config, Files, MenuBar, Preset, TabsPosition, Theme};

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

pub const SECTIONS: &[Section] = &[
    Section { id: "apariencia", name: "Apariencia", rows: APARIENCIA },
    Section { id: "ventana", name: "Ventana", rows: VENTANA },
    Section { id: "teclado", name: "Teclado", rows: TECLADO },
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
    fn three_sections_matching_config_today() {
        let names: Vec<&str> = SECTIONS.iter().map(|s| s.name).collect();
        assert_eq!(names, ["Apariencia", "Ventana", "Teclado"]);
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
}
