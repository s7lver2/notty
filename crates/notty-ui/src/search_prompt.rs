use std::ops::Range;

use notty_core::{Document, SearchError, SearchOptions};

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub replacement: String,
    pub opts: SearchOptions,
    pub current: usize,
    /// Solo tiene sentido en el prompt de reemplazar: si `true`, lo tecleado va al
    /// campo «por» (`replacement`) en vez de al de búsqueda (`query`). `Tab` o un
    /// clic en `Hit::SearchField` lo alternan.
    pub editing_replacement: bool,
}

impl SearchState {
    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.current = 0;
    }

    /// Añade `ch` al campo activo (`replacement` si `editing_replacement`, si no
    /// `query`). El contador de coincidencias solo se resetea cuando cambia `query`.
    pub fn type_char(&mut self, ch: char) {
        if self.editing_replacement {
            self.replacement.push(ch);
        } else {
            let mut q = self.query.clone();
            q.push(ch);
            self.set_query(q);
        }
    }

    /// Borra el último char del campo activo.
    pub fn backspace(&mut self) {
        if self.editing_replacement {
            self.replacement.pop();
        } else {
            let mut q = self.query.clone();
            q.pop();
            self.set_query(q);
        }
    }

    pub fn toggle_field(&mut self) {
        self.editing_replacement = !self.editing_replacement;
    }

    pub fn matches(&self, doc: &Document) -> Result<Vec<Range<usize>>, SearchError> {
        if self.query.is_empty() {
            return Ok(Vec::new());
        }
        doc.find_all(&self.query, self.opts)
    }

    pub fn count_label(&self, doc: &Document) -> String {
        if self.query.is_empty() {
            return String::new();
        }
        match self.matches(doc) {
            Err(_) => "regex ✕".to_string(),
            Ok(m) if m.is_empty() => "0/0".to_string(),
            Ok(m) => format!("{}/{}", (self.current + 1).min(m.len()), m.len()),
        }
    }

    pub fn next(&mut self, doc: &Document) {
        if let Ok(m) = self.matches(doc) {
            if !m.is_empty() {
                self.current = (self.current + 1) % m.len();
            }
        }
    }

    pub fn prev(&mut self, doc: &Document) {
        if let Ok(m) = self.matches(doc) {
            if !m.is_empty() {
                self.current = (self.current + m.len() - 1) % m.len();
            }
        }
    }

    pub fn toggle_case(&mut self) {
        self.opts.case_sensitive = !self.opts.case_sensitive;
        self.current = 0;
    }

    pub fn toggle_word(&mut self) {
        self.opts.whole_word = !self.opts.whole_word;
        self.current = 0;
    }

    pub fn toggle_regex(&mut self) {
        self.opts.regex = !self.opts.regex;
        self.current = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notty_core::Document;

    fn doc() -> Document {
        Document::new("Juan y juan y JUAN", "\n")
    }

    #[test]
    fn empty_query_has_empty_label() {
        let s = SearchState::default();
        assert_eq!(s.count_label(&doc()), "");
    }

    #[test]
    fn default_search_is_case_insensitive() {
        let mut s = SearchState::default();
        s.set_query("juan".into());
        assert_eq!(s.matches(&doc()).unwrap().len(), 3);
        assert_eq!(s.count_label(&doc()), "1/3");
    }

    #[test]
    fn no_matches_shows_zero_of_zero() {
        let mut s = SearchState::default();
        s.set_query("nada-de-esto".into());
        assert_eq!(s.count_label(&doc()), "0/0");
    }

    #[test]
    fn bad_regex_shows_error_label() {
        let mut s = SearchState::default();
        s.opts.regex = true;
        s.set_query("(".into());
        assert_eq!(s.count_label(&doc()), "regex ✕");
    }

    #[test]
    fn next_and_prev_wrap_and_update_label() {
        let mut s = SearchState::default();
        s.set_query("juan".into());
        s.next(&doc());
        assert_eq!(s.count_label(&doc()), "2/3");
        s.next(&doc());
        s.next(&doc());
        assert_eq!(s.count_label(&doc()), "1/3");
        s.prev(&doc());
        assert_eq!(s.count_label(&doc()), "3/3");
    }

    #[test]
    fn toggle_case_narrows_matches_and_resets_current() {
        let mut s = SearchState::default();
        s.set_query("juan".into());
        s.next(&doc());
        s.toggle_case();
        assert_eq!(s.matches(&doc()).unwrap().len(), 1);
        assert_eq!(s.current, 0);
    }

    #[test]
    fn typing_goes_to_query_by_default() {
        let mut s = SearchState::default();
        s.type_char('a');
        s.type_char('b');
        assert_eq!(s.query, "ab");
        assert_eq!(s.replacement, "");
    }

    #[test]
    fn toggle_field_routes_typing_to_replacement() {
        let mut s = SearchState::default();
        s.toggle_field();
        s.type_char('x');
        assert_eq!(s.query, "");
        assert_eq!(s.replacement, "x");
    }

    #[test]
    fn backspace_removes_from_the_active_field() {
        let mut s = SearchState::default();
        s.set_query("abc".into());
        s.backspace();
        assert_eq!(s.query, "ab");
        s.toggle_field();
        s.replacement = "xyz".into();
        s.backspace();
        assert_eq!(s.replacement, "xy");
    }

    #[test]
    fn toggle_word_and_regex_flip_flags() {
        let mut s = SearchState::default();
        assert!(!s.opts.whole_word);
        s.toggle_word();
        assert!(s.opts.whole_word);
        assert!(!s.opts.regex);
        s.toggle_regex();
        assert!(s.opts.regex);
    }
}
