//! Pregunta "el archivo cambió en disco": se responde escribiendo una palabra entera
//! y Enter (SI / NO), nunca con una sola tecla, para que una pulsación suelta
//! mientras se escribe no recargue ni sobrescriba nada.

use std::time::{Duration, Instant};

/// Tras abrirse la pregunta, las teclas de este rato se ignoran: casi seguro son el
/// final de lo que se estaba escribiendo en el documento.
const GRACE: Duration = Duration::from_millis(700);
const MAX_INPUT: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictAnswer {
    /// Guardar lo de notty encima del archivo.
    KeepMine,
    /// Cargar lo del disco (se descartan los cambios de notty).
    UseDisk,
    /// Ni SI ni NO.
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictState {
    /// `EditorState::id` del documento en conflicto.
    pub doc: u64,
    pub name: String,
    pub input: String,
    /// Se pulsó Enter con algo que no era SI ni NO.
    pub invalid: bool,
    opened: Instant,
}

impl ConflictState {
    pub fn new(doc: u64, name: String, now: Instant) -> Self {
        Self { doc, name, input: String::new(), invalid: false, opened: now }
    }

    pub fn accepts_input(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.opened) >= GRACE
    }

    pub fn type_char(&mut self, ch: char, now: Instant) {
        if self.accepts_input(now) && !ch.is_control() && self.input.chars().count() < MAX_INPUT {
            self.input.push(ch);
            self.invalid = false;
        }
    }

    pub fn backspace(&mut self) {
        self.input.pop();
        self.invalid = false;
    }

    pub fn answer(&self) -> ConflictAnswer {
        match self.input.trim().to_uppercase().as_str() {
            "SI" | "SÍ" | "YES" => ConflictAnswer::KeepMine,
            "NO" => ConflictAnswer::UseDisk,
            _ => ConflictAnswer::Invalid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> ConflictState {
        let t0 = Instant::now();
        let mut c = ConflictState::new(1, "a.txt".into(), t0);
        for ch in s.chars() {
            c.type_char(ch, t0 + GRACE);
        }
        c
    }

    #[test]
    fn whole_words_answer_case_insensitively() {
        for s in ["si", "SI", "Sí", "yes", " YES "] {
            assert_eq!(typed(s).answer(), ConflictAnswer::KeepMine, "{s}");
        }
        assert_eq!(typed("no").answer(), ConflictAnswer::UseDisk);
    }

    #[test]
    fn single_letters_and_other_words_do_nothing() {
        for s in ["", "m", "d", "s", "n", "nop", "sii"] {
            assert_eq!(typed(s).answer(), ConflictAnswer::Invalid, "{s}");
        }
    }

    #[test]
    fn keys_right_after_opening_are_ignored() {
        let t0 = Instant::now();
        let mut c = ConflictState::new(1, "a.txt".into(), t0);
        c.type_char('n', t0);
        c.type_char('o', t0 + Duration::from_millis(100));
        assert_eq!(c.input, "");
        c.type_char('n', t0 + GRACE);
        assert_eq!(c.input, "n");
    }
}
