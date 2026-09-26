//! Menús contextuales (clic derecho, `Shift+F10`, tecla Menú): qué elementos lleva
//! cada uno según dónde se abre y el estado del documento. Puro: `render.rs` los
//! dibuja con el mismo aspecto que los desplegables de la barra de menús y
//! `window.rs` ejecuta el comando elegido.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtxCmd {
    NewTab,
    CloseTab(usize),
    CloseOthers(usize),
    CloseRight(usize),
    CopyPath(usize),
    OpenFolder(usize),
    SaveAs(usize),
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
    FindSelection,
    ReplaceSelection,
    FindNext,
    FindPrev,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CtxItem {
    Entry { label: String, shortcut: &'static str, cmd: CtxCmd, enabled: bool },
    Sep,
}

impl CtxItem {
    fn entry(label: impl Into<String>, shortcut: &'static str, cmd: CtxCmd, enabled: bool) -> Self {
        CtxItem::Entry { label: label.into(), shortcut, cmd, enabled }
    }

    pub fn is_enabled(&self) -> bool {
        matches!(self, CtxItem::Entry { enabled: true, .. })
    }
}

/// Menú contextual abierto: elementos y punto (DIPs) donde se pidió.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextMenu {
    pub items: Vec<CtxItem>,
    pub x: f32,
    pub y: f32,
}

impl ContextMenu {
    pub fn cmd(&self, j: usize) -> Option<CtxCmd> {
        match self.items.get(j) {
            Some(CtxItem::Entry { cmd, enabled: true, .. }) => Some(*cmd),
            _ => None,
        }
    }
}

/// Siguiente elemento seleccionable desde `from` en la dirección `dir` (±1), dando la
/// vuelta, saltándose separadores y elementos desactivados. `None` si no hay ninguno.
pub fn step_selection(enabled: &[bool], from: Option<usize>, dir: i32) -> Option<usize> {
    let n = enabled.len();
    if n == 0 || !enabled.iter().any(|&e| e) {
        return None;
    }
    let mut i = match from {
        Some(i) => i as i64,
        None if dir > 0 => -1,
        None => n as i64,
    };
    for _ in 0..n {
        i = (i + dir as i64).rem_euclid(n as i64);
        if enabled[i as usize] {
            return Some(i as usize);
        }
    }
    None
}

/// Texto corto de la selección para la etiqueta «Buscar «…»»: la primera línea,
/// recortada a unos pocos caracteres con «…».
pub fn short_label(sel: &str) -> String {
    const MAX: usize = 24;
    let line = sel.lines().next().unwrap_or("").trim();
    if line.chars().count() > MAX {
        let mut s: String = line.chars().take(MAX - 1).collect();
        s.push('…');
        s
    } else {
        line.to_string()
    }
}

pub fn tab_menu(idx: usize, tab_count: usize, has_path: bool) -> Vec<CtxItem> {
    vec![
        CtxItem::entry("Nueva pestaña", "^N", CtxCmd::NewTab, true),
        CtxItem::Sep,
        CtxItem::entry("Cerrar", "^W", CtxCmd::CloseTab(idx), true),
        CtxItem::entry("Cerrar las demás", "", CtxCmd::CloseOthers(idx), tab_count > 1),
        CtxItem::entry("Cerrar las de la derecha", "", CtxCmd::CloseRight(idx), idx + 1 < tab_count),
        CtxItem::Sep,
        CtxItem::entry("Copiar ruta", "", CtxCmd::CopyPath(idx), has_path),
        CtxItem::entry("Abrir carpeta contenedora", "", CtxCmd::OpenFolder(idx), has_path),
        CtxItem::Sep,
        CtxItem::entry("Guardar como…", "", CtxCmd::SaveAs(idx), true),
    ]
}

/// Estado del documento que decide qué se puede hacer desde el menú del texto.
#[derive(Debug, Clone, Default)]
pub struct BodyCtx {
    /// Texto seleccionado (vacío si solo hay cursor).
    pub selection: String,
    pub clipboard_has_text: bool,
    /// Si hay un prompt de buscar/reemplazar abierto (F3 tiene algo que repetir).
    pub search_open: bool,
}

