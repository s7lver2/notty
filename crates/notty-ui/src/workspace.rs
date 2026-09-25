use crate::EditorState;

/// Varios documentos abiertos a la vez. Siempre tiene al menos uno.
pub struct Workspace {
    docs: Vec<EditorState>,
    active: usize,
    pub prompt: crate::Prompt,
}

impl Workspace {
    pub fn new() -> Self {
        Self { docs: vec![EditorState::new_empty()], active: 0, prompt: crate::Prompt::None }
    }

    pub fn close_prompt(&mut self) {
        self.prompt = crate::Prompt::None;
    }

    pub fn open_conflict(&mut self) {
        self.prompt = crate::Prompt::Conflict;
    }

    /// Acceso simultáneo al prompt (mutable) y al documento activo (solo lectura):
    /// hace falta para que, por ejemplo, `SearchState::next` pueda mirar `Document`
    /// mientras avanza `current` dentro de `ws.prompt`, sin que el borrow checker se
    /// queje por pedir `&self` y `&mut self` del mismo `Workspace` a la vez.
    pub fn prompt_and_active(&mut self) -> (&mut crate::Prompt, &EditorState) {
        (&mut self.prompt, &self.docs[self.active])
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

    /// `EditorState::new_empty`/`from_opened` no conocen el tamaño de la ventana (se
    /// crean con `visible_lines: 1`): se hereda el de la pestaña activa, que sí lo
    /// tiene, para que el documento no se vea "de una línea" hasta el próximo resize.
    pub fn open(&mut self, mut state: EditorState) {
        state.viewport.visible_lines = self.active().viewport.visible_lines;
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
        self.close(self.active)
    }

    /// Igual que `close_active`, pero con un índice explícito (clic en la ✕ de una
    /// pestaña que no es la activa). Si `idx` cierra una pestaña anterior a la activa,
    /// `active` se desplaza para seguir señalando al mismo documento.
    pub fn close(&mut self, idx: usize) -> bool {
        if idx >= self.docs.len() {
            return false;
        }
        if self.docs.len() == 1 {
            let visible_lines = self.docs[0].viewport.visible_lines;
            self.docs[0] = EditorState::new_empty();
            self.docs[0].viewport.visible_lines = visible_lines;
            return false;
        }
        self.docs.remove(idx);
        if idx < self.active {
            self.active -= 1;
        }
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
    use crate::Prompt;

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

    /// `EditorState::new_empty` arranca con `visible_lines: 1` (no conoce el tamaño de
    /// la ventana); `open` debe heredar el de la pestaña activa, o el documento se ve
    /// "de una sola línea" hasta el próximo resize.
    #[test]
    fn open_inherits_visible_lines_from_the_active_doc() {
        let mut w = Workspace::new();
        w.active_mut().viewport.visible_lines = 30;
        w.open(EditorState::new_empty());
        assert_eq!(w.active().viewport.visible_lines, 30);
    }

    #[test]
    fn closing_the_last_doc_keeps_visible_lines() {
        let mut w = Workspace::new();
        w.active_mut().viewport.visible_lines = 30;
        w.close_active();
        assert_eq!(w.len(), 1);
        assert_eq!(w.active().viewport.visible_lines, 30);
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

    #[test]
    fn starts_without_a_prompt() {
        assert!(matches!(Workspace::new().prompt, Prompt::None));
    }

    #[test]
    fn close_by_index_before_active_shifts_active_left() {
        let mut w = Workspace::new();
        w.open(EditorState::new_empty());
        w.open(EditorState::new_empty());
        assert_eq!(w.active_index(), 2);
        assert!(w.close(0));
        assert_eq!(w.len(), 2);
        assert_eq!(w.active_index(), 1);
    }

    #[test]
    fn close_by_index_out_of_range_does_nothing() {
        let mut w = Workspace::new();
        w.open(EditorState::new_empty());
        assert!(!w.close(5));
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn close_prompt_clears_it() {
        let mut w = Workspace::new();
        w.prompt = Prompt::Find(crate::search_prompt::SearchState::default());
        w.close_prompt();
        assert!(matches!(w.prompt, Prompt::None));
    }

    #[test]
    fn open_conflict_sets_the_prompt() {
        let mut w = Workspace::new();
        w.open_conflict();
        assert!(matches!(w.prompt, crate::Prompt::Conflict));
    }
}
