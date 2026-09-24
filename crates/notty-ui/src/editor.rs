use std::path::PathBuf;

use notty_core::Document;
use notty_io::{LineEnding, TextEncoding};

use crate::{OpenedDoc, Viewport};

/// Todo lo que necesita una ventana de notty para saber qué mostrar y qué guardar.
pub struct EditorState {
    pub doc: Document,
    pub viewport: Viewport,
    pub encoding: TextEncoding,
    pub eol: LineEnding,
    pub path: Option<PathBuf>,
}

impl EditorState {
    pub fn new_empty() -> Self {
        Self {
            doc: Document::new("", LineEnding::Crlf.as_str()),
            viewport: Viewport { first_line: 0, visible_lines: 1 },
            encoding: TextEncoding::Utf8,
            eol: LineEnding::Crlf,
            path: None,
        }
    }

    pub fn from_opened(opened: OpenedDoc) -> Self {
        Self {
            doc: opened.document,
            viewport: Viewport { first_line: 0, visible_lines: 1 },
            encoding: opened.encoding,
            eol: opened.eol,
            path: Some(opened.path),
        }
    }
}

impl EditorState {
    pub fn apply(&mut self, action: crate::EditorAction, now: std::time::Instant) {
        use crate::EditorAction::*;
        let buf_len = self.doc.buffer().len_chars();
        let cur = self.doc.selection();
        let (anchor, head) = (cur.anchor, cur.head);
        let (head_line, _) = self.doc.buffer().line_col(head);
        let home = self.doc.buffer().line_start(head_line);
        let end = self.line_end(head);

        match action {
            MoveLeft => self.doc.set_cursor(head.saturating_sub(1)),
            ExtendLeft => self.doc.set_selection(anchor, head.saturating_sub(1)),
            MoveRight => self.doc.set_cursor((head + 1).min(buf_len)),
            ExtendRight => self.doc.set_selection(anchor, (head + 1).min(buf_len)),
            MoveUp => self.vertical(-1, false),
            ExtendUp => self.vertical(-1, true),
            MoveDown => self.vertical(1, false),
            ExtendDown => self.vertical(1, true),
            MoveHome => self.doc.set_cursor(home),
            ExtendHome => self.doc.set_selection(anchor, home),
            MoveEnd => self.doc.set_cursor(end),
            ExtendEnd => self.doc.set_selection(anchor, end),
            MoveDocStart => self.doc.set_cursor(0),
            MoveDocEnd => self.doc.set_cursor(buf_len),
            SelectAll => self.doc.set_selection(0, buf_len),
            Backspace => self.doc.backspace(now),
            DeleteForward => self.doc.delete_forward(now),
            InsertNewline => self.doc.insert_newline(now),
            Undo => {
                self.doc.undo();
            }
            Redo => {
                self.doc.redo();
            }
            Copy | Cut | Paste | Save | Find | None => {} // se resuelven en window.rs (Tasks 7-8) o en planes posteriores
        }

        let (line, _) = self.doc.buffer().line_col(self.doc.selection().head);
        self.viewport.scroll_to_include(line, self.doc.buffer().len_lines());
    }

    /// Inserta un carácter tecleado. Ignora control chars salvo los que ya
    /// llegan como `EditorAction` (Enter, Backspace, Delete).
    pub fn insert_char(&mut self, ch: char, now: std::time::Instant) {
        if ch.is_control() {
            return;
        }
        self.doc.insert(&ch.to_string(), now);
        let (line, _) = self.doc.buffer().line_col(self.doc.selection().head);
        self.viewport.scroll_to_include(line, self.doc.buffer().len_lines());
    }

    fn line_end(&self, idx: usize) -> usize {
        let buf = self.doc.buffer();
        let (line, _) = buf.line_col(idx);
        let total = buf.len_lines();
        let end = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
        let text = buf.slice(buf.line_start(line)..end);
        buf.line_start(line) + text.trim_end_matches(['\r', '\n']).chars().count()
    }

    pub fn save(&mut self) -> Result<(), notty_io::CodecError> {
        let path = self.path.clone().ok_or_else(|| notty_io::CodecError::Io("sin ruta".to_string()))?;
        crate::save_document(&self.doc, &path, self.encoding)?;
        self.doc.mark_saved();
        Ok(())
    }

    pub fn scroll_by(&mut self, delta_lines: i32) {
        let total = self.doc.buffer().len_lines();
        let max_first = total.saturating_sub(1) as i64;
        let new_first = (self.viewport.first_line as i64 + delta_lines as i64).clamp(0, max_first);
        self.viewport.first_line = new_first as usize;
    }

