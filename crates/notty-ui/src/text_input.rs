//! Campo de texto de una línea para Ajustes (texto de muestra y buscador de
//! Fuentes, formulario de Ligaduras): cursor, selección con Shift, borrar. Sin IME.
//! Puro: la ventana le pasa las teclas y lo dibuja.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInput {
    pub text: String,
    /// Posición del cursor en caracteres.
    pub caret: usize,
    /// Otro extremo de la selección, si hay una.
    pub anchor: Option<usize>,
    /// Máximo de caracteres (0 = sin límite).
    pub max_chars: usize,
}

impl TextInput {
    pub fn new(text: &str, max_chars: usize) -> Self {
        Self { text: text.to_string(), caret: text.chars().count(), anchor: None, max_chars }
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte_at(&self, char_idx: usize) -> usize {
        self.text.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(self.text.len())
    }

    /// Selección como rango de caracteres, si no está vacía.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let a = self.anchor?;
        (a != self.caret).then(|| (a.min(self.caret), a.max(self.caret)))
    }

    fn delete_selection(&mut self) -> bool {
        let Some((a, b)) = self.selection() else {
            self.anchor = None;
            return false;
        };
        let (ba, bb) = (self.byte_at(a), self.byte_at(b));
        self.text.replace_range(ba..bb, "");
        self.caret = a;
        self.anchor = None;
        true
    }

    /// Escribe `s` en el cursor (sustituyendo la selección). Los saltos de línea y
    /// demás controles se descartan.
    pub fn insert(&mut self, s: &str) {
        self.delete_selection();
        let clean: String = s.chars().filter(|c| !c.is_control()).collect();
        let room = if self.max_chars == 0 { usize::MAX } else { self.max_chars.saturating_sub(self.len()) };
        let clean: String = clean.chars().take(room).collect();
        let at = self.byte_at(self.caret);
        self.text.insert_str(at, &clean);
        self.caret += clean.chars().count();
    }

    pub fn backspace(&mut self) {
        if self.delete_selection() || self.caret == 0 {
            return;
        }
        let (a, b) = (self.byte_at(self.caret - 1), self.byte_at(self.caret));
        self.text.replace_range(a..b, "");
        self.caret -= 1;
    }

    pub fn delete(&mut self) {
        if self.delete_selection() || self.caret >= self.len() {
            return;
        }
        let (a, b) = (self.byte_at(self.caret), self.byte_at(self.caret + 1));
        self.text.replace_range(a..b, "");
    }

    /// Mueve el cursor a `to`; con `extend` alarga la selección en vez de quitarla.
    pub fn move_to(&mut self, to: usize, extend: bool) {
        let to = to.min(self.len());
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = None;
        }
        self.caret = to;
    }

    pub fn left(&mut self, extend: bool) {
        if !extend {
            if let Some((a, _)) = self.selection() {
                self.move_to(a, false);
                return;
            }
        }
        self.move_to(self.caret.saturating_sub(1), extend);
    }

    pub fn right(&mut self, extend: bool) {
        if !extend {
            if let Some((_, b)) = self.selection() {
                self.move_to(b, false);
                return;
            }
        }
        self.move_to(self.caret + 1, extend);
    }

    pub fn home(&mut self, extend: bool) {
        self.move_to(0, extend);
    }

    pub fn end(&mut self, extend: bool) {
        self.move_to(self.len(), extend);
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.len();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.caret = 0;
        self.anchor = None;
    }

    /// El texto hasta `char_idx` (para medir dónde cae el cursor al dibujar).
    pub fn prefix(&self, char_idx: usize) -> &str {
        &self.text[..self.byte_at(char_idx)]
    }
}

/// Une las dos mitades de un par sustituto que `WM_CHAR` entrega por separado
/// (emoji y demás caracteres fuera del BMP). La mitad alta se guarda en `pending`.
pub fn char_from_utf16_unit(pending: &mut Option<u16>, unit: u16) -> Option<char> {
    match unit {
        0xD800..=0xDBFF => {
            *pending = Some(unit);
            None
        }
        0xDC00..=0xDFFF => {
            let hi = pending.take()?;
            char::decode_utf16([hi, unit]).next()?.ok()
        }
        _ => {
            *pending = None;
            char::from_u32(unit as u32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surrogate_pair_becomes_one_char() {
        let mut pending = None;
        assert_eq!(char_from_utf16_unit(&mut pending, 0xD83D), None);
        assert_eq!(char_from_utf16_unit(&mut pending, 0xDE00), Some('😀'));
        assert_eq!(pending, None);
        assert_eq!(char_from_utf16_unit(&mut pending, 'ñ' as u16), Some('ñ'));
        // Mitad baja suelta: se ignora.
        assert_eq!(char_from_utf16_unit(&mut pending, 0xDE00), None);
    }

    #[test]
    fn typing_and_backspace() {
        let mut t = TextInput::default();
        t.insert("hola");
        assert_eq!((t.text.as_str(), t.caret), ("hola", 4));
        t.backspace();
        assert_eq!(t.text, "hol");
        t.home(false);
        t.backspace();
        assert_eq!(t.text, "hol");
        t.delete();
        assert_eq!(t.text, "ol");
    }

    #[test]
    fn inserts_in_the_middle_with_multibyte_chars() {
        let mut t = TextInput::new("añb", 0);
        t.left(false);
        t.insert("→");
        assert_eq!(t.text, "añ→b");
        assert_eq!(t.caret, 3);
    }

    #[test]
    fn shift_selection_is_replaced_by_typing() {
        let mut t = TextInput::new("abcd", 0);
        t.left(true);
        t.left(true);
        assert_eq!(t.selection(), Some((2, 4)));
        t.insert("X");
        assert_eq!(t.text, "abX");
        t.select_all();
        t.backspace();
        assert!(t.text.is_empty());
    }

    #[test]
    fn arrows_collapse_a_selection_to_its_side() {
        let mut t = TextInput::new("abcd", 0);
        t.select_all();
        t.left(false);
        assert_eq!((t.caret, t.selection()), (0, None));
    }

    #[test]
    fn max_chars_and_control_chars() {
        let mut t = TextInput::new("", 3);
        t.insert("ab\ncde");
        assert_eq!(t.text, "abc");
        assert_eq!(t.prefix(2), "ab");
    }
}