pub fn body_menu(ctx: &BodyCtx) -> Vec<CtxItem> {
    let has_sel = !ctx.selection.is_empty();
    // Una selección de varias líneas no cabe en el campo de una línea del prompt.
    let searchable = has_sel && !ctx.selection.contains(['\n', '\r']);
    let (find_label, replace_label) = if searchable {
        let s = short_label(&ctx.selection);
        (format!("Buscar «{s}»"), format!("Reemplazar «{s}»"))
    } else {
        ("Buscar la selección".to_string(), "Reemplazar la selección".to_string())
    };
    vec![
        CtxItem::entry("Deshacer", "^Z", CtxCmd::Undo, true),
        CtxItem::entry("Rehacer", "^Y", CtxCmd::Redo, true),
        CtxItem::Sep,
        CtxItem::entry("Cortar", "^X", CtxCmd::Cut, has_sel),
        CtxItem::entry("Copiar", "^C", CtxCmd::Copy, has_sel),
        CtxItem::entry("Pegar", "^V", CtxCmd::Paste, ctx.clipboard_has_text),
        CtxItem::entry("Eliminar", "Supr", CtxCmd::Delete, has_sel),
        CtxItem::Sep,
        CtxItem::entry("Seleccionar todo", "^A", CtxCmd::SelectAll, true),
        CtxItem::Sep,
        CtxItem::entry(find_label, "^F", CtxCmd::FindSelection, searchable),
        CtxItem::entry(replace_label, "^H", CtxCmd::ReplaceSelection, searchable),
        CtxItem::entry("Buscar siguiente", "F3", CtxCmd::FindNext, ctx.search_open || searchable),
        CtxItem::entry("Buscar anterior", "⇧F3", CtxCmd::FindPrev, ctx.search_open || searchable),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled(items: &[CtxItem]) -> Vec<bool> {
        items.iter().map(CtxItem::is_enabled).collect()
    }

    fn find(items: &[CtxItem], cmd: CtxCmd) -> &CtxItem {
        items.iter().find(|i| matches!(i, CtxItem::Entry { cmd: c, .. } if *c == cmd)).unwrap()
    }

    #[test]
    fn tab_menu_disables_path_actions_without_a_path() {
        let m = tab_menu(0, 1, false);
        assert!(!find(&m, CtxCmd::CopyPath(0)).is_enabled());
        assert!(!find(&m, CtxCmd::OpenFolder(0)).is_enabled());
        assert!(!find(&m, CtxCmd::CloseOthers(0)).is_enabled());
        assert!(!find(&m, CtxCmd::CloseRight(0)).is_enabled());
        let m = tab_menu(0, 3, true);
        assert!(find(&m, CtxCmd::CopyPath(0)).is_enabled());
        assert!(find(&m, CtxCmd::CloseRight(0)).is_enabled());
        assert!(!find(&tab_menu(2, 3, true), CtxCmd::CloseRight(2)).is_enabled());
    }

    #[test]
    fn body_menu_needs_a_selection_to_cut_copy_and_search() {
        let m = body_menu(&BodyCtx::default());
        for cmd in [CtxCmd::Cut, CtxCmd::Copy, CtxCmd::Delete, CtxCmd::FindSelection, CtxCmd::FindNext, CtxCmd::Paste] {
            assert!(!find(&m, cmd).is_enabled(), "{cmd:?}");
        }
        let m = body_menu(&BodyCtx { selection: "hola".into(), clipboard_has_text: true, search_open: false });
        for cmd in [CtxCmd::Cut, CtxCmd::Copy, CtxCmd::Delete, CtxCmd::FindSelection, CtxCmd::ReplaceSelection, CtxCmd::Paste] {
            assert!(find(&m, cmd).is_enabled(), "{cmd:?}");
        }
        assert!(matches!(find(&m, CtxCmd::FindSelection), CtxItem::Entry { label, .. } if label == "Buscar «hola»"));
    }

    #[test]
    fn multiline_selection_can_be_copied_but_not_searched() {
        let m = body_menu(&BodyCtx { selection: "a\r\nb".into(), ..Default::default() });
        assert!(find(&m, CtxCmd::Copy).is_enabled());
        assert!(!find(&m, CtxCmd::FindSelection).is_enabled());
    }

    #[test]
    fn long_selection_label_is_shortened() {
        let s = short_label(&"x".repeat(100));
        assert_eq!(s.chars().count(), 24);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn step_selection_skips_disabled_and_wraps() {
        let m = body_menu(&BodyCtx::default());
        let e = enabled(&m);
        assert_eq!(step_selection(&e, None, 1), Some(0));
        assert_eq!(step_selection(&e, Some(1), 1), Some(8)); // salta separadores y desactivados
        assert_eq!(step_selection(&e, Some(0), -1), Some(8));
        assert_eq!(step_selection(&[false, false], None, 1), None);
    }

    #[test]
    fn cmd_ignores_separators_and_disabled_items() {
        let menu = ContextMenu { items: body_menu(&BodyCtx::default()), x: 0.0, y: 0.0 };
        assert_eq!(menu.cmd(0), Some(CtxCmd::Undo));
        assert_eq!(menu.cmd(2), None);
        assert_eq!(menu.cmd(3), None);
    }
}