    fn vertical(&mut self, delta: i32, extend: bool) {
        let buf = self.doc.buffer();
        let (line, col) = buf.line_col(self.doc.selection().head);
        let target_line = (line as i64 + delta as i64).clamp(0, buf.len_lines() as i64 - 1) as usize;
        let target_start = buf.line_start(target_line);
        let target_end = self.line_end(target_start);
        let idx = (target_start + col).min(target_end);
        if extend {
            self.doc.set_selection(self.doc.selection().anchor, idx);
        } else {
            self.doc.set_cursor(idx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditorAction::*;
    use std::time::Instant;

    fn state(text: &str) -> EditorState {
        let mut s = EditorState::new_empty();
        s.doc = Document::new(text, "\n");
        s.viewport = Viewport { first_line: 0, visible_lines: 3 };
        s
    }

    #[test]
    fn move_right_advances_caret() {
        let mut s = state("abc");
        s.apply(MoveRight, Instant::now());
        assert_eq!(s.doc.selection(), notty_core::Selection::caret(1));
    }

    #[test]
    fn move_left_at_start_stays_at_zero() {
        let mut s = state("abc");
        s.apply(MoveLeft, Instant::now());
        assert_eq!(s.doc.selection(), notty_core::Selection::caret(0));
    }

    #[test]
    fn extend_right_grows_selection_from_anchor() {
        let mut s = state("abcdef");
        s.apply(ExtendRight, Instant::now());
        s.apply(ExtendRight, Instant::now());
        assert_eq!(s.doc.selection(), notty_core::Selection { anchor: 0, head: 2 });
    }

    #[test]
    fn move_after_extend_collapses_to_caret() {
        let mut s = state("abcdef");
        s.apply(ExtendRight, Instant::now());
        s.apply(ExtendRight, Instant::now());
        s.apply(MoveRight, Instant::now());
        assert_eq!(s.doc.selection(), notty_core::Selection::caret(3));
    }

    #[test]
    fn move_down_scrolls_viewport() {
        let mut s = state("a\nb\nc\nd\ne\n");
        for _ in 0..4 {
            s.apply(MoveDown, Instant::now());
        }
        assert!(s.viewport.range(s.doc.buffer().len_lines()).contains(&4));
    }

    #[test]
    fn select_all_selects_whole_document() {
        let mut s = state("abc");
        s.apply(SelectAll, Instant::now());
        assert_eq!(s.doc.selection(), notty_core::Selection { anchor: 0, head: 3 });
    }

    #[test]
    fn backspace_removes_previous_char() {
        let mut s = state("abc");
        s.apply(MoveDocEnd, Instant::now());
        s.apply(Backspace, Instant::now());
        assert_eq!(s.doc.text(), "ab");
    }

    #[test]
    fn insert_char_types_at_caret() {
        let mut s = state("ac");
        s.apply(MoveRight, Instant::now());
        s.insert_char('b', Instant::now());
        assert_eq!(s.doc.text(), "abc");
        assert_eq!(s.doc.selection(), notty_core::Selection::caret(2));
    }

    #[test]
    fn insert_char_ignores_control_chars() {
        let mut s = state("a");
        s.insert_char('\r', Instant::now());
        s.insert_char('\u{7f}', Instant::now());
        assert_eq!(s.doc.text(), "a");
    }

    #[test]
    fn undo_redo_go_through_document() {
        let mut s = state("");
        s.insert_char('a', Instant::now());
        s.apply(Undo, Instant::now());
        assert_eq!(s.doc.text(), "");
        s.apply(Redo, Instant::now());
        assert_eq!(s.doc.text(), "a");
    }

    #[test]
    fn scroll_by_moves_first_line_within_bounds() {
        let mut s = state("a\nb\nc\nd\ne\nf\n");
        s.viewport = Viewport { first_line: 0, visible_lines: 2 };
        s.scroll_by(3);
        assert_eq!(s.viewport.first_line, 3);
        s.scroll_by(-10);
        assert_eq!(s.viewport.first_line, 0);
        s.scroll_by(100);
        assert_eq!(s.viewport.first_line, s.doc.buffer().len_lines() - 1);
    }

    #[test]
    fn save_without_path_is_an_error() {
        let mut s = state("hola");
        assert!(s.save().is_err());
    }

    #[test]
    fn save_writes_file_and_clears_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        let mut s = state("hol");
        s.path = Some(path.clone());
        // Un `Document` recién creado no cuenta como "sucio" (no hay edición que
        // deshacer): lo editamos para que `is_dirty()` sea true antes de guardar.
        s.apply(MoveDocEnd, Instant::now());
        s.insert_char('a', Instant::now());
        assert!(s.doc.is_dirty());
        s.save().unwrap();
        assert!(!s.doc.is_dirty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hola");
    }
}
