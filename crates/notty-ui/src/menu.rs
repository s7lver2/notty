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
    ToggleLineNumbers,
    ToggleHintsBar,
    NewTemp,
    /// Todavía no existen en la app: se dibujan pero no hacen nada (Plan 7+).
    Shortcuts,
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
];

const AYUDA: &[MenuItem] = &[
    MenuItem::Entry { label: "Atajos de teclado", shortcut: "", cmd: MenuCmd::Shortcuts },
    MenuItem::Entry { label: "Acerca de notty", shortcut: "", cmd: MenuCmd::About },
];

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
    fn every_menu_has_at_least_one_entry() {
        for m in MENUS {
            assert!(m.items.iter().any(|i| matches!(i, MenuItem::Entry { .. })));
        }
    }
}
