use std::ops::Range;
use std::time::Instant;

use crate::{Buffer, Edit, History};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn caret(idx: usize) -> Self {
        Self { anchor: idx, head: idx }
    }

    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// Un archivo abierto en memoria: texto, selección e historial.
#[derive(Debug)]
pub struct Document {
    buffer: Buffer,
    history: History,
    sel: Selection,
    newline: &'static str,
    saved: Option<u64>,
}

impl Document {
    pub fn new(text: &str, newline: &'static str) -> Self {
        Self { buffer: Buffer::new(text), history: History::default(), sel: Selection::default(), newline, saved: None }
    }

    pub fn text(&self) -> String {
        self.buffer.to_string()
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn selection(&self) -> Selection {
        self.sel
    }

    pub fn set_selection(&mut self, anchor: usize, head: usize) {
        let n = self.buffer.len_chars();
        self.sel = Selection { anchor: anchor.min(n), head: head.min(n) };
        self.history.seal();
    }

    pub fn set_cursor(&mut self, idx: usize) {
        self.set_selection(idx, idx);
    }

    pub fn line_col(&self) -> (usize, usize) {
        self.buffer.line_col(self.sel.head)
    }

    pub fn newline(&self) -> &'static str {
        self.newline
    }

    pub fn set_newline(&mut self, newline: &'static str) {
        self.newline = newline;
    }

    pub fn is_dirty(&self) -> bool {
        self.history.top_id() != self.saved
    }

    pub fn mark_saved(&mut self) {
        self.saved = self.history.top_id();
        self.history.seal();
    }

    pub fn insert(&mut self, text: &str, now: Instant) {
        let range = self.sel.range();
        self.replace_range(range, text, now);
    }

    pub fn insert_newline(&mut self, now: Instant) {
        let nl = self.newline;
        self.insert(nl, now);
    }

    pub fn replace_range(&mut self, range: Range<usize>, text: &str, now: Instant) {
        let removed = self.buffer.slice(range.clone());
        if removed.is_empty() && text.is_empty() {
            return;
        }
        let edit = Edit { at: range.start, removed, inserted: text.to_string() };
        edit.apply(&mut self.buffer);
        self.sel = Selection::caret(range.start + text.chars().count());
        self.history.record(edit, now);
    }

    pub fn backspace(&mut self, now: Instant) {
        if !self.sel.is_empty() {
            let range = self.sel.range();
            return self.replace_range(range, "", now);
        }
        let c = self.sel.head;
        if c == 0 {
            return;
        }
        let start = if c >= 2 && self.buffer.slice(c - 2..c) == "\r\n" { c - 2 } else { c - 1 };
        self.replace_range(start..c, "", now);
    }

    pub fn delete_forward(&mut self, now: Instant) {
        if !self.sel.is_empty() {
            let range = self.sel.range();
            return self.replace_range(range, "", now);
        }
        let (c, n) = (self.sel.head, self.buffer.len_chars());
        if c >= n {
            return;
        }
        let end = if c + 2 <= n && self.buffer.slice(c..c + 2) == "\r\n" { c + 2 } else { c + 1 };
        self.replace_range(c..end, "", now);
    }

    pub fn undo(&mut self) -> bool {
        match self.history.undo(&mut self.buffer) {
            Some(c) => {
                self.sel = Selection::caret(c);
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.history.redo(&mut self.buffer) {
            Some(c) => {
                self.sel = Selection::caret(c);
                true
            }
            None => false,
        }
    }
}

impl Document {
    /// Coincidencias como rangos de **chars**, listos para seleccionar o resaltar.
    pub fn find_all(&self, query: &str, opts: crate::SearchOptions) -> Result<Vec<Range<usize>>, crate::SearchError> {
        let text = self.text();
        Ok(crate::find_all(&text, query, opts)?
            .into_iter()
            .map(|r| self.buffer.byte_to_char(r.start)..self.buffer.byte_to_char(r.end))
            .collect())
    }

    /// Reemplaza todas las coincidencias como un único paso de deshacer.
    pub fn replace_all(&mut self, query: &str, replacement: &str, opts: crate::SearchOptions, now: Instant) -> Result<usize, crate::SearchError> {
        let text = self.text();
        let (new_text, count) = crate::replace_all(&text, query, replacement, opts)?;
        if count > 0 {
            self.history.seal();
            let len = self.buffer.len_chars();
            self.replace_range(0..len, &new_text, now);
            self.history.seal();
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn typing_then_undo_restores_text() {
        let mut d = Document::new("", "\r\n");
        let t = Instant::now();
        for (i, c) in "hola".chars().enumerate() {
            d.insert(&c.to_string(), t + Duration::from_millis(i as u64 * 100));
        }
        assert_eq!(d.text(), "hola");
        assert!(d.undo());
        assert_eq!(d.text(), "");
        assert_eq!(d.selection(), Selection::caret(0));
    }

    #[test]
    fn insert_replaces_selection() {
        let mut d = Document::new("hola mundo", "\r\n");
        d.set_selection(5, 10);
        d.insert("notty", Instant::now());
        assert_eq!(d.text(), "hola notty");
        assert_eq!(d.selection(), Selection::caret(10));
    }

    #[test]
    fn newline_uses_document_line_ending() {
        let mut d = Document::new("ab", "\r\n");
        d.set_cursor(1);
        d.insert_newline(Instant::now());
        assert_eq!(d.text(), "a\r\nb");
        assert_eq!(d.selection(), Selection::caret(3));
    }

    #[test]
    fn backspace_removes_whole_crlf() {
        let mut d = Document::new("a\r\nb", "\r\n");
        d.set_cursor(3);
        d.backspace(Instant::now());
        assert_eq!(d.text(), "ab");
        assert_eq!(d.selection(), Selection::caret(1));
    }

    #[test]
    fn delete_forward_removes_whole_crlf() {
        let mut d = Document::new("a\r\nb", "\r\n");
        d.set_cursor(1);
        d.delete_forward(Instant::now());
        assert_eq!(d.text(), "ab");
    }

    #[test]
    fn backspace_at_start_does_nothing() {
        let mut d = Document::new("a", "\n");
        d.set_cursor(0);
        d.backspace(Instant::now());
        assert_eq!(d.text(), "a");
        assert!(!d.is_dirty());
    }

    #[test]
    fn dirty_tracks_saved_point() {
        let mut d = Document::new("", "\n");
        let t = Instant::now();
        assert!(!d.is_dirty());
        d.insert("a", t);
        assert!(d.is_dirty());
        d.mark_saved();
        assert!(!d.is_dirty());
        d.insert("b", t);
        assert!(d.is_dirty());
        d.undo();
        assert!(!d.is_dirty());
        d.redo();
        assert!(d.is_dirty());
    }

    #[test]
    fn cursor_is_clamped() {
        let mut d = Document::new("ab", "\n");
        d.set_cursor(99);
        assert_eq!(d.selection(), Selection::caret(2));
        assert_eq!(d.line_col(), (0, 2));
    }

    #[test]
    fn find_all_returns_char_ranges() {
        let d = Document::new("ñandú ñ", "\n");
        assert_eq!(d.find_all("ñ", crate::SearchOptions::default()).unwrap(), vec![0..1, 6..7]);
    }

    #[test]
    fn replace_all_is_one_undo_step() {
        let mut d = Document::new("a b a", "\n");
        let n = d.replace_all("a", "x", crate::SearchOptions::default(), Instant::now()).unwrap();
        assert_eq!(n, 2);
        assert_eq!(d.text(), "x b x");
        d.undo();
        assert_eq!(d.text(), "a b a");
    }
}
