//! Contenido de los menús desplegables (`MENUS` de la maqueta, línea 496): qué pone
//! cada elemento y a qué comando corresponde. Puro: no sabe dibujar ni ejecutar nada.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCmd {
    New,
    Open,
    Save,
    SaveAs,
    Settings,
    CloseTab,
    Undo,
    Redo,
    Find,
    Replace,
    FindNext,
    FindPrev,
    ToggleVim,
    ToggleRaw,
    ToggleMdPreview,
    ToggleLineNumbers,
    ToggleHintsBar,
    NewTemp,
    /// Todavía no existen en la app: se dibujan pero no hacen nada (Plan 7+).
    Shortcuts,
    WhatsNew,
    About,
}

#[derive(Debug, Clone, Copy)]
pub enum MenuItem {
    Entry { label: &'static str, shortcut: &'static str, cmd: MenuCmd },
    Sep,
}

#[derive(Debug, Clone, Copy)]
pub struct MenuDef {
    pub name: &'static str,
    pub items: &'static [MenuItem],
}

const ARCHIVO: &[MenuItem] = &[
    MenuItem::Entry { label: "Nuevo", shortcut: "^N", cmd: MenuCmd::New },
    MenuItem::Entry { label: "Nuevo temporal", shortcut: "^⇧N", cmd: MenuCmd::NewTemp },
    MenuItem::Entry { label: "Abrir", shortcut: "^O", cmd: MenuCmd::Open },
    MenuItem::Entry { label: "Guardar", shortcut: "^S", cmd: MenuCmd::Save },
    MenuItem::Entry { label: "Guardar como", shortcut: "", cmd: MenuCmd::SaveAs },
    MenuItem::Sep,
    MenuItem::Entry { label: "Ajustes", shortcut: "^,", cmd: MenuCmd::Settings },
    MenuItem::Sep,
    MenuItem::Entry { label: "Cerrar pestaña", shortcut: "^W", cmd: MenuCmd::CloseTab },
];

const EDITAR: &[MenuItem] = &[
    MenuItem::Entry { label: "Deshacer", shortcut: "^Z", cmd: MenuCmd::Undo },
    MenuItem::Entry { label: "Rehacer", shortcut: "^Y", cmd: MenuCmd::Redo },
];

const BUSCAR: &[MenuItem] = &[
    MenuItem::Entry { label: "Buscar", shortcut: "^F", cmd: MenuCmd::Find },
    MenuItem::Entry { label: "Reemplazar", shortcut: "^H", cmd: MenuCmd::Replace },
    MenuItem::Entry { label: "Siguiente", shortcut: "F3", cmd: MenuCmd::FindNext },
    MenuItem::Entry { label: "Anterior", shortcut: "⇧F3", cmd: MenuCmd::FindPrev },
];

const VER: &[MenuItem] = &[
    MenuItem::Entry { label: "Números de línea", shortcut: "", cmd: MenuCmd::ToggleLineNumbers },
    MenuItem::Entry { label: "Barra de atajos", shortcut: "", cmd: MenuCmd::ToggleHintsBar },
    MenuItem::Sep,
    MenuItem::Entry { label: "Modo vim", shortcut: "^Alt+V", cmd: MenuCmd::ToggleVim },
    MenuItem::Entry { label: "Ver como raw", shortcut: "^⇧H", cmd: MenuCmd::ToggleRaw },
    MenuItem::Entry { label: "Previsualización de Markdown", shortcut: "^⇧M", cmd: MenuCmd::ToggleMdPreview },
];

const AYUDA: &[MenuItem] = &[
    MenuItem::Entry { label: "Atajos de teclado", shortcut: "", cmd: MenuCmd::Shortcuts },
    MenuItem::Entry { label: "Novedades", shortcut: "", cmd: MenuCmd::WhatsNew },
    MenuItem::Entry { label: "Acerca de notty", shortcut: "", cmd: MenuCmd::About },
];

