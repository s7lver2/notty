use crate::EditorState;

/// Varios documentos abiertos a la vez. Siempre tiene al menos uno.
pub struct Workspace {
    docs: Vec<EditorState>,
    active: usize,
}

impl Workspace {
    pub fn new() -> Self {
        Self { docs: vec![EditorState::new_empty()], active: 0 }
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// Siempre `false`: un `Workspace` nunca se queda sin documentos.
    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> &EditorState {
        &self.docs[self.active]
    }

    pub fn active_mut(&mut self) -> &mut EditorState {
        &mut self.docs[self.active]
    }

    pub fn iter(&self) -> impl Iterator<Item = &EditorState> {
        self.docs.iter()
    }

    pub fn open(&mut self, state: EditorState) {
        self.docs.push(state);
        self.active = self.docs.len() - 1;
    }

    pub fn activate(&mut self, idx: usize) {
        self.active = idx.min(self.docs.len() - 1);
    }

    pub fn next(&mut self) {
        self.active = (self.active + 1) % self.docs.len();
    }

    pub fn prev(&mut self) {
        self.active = (self.active + self.docs.len() - 1) % self.docs.len();
    }

    /// Cierra la pestaña activa. Devuelve `true` si de verdad quedó una lista más
    /// corta; si era la última, la sustituye por un documento vacío y devuelve `false`.
    pub fn close_active(&mut self) -> bool {
        if self.docs.len() == 1 {
            self.docs[0] = EditorState::new_empty();
            return false;
        }
        self.docs.remove(self.active);
        self.active = self.active.min(self.docs.len() - 1);
        true
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_one_empty_doc() {
        let w = Workspace::new();
        assert_eq!(w.len(), 1);
        assert_eq!(w.active_index(), 0);
    }

    #[test]
    fn open_adds_and_activates() {
        let mut w = Workspace::new();
        w.open(EditorState::new_empty());
        assert_eq!(w.len(), 2);
        assert_eq!(w.active_index(), 1);
    }

    #[test]
    fn next_and_prev_wrap_around() {
        let mut w = Workspace::new();
        w.open(EditorState::new_empty());
        w.open(EditorState::new_empty());
        w.activate(0);
        w.prev();
        assert_eq!(w.active_index(), 2);
        w.next();
        assert_eq!(w.active_index(), 0);
    }

    #[test]
    fn close_active_removes_it_and_keeps_a_valid_index() {
        let mut w = Workspace::new();
        w.open(EditorState::new_empty());
        w.activate(0);
        assert!(w.close_active());
        assert_eq!(w.len(), 1);
        assert_eq!(w.active_index(), 0);
    }

    #[test]
    fn closing_the_last_doc_replaces_it_instead_of_leaving_zero() {
        let mut w = Workspace::new();
        assert!(!w.close_active());
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn activate_clamps_out_of_range() {
        let mut w = Workspace::new();
        w.activate(99);
        assert_eq!(w.active_index(), 0);
    }
}
