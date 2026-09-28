//! Qué pone en la barra de atajos y en la barra de estado, según lo que se esté haciendo.

use notty_core::Document;
use notty_io::{LineEnding, TextEncoding};

/// Contexto que decide la barra de atajos (el primero que aplique, en este orden).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintsCtx {
    PathOpen,
    PathSave,
    Find,
    Replace,
    Raw,
    MdPreviewReadOnly,
    MdPreviewInline,
    VimNormal,
    VimInsert,
    Normal,
}

/// Pares (tecla, acción). La tecla va en negrita `--text-hint`, la acción en `--text-2`.
fn hints_items_es(ctx: HintsCtx) -> &'static [(&'static str, &'static str)] {
    match ctx {
        HintsCtx::PathOpen => &[("Tab", "Completar"), ("Enter", "Abrir"), ("Esc", "Cancelar"), ("^O", "Diálogo de Windows")],
        HintsCtx::PathSave => &[("Tab", "Completar"), ("Enter", "Crear"), ("Esc", "Cancelar"), ("^O", "Diálogo de Windows")],
        HintsCtx::Find => &[("Enter", "Siguiente"), ("⇧Enter", "Anterior"), ("Alt+C", "Aa"), ("Alt+W", "Palabra"), ("Alt+R", "Regex"), ("Esc", "Cerrar")],
        HintsCtx::Replace => &[("Enter", "Reemplazar"), ("^Alt+Enter", "Todas"), ("Alt+C", "Aa"), ("Alt+W", "Palabra"), ("Alt+R", "Regex"), ("Esc", "Cerrar")],
        HintsCtx::Raw => &[("^S", "Guardar"), ("^Shift+H", "Ver como texto"), ("←→↑↓", "Moverse"), ("0-F", "Escribir byte")],
        HintsCtx::MdPreviewReadOnly => &[("^Shift+M", "Ver como texto"), ("←→↑↓", "Moverse")],
        HintsCtx::MdPreviewInline => &[("^S", "Guardar"), ("^Shift+M", "Salir de previsualización")],
        HintsCtx::VimNormal => &[("i", "Insertar"), ("hjkl", "Moverse"), ("dd", "Borrar línea"), ("/", "Buscar"), (":w", "Guardar"), ("^Alt+V", "Salir de vim")],
        HintsCtx::VimInsert => &[("Esc", "Modo normal"), ("^S", "Guardar")],
        HintsCtx::Normal => &[("^S", "Guardar"), ("^F", "Buscar"), ("^H", "Reemplazar"), ("^O", "Abrir"), ("^Alt+V", "Vim")],
    }
}

/// Como `hints_items_es`, pero con la acción traducida a `lang` (la tecla no cambia).
pub fn hints_items(ctx: HintsCtx, lang: notty_config::Lang) -> Vec<(&'static str, &'static str)> {
    hints_items_es(ctx).iter().map(|(k, a)| (*k, crate::strings::tr(lang, a))).collect()
}

/// Campos de la derecha de la barra de estado en modo texto: `Ln 1, Col 1`, codificación, EOL.
pub fn status_right(doc: &Document, encoding: TextEncoding, eol: LineEnding) -> [String; 3] {
    let (line, col) = doc.line_col();
    [format!("Ln {}, Col {}", line + 1, col + 1), encoding.label().to_string(), eol.label().to_string()]
}

/// Campo de la derecha en vista raw: desplazamiento del byte seleccionado, `0x0000000a`.
pub fn raw_offset(offset: usize) -> String {
    format!("0x{offset:08x}")
}

/// Texto de la izquierda en vista raw: `Raw · 384 B`.
pub fn raw_size(len: usize) -> String {
    format!("Raw · {len} B")
}

/// Nombre que se enseña de un documento (pestaña y título): el nombre del archivo, o
/// `sin ruta` si todavía no tiene (lo que la maqueta llama `CLICKME`).
pub fn doc_name(path: Option<&std::path::Path>) -> String {
    doc_name_lang(path, notty_config::Lang::Es)
}

/// Como `doc_name`, traducido a `lang`.
pub fn doc_name_lang(path: Option<&std::path::Path>, lang: notty_config::Lang) -> String {
    path.and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| crate::strings::tr(lang, "sin ruta").to_string())
}

/// Título de la ventana (barra de tareas, y barra de título cuando no hay pestañas en ella):
/// `notas.txt · notty`, con ` •` tras el nombre si hay cambios sin guardar.
pub fn window_title(name: &str, dirty: bool, zen: bool) -> String {
    let dot = if dirty { " •" } else { "" };
    if zen { format!("{name}{dot}") } else { format!("{name}{dot} · notty") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_hints_match_mockup() {
        assert_eq!(hints_items(HintsCtx::Normal, notty_config::Lang::Es)[0], ("^S", "Guardar"));
        assert_eq!(hints_items(HintsCtx::Normal, notty_config::Lang::Es).len(), 5);
    }

    #[test]
    fn path_hints_depend_on_purpose() {
        assert_eq!(hints_items(HintsCtx::PathOpen, notty_config::Lang::Es)[1].1, "Abrir");
        assert_eq!(hints_items(HintsCtx::PathSave, notty_config::Lang::Es)[1].1, "Crear");
    }

    #[test]
    fn hints_translate_to_english() {
        assert_eq!(hints_items(HintsCtx::Normal, notty_config::Lang::En)[0], ("^S", "Save"));
    }

    #[test]
    fn status_right_is_three_separate_fields() {
        let mut doc = Document::new("hola\nmundo", "\n");
        doc.set_cursor(7);
        assert_eq!(status_right(&doc, TextEncoding::Utf8, LineEnding::Lf), ["Ln 2, Col 3".to_string(), "UTF-8".into(), "LF".into()]);
    }

    #[test]
    fn raw_labels() {
        assert_eq!(raw_offset(10), "0x0000000a");
        assert_eq!(raw_size(384), "Raw · 384 B");
    }

    #[test]
    fn names_and_titles() {
        assert_eq!(doc_name(None), "sin ruta");
        assert_eq!(doc_name(Some(std::path::Path::new(r"C:\a\notas.txt"))), "notas.txt");
        assert_eq!(window_title("notas.txt", true, false), "notas.txt • · notty");
        assert_eq!(window_title("notas.txt", false, true), "notas.txt");
    }
}
