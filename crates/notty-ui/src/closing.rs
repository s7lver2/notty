//! Cerrar documentos (o la ventana) con cambios sin guardar: a quién hay que
//! preguntar, qué se vuelca a recuperación y qué se cierra al final. Puro: la
//! ventana hace el guardado y el cierre de verdad.

use notty_config::{OnCloseUnsaved, TempMode};

use crate::EditorState;

/// Respuesta a "tiene cambios sin guardar".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseChoice {
    Save,
    Discard,
    Cancel,
}

impl CloseChoice {
    /// G guarda, N no guarda, C/Esc cancela. Cualquier otra tecla no responde.
    pub fn from_vk(vk: u32) -> Option<Self> {
        match vk {
            0x47 => Some(Self::Save),
            0x4E => Some(Self::Discard),
            0x43 | 0x1B => Some(Self::Cancel),
            _ => None,
        }
    }
}

/// Cierre en curso. Los documentos van por `EditorState::id`, no por índice: cerrar
/// o cambiar de pestaña a mitad de la pregunta no hace que la respuesta caiga en otro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseRequest {
    /// Todos los documentos a cerrar.
    pub targets: Vec<u64>,
    /// Los que faltan por preguntar; el primero es el de la pregunta en pantalla.
    pub ask: Vec<u64>,
    /// Los que se vuelcan a recuperación (modo "Recuperar") justo antes de cerrar.
    pub recover: Vec<u64>,
    /// Cerrar la ventana entera al terminar.
    pub quit: bool,
    /// Nombre del documento de la pregunta en pantalla.
    pub name: String,
}

impl CloseRequest {
    pub fn new<'a>(docs: impl IntoIterator<Item = &'a EditorState>, mode: OnCloseUnsaved, quit: bool) -> Self {
        let mut req = Self { targets: Vec::new(), ask: Vec::new(), recover: Vec::new(), quit, name: String::new() };
        for st in docs {
            req.targets.push(st.id);
            if !has_unsaved(st) {
                continue;
            }
            // Los bytes de la vista raw no caben en un volcado de texto: esos se preguntan siempre.
            let raw_dirty = st.raw.as_ref().is_some_and(|r| r.is_dirty());
            if mode == OnCloseUnsaved::Recuperar && !raw_dirty {
                req.recover.push(st.id);
            } else {
                req.ask.push(st.id);
            }
        }
        req
    }
}

/// Si cerrar `st` perdería algo. Los temporales volátiles no cuentan (su gracia es
/// desaparecer al cerrar), ni un documento sin ruta que se ha quedado vacío.
pub fn has_unsaved(st: &EditorState) -> bool {
    if st.temp == Some(TempMode::Volatile) {
        return false;
    }
    if st.raw.as_ref().is_some_and(|r| r.is_dirty()) {
        return true;
    }
    st.doc.is_dirty() && !(st.path.is_none() && st.doc.buffer().len_chars() == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn dirty(text: &str) -> EditorState {
        let mut st = EditorState::new_empty();
        st.doc.insert(text, Instant::now());
        st
    }

    #[test]
    fn keys_map_to_choices() {
        assert_eq!(CloseChoice::from_vk(0x47), Some(CloseChoice::Save));
        assert_eq!(CloseChoice::from_vk(0x4E), Some(CloseChoice::Discard));
        assert_eq!(CloseChoice::from_vk(0x1B), Some(CloseChoice::Cancel));
        assert_eq!(CloseChoice::from_vk(0x0D), None);
    }

    #[test]
    fn clean_empty_and_volatile_docs_need_nothing() {
        let clean = EditorState::new_empty();
        let mut emptied = dirty("x");
        emptied.doc.backspace(Instant::now());
        let mut volatile = dirty("x");
        volatile.temp = Some(TempMode::Volatile);
        assert!(!has_unsaved(&clean));
        assert!(!has_unsaved(&emptied));
        assert!(!has_unsaved(&volatile));
        assert!(has_unsaved(&dirty("x")));
    }

    #[test]
    fn ask_mode_asks_only_for_unsaved_docs() {
        let docs = [EditorState::new_empty(), dirty("a"), dirty("b")];
        let req = CloseRequest::new(&docs, OnCloseUnsaved::Preguntar, true);
        assert_eq!(req.targets, docs.iter().map(|d| d.id).collect::<Vec<_>>());
        assert_eq!(req.ask, vec![docs[1].id, docs[2].id]);
        assert!(req.recover.is_empty());
        assert!(req.quit);
    }

    #[test]
    fn recover_mode_recovers_instead_of_asking() {
        let docs = [dirty("a"), EditorState::new_empty()];
        let req = CloseRequest::new(&docs, OnCloseUnsaved::Recuperar, false);
        assert!(req.ask.is_empty());
        assert_eq!(req.recover, vec![docs[0].id]);
    }

    #[test]
    fn every_doc_gets_its_own_id() {
        assert_ne!(EditorState::new_empty().id, EditorState::new_empty().id);
    }
}
