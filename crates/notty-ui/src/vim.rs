use std::time::Instant;

use notty_core::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VimMode {
    #[default]
    Normal,
    Insert,
    Visual,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VimState {
    pub mode: VimMode,
    pending: String,
    visual_anchor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimOutcome {
    Handled,
    OpenFind,
    OpenCmdline,
    Bubble,
}

impl VimState {
    pub fn handle_key(&mut self, doc: &mut Document, vk: u32, ch: Option<char>, now: Instant) -> VimOutcome {
        if self.mode == VimMode::Insert {
            return self.handle_insert(doc, vk, ch, now);
        }
        match ch {
            Some(c) => self.handle_normal_or_visual(doc, c, now),
            None if vk == 0x1B => {
                self.pending.clear();
                VimOutcome::Handled
            }
            None => VimOutcome::Bubble,
        }
    }

    fn handle_insert(&mut self, doc: &mut Document, vk: u32, ch: Option<char>, now: Instant) -> VimOutcome {
        if vk == 0x1B {
            self.mode = VimMode::Normal;
            return VimOutcome::Handled;
        }
        match ch {
            Some(c) if !c.is_control() => {
                doc.insert(&c.to_string(), now);
                VimOutcome::Handled
            }
            Some(_) | None => VimOutcome::Bubble,
        }
    }

    fn line_end(&self, doc: &Document, idx: usize) -> usize {
        let buf = doc.buffer();
        let (line, _) = buf.line_col(idx);
        let total = buf.len_lines();
        let end = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
        let text = buf.slice(buf.line_start(line)..end);
        buf.line_start(line) + text.trim_end_matches(['\r', '\n']).chars().count()
    }

    fn move_to(&mut self, doc: &mut Document, idx: usize) {
        if self.mode == VimMode::Visual {
            doc.set_selection(self.visual_anchor, idx);
        } else {
            doc.set_cursor(idx);
        }
    }

    fn handle_normal_or_visual(&mut self, doc: &mut Document, c: char, now: Instant) -> VimOutcome {
        let key = self.pending.clone() + &c.to_string();
        let head = doc.selection().head;
        let buf_len = doc.buffer().len_chars();
        self.pending.clear();

        match key.as_str() {
            "i" => self.mode = VimMode::Insert,
            "a" => {
                doc.set_cursor((head + 1).min(buf_len));
                self.mode = VimMode::Insert;
            }
            "A" => {
                let end = self.line_end(doc, head);
                doc.set_cursor(end);
                self.mode = VimMode::Insert;
            }
            "o" => {
                let end = self.line_end(doc, head);
                doc.set_cursor(end);
                doc.insert_newline(now);
                self.mode = VimMode::Insert;
            }
            "h" => self.move_to(doc, head.saturating_sub(1)),
            "l" => self.move_to(doc, (head + 1).min(buf_len)),
            "j" | "k" => {
                let buf = doc.buffer();
                let (line, col) = buf.line_col(head);
                let delta: i64 = if key == "j" { 1 } else { -1 };
                let target_line = (line as i64 + delta).clamp(0, buf.len_lines() as i64 - 1) as usize;
                let start = buf.line_start(target_line);
                let end = self.line_end(doc, start);
                let idx = (start + col).min(end);
                self.move_to(doc, idx);
            }
            "0" => {
                let (line, _) = doc.buffer().line_col(head);
                let start = doc.buffer().line_start(line);
                self.move_to(doc, start);
            }
            "$" => {
                let end = self.line_end(doc, head);
                self.move_to(doc, end);
            }
            "x" => {
                if head < buf_len {
                    doc.replace_range(head..head + 1, "", now);
                }
            }
            "dd" => {
                let buf = doc.buffer();
                let (line, _) = buf.line_col(head);
                let start = buf.line_start(line);
                let total = buf.len_lines();
                let end = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
                doc.replace_range(start..end, "", now);
            }
            "d" => self.pending = "d".to_string(),
            "gg" => self.move_to(doc, 0),
            "g" => self.pending = "g".to_string(),
            "G" => self.move_to(doc, buf_len),
            "u" => {
                doc.undo();
            }
            "v" => {
                if self.mode == VimMode::Visual {
                    self.mode = VimMode::Normal;
                } else {
                    self.visual_anchor = head;
                    self.mode = VimMode::Visual;
                }
            }
            "/" => return VimOutcome::OpenFind,
            ":" => return VimOutcome::OpenCmdline,
            _ => return VimOutcome::Bubble,
        }
        VimOutcome::Handled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notty_core::{Document, Selection};
    use std::time::Instant;

    fn doc(text: &str) -> Document {
        Document::new(text, "\n")
    }

    fn key(v: &mut VimState, d: &mut Document, vk: u32) -> VimOutcome {
        v.handle_key(d, vk, None, Instant::now())
    }

    fn ch(v: &mut VimState, d: &mut Document, c: char) -> VimOutcome {
        v.handle_key(d, 0, Some(c), Instant::now())
    }

    #[test]
    fn starts_in_normal_mode() {
        assert_eq!(VimState::default().mode, VimMode::Normal);
    }

    #[test]
    fn i_enters_insert_and_typing_edits_the_document() {
        let mut v = VimState::default();
        let mut d = doc("ac");
        ch(&mut v, &mut d, 'i');
        assert_eq!(v.mode, VimMode::Insert);
        d.set_cursor(1);
        ch(&mut v, &mut d, 'b');
        assert_eq!(d.text(), "abc");
    }

    #[test]
    fn esc_returns_to_normal() {
        let mut v = VimState::default();
        let mut d = doc("a");
        ch(&mut v, &mut d, 'i');
        key(&mut v, &mut d, 0x1B);
        assert_eq!(v.mode, VimMode::Normal);
    }

    #[test]
    fn hjkl_move_the_cursor() {
        let mut v = VimState::default();
        let mut d = doc("ab\ncd");
        ch(&mut v, &mut d, 'l');
        assert_eq!(d.selection(), Selection::caret(1));
        ch(&mut v, &mut d, 'j');
        assert_eq!(d.line_col().0, 1);
        ch(&mut v, &mut d, 'h');
        ch(&mut v, &mut d, 'k');
        assert_eq!(d.line_col().0, 0);
    }

    #[test]
    fn zero_and_dollar_go_to_line_bounds() {
        let mut v = VimState::default();
        let mut d = doc("abcdef");
        d.set_cursor(3);
        ch(&mut v, &mut d, '0');
        assert_eq!(d.selection(), Selection::caret(0));
        ch(&mut v, &mut d, '$');
        assert_eq!(d.selection(), Selection::caret(6));
    }

    #[test]
    fn x_deletes_char_under_cursor() {
        let mut v = VimState::default();
        let mut d = doc("abc");
        ch(&mut v, &mut d, 'x');
        assert_eq!(d.text(), "bc");
    }

    #[test]
    fn dd_deletes_the_whole_line() {
        let mut v = VimState::default();
        let mut d = doc("uno\ndos\ntres");
        d.set_cursor(5); // dentro de "dos"
        ch(&mut v, &mut d, 'd');
        ch(&mut v, &mut d, 'd');
        assert_eq!(d.text(), "uno\ntres");
    }

    #[test]
    fn gg_and_g_go_to_document_bounds() {
        let mut v = VimState::default();
        let mut d = doc("abc\ndef");
        ch(&mut v, &mut d, 'G');
        assert_eq!(d.selection(), Selection::caret(7));
        ch(&mut v, &mut d, 'g');
        ch(&mut v, &mut d, 'g');
        assert_eq!(d.selection(), Selection::caret(0));
    }

    #[test]
    fn u_undoes() {
        let mut v = VimState::default();
        let mut d = doc("a");
        d.insert("b", Instant::now());
        ch(&mut v, &mut d, 'u');
        assert_eq!(d.text(), "a");
    }

    #[test]
    fn v_enters_visual_and_hjkl_extends_selection() {
        let mut v = VimState::default();
        let mut d = doc("abcdef");
        ch(&mut v, &mut d, 'v');
        assert_eq!(v.mode, VimMode::Visual);
        ch(&mut v, &mut d, 'l');
        ch(&mut v, &mut d, 'l');
        assert_eq!(d.selection(), Selection { anchor: 0, head: 2 });
    }

    #[test]
    fn slash_and_colon_open_prompts() {
        let mut v = VimState::default();
        let mut d = doc("a");
        assert_eq!(ch(&mut v, &mut d, '/'), VimOutcome::OpenFind);
        assert_eq!(ch(&mut v, &mut d, ':'), VimOutcome::OpenCmdline);
    }

    #[test]
    fn unmapped_key_bubbles_up() {
        let mut v = VimState::default();
        let mut d = doc("a");
        assert_eq!(key(&mut v, &mut d, 0xBD /* algo que vim no usa */), VimOutcome::Bubble);
    }
}