impl MenuCmd {
    /// El comando reasignable (Ajustes → Teclado, `[keys]`) que hace lo mismo, si hay.
    pub fn remappable(self) -> Option<notty_input::Command> {
        use notty_input::Command;
        match self {
            MenuCmd::New => Some(Command::NewTab),
            MenuCmd::NewTemp => Some(Command::NewTempTab),
            MenuCmd::Settings => Some(Command::OpenSettings),
            MenuCmd::CloseTab => Some(Command::CloseTab),
            MenuCmd::ToggleVim => Some(Command::ToggleVim),
            MenuCmd::ToggleRaw => Some(Command::ToggleRaw),
            MenuCmd::ToggleMdPreview => Some(Command::ToggleMdPreview),
            _ => None,
        }
    }
}

/// "Ctrl+Shift+N" → "^⇧N", el formato corto de los atajos en los menús.
pub fn compact_spec(spec: &str) -> String {
    spec.replace("Ctrl+", "^").replace("Shift+", "⇧")
}

/// Atajo en vigor de cada elemento de menú reasignable, leído de `cfg` (vacío si el
/// usuario lo dejó sin atajo).
pub fn shortcut_labels(cfg: &notty_config::Config) -> Vec<(MenuCmd, String)> {
    MENUS
        .iter()
        .flat_map(|m| m.items.iter())
        .filter_map(|it| match it {
            MenuItem::Entry { cmd, .. } => cmd.remappable().map(|c| (*cmd, compact_spec(&notty_input::binding_spec(cfg, c)))),
            MenuItem::Sep => None,
        })
        .collect()
}

/// Estado actual de las opciones que se encienden/apagan desde los menús, leído al
/// dibujar para marcar con ✓ las activas.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MenuChecks {
    pub line_numbers: bool,
    pub hints_bar: bool,
    pub vim: bool,
    pub raw: bool,
    pub md_preview: bool,
}

impl MenuChecks {
    /// `Some(activa)` si `cmd` es una opción de encender/apagar, `None` si es una acción.
    pub fn state_of(&self, cmd: MenuCmd) -> Option<bool> {
        match cmd {
            MenuCmd::ToggleLineNumbers => Some(self.line_numbers),
            MenuCmd::ToggleHintsBar => Some(self.hints_bar),
            MenuCmd::ToggleVim => Some(self.vim),
            MenuCmd::ToggleRaw => Some(self.raw),
            MenuCmd::ToggleMdPreview => Some(self.md_preview),
            _ => None,
        }
    }
}

pub const MENUS: &[MenuDef] = &[
    MenuDef { name: "Archivo", items: ARCHIVO },
    MenuDef { name: "Editar", items: EDITAR },
    MenuDef { name: "Buscar", items: BUSCAR },
    MenuDef { name: "Ver", items: VER },
    MenuDef { name: "Ayuda", items: AYUDA },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_menus_matching_the_menubar() {
        let names: Vec<&str> = MENUS.iter().map(|m| m.name).collect();
        assert_eq!(names, ["Archivo", "Editar", "Buscar", "Ver", "Ayuda"]);
    }

    #[test]
    fn archivo_starts_with_nuevo_and_ctrl_n() {
        assert!(matches!(
            ARCHIVO[0],
            MenuItem::Entry { label: "Nuevo", shortcut: "^N", cmd: MenuCmd::New }
        ));
    }

    #[test]
    fn shortcut_labels_follow_remaps() {
        let mut cfg = notty_config::Config::default();
        let labels = shortcut_labels(&cfg);
        assert!(labels.contains(&(MenuCmd::New, "^N".to_string())));
        assert!(labels.contains(&(MenuCmd::ToggleVim, "^Alt+V".to_string())));
        cfg.keys.insert("new_tab".to_string(), "Ctrl+T".to_string());
        assert!(shortcut_labels(&cfg).contains(&(MenuCmd::New, "^T".to_string())));
    }

    #[test]
    fn toggles_report_their_state_and_actions_do_not() {
        let c = MenuChecks { line_numbers: true, hints_bar: false, vim: true, raw: false, md_preview: false };
        assert_eq!(c.state_of(MenuCmd::ToggleLineNumbers), Some(true));
        assert_eq!(c.state_of(MenuCmd::ToggleHintsBar), Some(false));
        assert_eq!(c.state_of(MenuCmd::ToggleVim), Some(true));
        assert_eq!(c.state_of(MenuCmd::Save), None);
    }

    #[test]
    fn every_menu_has_at_least_one_entry() {
        for m in MENUS {
            assert!(m.items.iter().any(|i| matches!(i, MenuItem::Entry { .. })));
        }
    }
}
