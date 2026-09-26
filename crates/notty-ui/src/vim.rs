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
    /// Contador tecleado antes del comando (`10000` en `10000dd`).
    count: String,
    /// Lo último copiado o borrado con `yy`/`dd`/`x`/`dw`/`D`, para `p`/`P`.
    register: String,
    register_linewise: bool,
}

/// Techo del contador: `99999999dd` no debe quedarse pensando en un documento enorme.
const MAX_COUNT: usize = 1_000_000;

#[derive(PartialEq, Eq, Clone, Copy)]
enum CharClass {
    Space,
    Word,
    Punct,
}

fn class_of(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Space
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punct
    }
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
                self.count.clear();
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

    /// Inicio de la línea `line` + `n` (o el final del buffer si no hay tantas).
    fn line_span_end(doc: &Document, line: usize, n: usize) -> usize {
        let buf = doc.buffer();
        let last = line + n;
        if last < buf.len_lines() { buf.line_start(last) } else { buf.len_chars() }
    }

    fn next_word_start(doc: &Document, from: usize) -> usize {
        let buf = doc.buffer();
        let len = buf.len_chars();
        let rest: Vec<char> = buf.slice(from..len).chars().collect();
        let mut i = 0;
        if let Some(&first) = rest.first() {
            let cls = class_of(first);
            if cls != CharClass::Space {
                while i < rest.len() && class_of(rest[i]) == cls {
                    i += 1;
                }
            }
        }
        while i < rest.len() && class_of(rest[i]) == CharClass::Space {
            i += 1;
        }
        from + i
    }

    fn word_end(doc: &Document, from: usize) -> usize {
        let buf = doc.buffer();
        let len = buf.len_chars();
        let rest: Vec<char> = buf.slice(from..len).chars().collect();
        let mut i = 1;
        while i < rest.len() && class_of(rest[i]) == CharClass::Space {
            i += 1;
        }
        if i >= rest.len() {
            return len.saturating_sub(1).max(from);
        }
        let cls = class_of(rest[i]);
        while i + 1 < rest.len() && class_of(rest[i + 1]) == cls {
            i += 1;
        }
        from + i
    }

    fn prev_word_start(doc: &Document, from: usize) -> usize {
        let before: Vec<char> = doc.buffer().slice(0..from).chars().collect();
        let mut i = before.len();
        while i > 0 && class_of(before[i - 1]) == CharClass::Space {
            i -= 1;
        }
        if i == 0 {
            return 0;
        }
        let cls = class_of(before[i - 1]);
        while i > 0 && class_of(before[i - 1]) == cls {
            i -= 1;
        }
        i
    }

    fn yank(&mut self, text: String, linewise: bool) {
        self.register = text;
        self.register_linewise = linewise;
    }

    fn handle_normal_or_visual(&mut self, doc: &mut Document, c: char, now: Instant) -> VimOutcome {
        // Contador: dígitos antes del comando (un `0` inicial es "inicio de línea").
        if c.is_ascii_digit() && (c != '0' || !self.count.is_empty()) {
            if self.count.len() < 8 {
                self.count.push(c);
            }
            return VimOutcome::Handled;
        }
        // Un prefijo (`d`, `g`, `y`) seguido de algo que no forma combinación se
        // descarta y la tecla nueva cuenta sola: si no, `d` y luego `i` se tragaban la `i`.
        if !self.pending.is_empty()
            && !matches!((self.pending.as_str(), c), ("d", 'd') | ("d", 'w') | ("g", 'g') | ("y", 'y'))
        {
            self.pending.clear();
        }
        let key = self.pending.clone() + &c.to_string();
        let explicit_count = self.count.parse::<usize>().ok().map(|n| n.clamp(1, MAX_COUNT));
        let n = explicit_count.unwrap_or(1);
        let head = doc.selection().head;
        let buf_len = doc.buffer().len_chars();
        self.pending.clear();
        // Los prefijos conservan el contador hasta que se complete el comando.
        if !matches!(key.as_str(), "d" | "g" | "y") {
            self.count.clear();
        }

        match key.as_str() {
            // Solo `i` entra en Insert (decisión de producto: una única puerta de entrada).
            "i" => self.mode = VimMode::Insert,
            "h" => {
                let (line, _) = doc.buffer().line_col(head);
                let start = doc.buffer().line_start(line);
                self.move_to(doc, head.saturating_sub(n).max(start));
            }
            "l" => {
                let end = self.line_end(doc, head);
                self.move_to(doc, (head + n).min(end));
            }
            "j" | "k" => {
                let buf = doc.buffer();
                let (line, col) = buf.line_col(head);
                let delta = if key == "j" { n as i64 } else { -(n as i64) };
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
            "^" => {
                let (line, _) = doc.buffer().line_col(head);
                let start = doc.buffer().line_start(line);
                let end = self.line_end(doc, head);
                let text = doc.buffer().slice(start..end);
                let indent = text.chars().take_while(|c| c.is_whitespace()).count();
                self.move_to(doc, start + indent);
            }
            "$" => {
                let end = self.line_end(doc, head);
                self.move_to(doc, end);
            }
            "w" => {
                let mut p = head;
                for _ in 0..n {
                    let next = Self::next_word_start(doc, p);
                    if next == p {
                        break;
                    }
                    p = next;
                }
                self.move_to(doc, p);
            }
            "e" => {
                let mut p = head;
                for _ in 0..n {
                    let next = Self::word_end(doc, p);
                    if next == p {
                        break;
                    }
                    p = next;
                }
                self.move_to(doc, p);
            }
            "b" => {
                let mut p = head;
                for _ in 0..n {
                    let prev = Self::prev_word_start(doc, p);
                    if prev == p {
                        break;
                    }
                    p = prev;
                }
                self.move_to(doc, p);
            }
            "x" => {
                let end = self.line_end(doc, head);
                let to = (head + n).min(end);
                if to > head {
                    let removed = doc.buffer().slice(head..to);
                    self.yank(removed, false);
                    doc.replace_range(head..to, "", now);
                }
            }
            "~" => {
                let end = self.line_end(doc, head);
                let to = (head + n).min(end);
                if to > head {
                    let toggled: String = doc
                        .buffer()
                        .slice(head..to)
                        .chars()
                        .map(|c| if c.is_uppercase() { c.to_lowercase().next().unwrap_or(c) } else { c.to_uppercase().next().unwrap_or(c) })
                        .collect();
                    doc.replace_range(head..to, &toggled, now);
                    doc.set_cursor(to.min(end));
                }
            }
            "dd" => {
                let (line, _) = doc.buffer().line_col(head);
                let mut start = doc.buffer().line_start(line);
                let end = Self::line_span_end(doc, line, n);
                // Borrando hasta el final del documento: también el salto de línea que
                // queda delante, para no dejar una línea vacía colgando.
                if end == buf_len && start > 0 && line + n >= doc.buffer().len_lines() {
                    let prev_text = doc.buffer().slice(start - 1..start);
                    if prev_text == "\n" {
                        start -= if start >= 2 && doc.buffer().slice(start - 2..start - 1) == "\r" { 2 } else { 1 };
                    }
                }
                let removed = doc.buffer().slice(start..end);
                self.yank(removed, true);
                doc.replace_range(start..end, "", now);
                let caret = start.min(doc.buffer().len_chars());
                let (l, _) = doc.buffer().line_col(caret);
                let ls = doc.buffer().line_start(l);
                doc.set_cursor(ls);
            }
            "dw" => {
                let mut p = head;
                for _ in 0..n {
                    let next = Self::next_word_start(doc, p);
                    if next == p {
                        break;
                    }
                    p = next;
                }
                if p > head {
                    let removed = doc.buffer().slice(head..p);
                    self.yank(removed, false);
                    doc.replace_range(head..p, "", now);
                }
            }
            "D" => {
                let end = self.line_end(doc, head);
                if end > head {
                    let removed = doc.buffer().slice(head..end);
                    self.yank(removed, false);
                    doc.replace_range(head..end, "", now);
                }
            }
            "yy" => {
                let (line, _) = doc.buffer().line_col(head);
                let start = doc.buffer().line_start(line);
                let end = Self::line_span_end(doc, line, n);
                let mut text = doc.buffer().slice(start..end);
                if !text.ends_with('\n') {
                    text.push_str(doc.newline());
                }
                self.yank(text, true);
            }
            "p" | "P" => {
                if self.register.is_empty() {
                    return VimOutcome::Handled;
                }
                let text = self.register.repeat(n);
                if self.register_linewise {
                    let (line, _) = doc.buffer().line_col(head);
                    let at = if key == "P" { doc.buffer().line_start(line) } else { Self::line_span_end(doc, line, 1) };
                    // Pegar debajo de la última línea (sin salto final): el salto va delante.
                    let at_end_without_nl = at == doc.buffer().len_chars()
                        && !doc.buffer().slice(0..at).ends_with('\n')
                        && at > 0;
                    let insert = if at_end_without_nl {
                        format!("{}{}", doc.newline(), text.trim_end_matches(['\r', '\n']))
                    } else {
                        text
                    };
                    doc.replace_range(at..at, &insert, now);
                    let first_line_start = if at_end_without_nl { at + doc.newline().chars().count() } else { at };
                    doc.set_cursor(first_line_start);
                } else {
                    let end = self.line_end(doc, head);
                    let at = if key == "p" { (head + 1).min(end.max(head)) } else { head };
                    doc.replace_range(at..at, &text, now);
                    let len = text.chars().count();
                    doc.set_cursor((at + len).saturating_sub(1));
                }
            }
            "J" => {
                for _ in 0..n.max(1) {
                    let (line, _) = doc.buffer().line_col(doc.selection().head);
                    if line + 1 >= doc.buffer().len_lines() {
                        break;
                    }
                    let end = self.line_end(doc, doc.selection().head);
                    let next_start = doc.buffer().line_start(line + 1);
                    let next_end = self.line_end(doc, next_start);
                    let next_text = doc.buffer().slice(next_start..next_end);
                    let indent = next_text.chars().take_while(|c| c.is_whitespace()).count();
                    let sep = if next_text.trim().is_empty() { "" } else { " " };
                    doc.replace_range(end..next_start + indent, sep, now);
                    doc.set_cursor(end);
                }
            }
            "d" => self.pending = "d".to_string(),
            "y" => self.pending = "y".to_string(),
            "gg" => {
                let target = match explicit_count {
                    Some(line) => doc.buffer().line_start((line - 1).min(doc.buffer().len_lines() - 1)),
                    None => 0,
                };
                self.move_to(doc, target);
            }
            "g" => self.pending = "g".to_string(),
            "G" => {
                let target = match explicit_count {
                    Some(line) => doc.buffer().line_start((line - 1).min(doc.buffer().len_lines() - 1)),
                    None => buf_len,
                };
                self.move_to(doc, target);
            }
            "u" => {
                for _ in 0..n {
                    if !doc.undo() {
                        break;
                    }
                }
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

    fn keys(v: &mut VimState, d: &mut Document, s: &str) {
        for c in s.chars() {
            ch(v, d, c);
        }
    }

    #[test]
    fn count_dd_deletes_that_many_lines() {
        let mut v = VimState::default();
        let mut d = doc("1\n2\n3\n4\n5");
        keys(&mut v, &mut d, "3dd");
        assert_eq!(d.text(), "4\n5");
    }

    #[test]
    fn huge_count_dd_empties_the_document_without_hanging() {
        let mut v = VimState::default();
        let mut d = doc("a\nb\nc");
        keys(&mut v, &mut d, "10000dd");
        assert_eq!(d.text(), "");
    }

    #[test]
    fn dd_on_last_line_takes_the_preceding_newline() {
        let mut v = VimState::default();
        let mut d = doc("a\nb");
        d.set_cursor(2);
        keys(&mut v, &mut d, "dd");
        assert_eq!(d.text(), "a");
    }

    #[test]
    fn count_moves_and_count_x() {
        let mut v = VimState::default();
        let mut d = doc("abcdef\n1\n2\n3");
        keys(&mut v, &mut d, "3l");
        assert_eq!(d.selection(), Selection::caret(3));
        keys(&mut v, &mut d, "2x");
        assert_eq!(d.text(), "abcf\n1\n2\n3");
        keys(&mut v, &mut d, "2j");
        assert_eq!(d.line_col().0, 2);
    }

    #[test]
    fn count_g_goes_to_line() {
        let mut v = VimState::default();
        let mut d = doc("a\nb\nc\nd");
        keys(&mut v, &mut d, "3G");
        assert_eq!(d.line_col().0, 2);
    }

    #[test]
    fn zero_alone_is_line_start_but_inside_a_count_is_a_digit() {
        let mut v = VimState::default();
        let mut d = doc(&"x\n".repeat(20));
        keys(&mut v, &mut d, "10j");
        assert_eq!(d.line_col().0, 10);
        keys(&mut v, &mut d, "0");
        assert_eq!(d.line_col(), (10, 0));
    }

    #[test]
    fn word_motions() {
        let mut v = VimState::default();
        let mut d = doc("uno dos, tres");
        keys(&mut v, &mut d, "w");
        assert_eq!(d.selection(), Selection::caret(4));
        keys(&mut v, &mut d, "e");
        assert_eq!(d.selection(), Selection::caret(6));
        keys(&mut v, &mut d, "b");
        assert_eq!(d.selection(), Selection::caret(4));
        keys(&mut v, &mut d, "2w");
        assert_eq!(d.selection(), Selection::caret(9));
    }

    #[test]
    fn yy_and_p_duplicate_a_line() {
        let mut v = VimState::default();
        let mut d = doc("uno\ndos");
        keys(&mut v, &mut d, "yyp");
        assert_eq!(d.text(), "uno\nuno\ndos");
    }

    #[test]
    fn dd_then_p_moves_a_line_down() {
        let mut v = VimState::default();
        let mut d = doc("uno\ndos\ntres");
        keys(&mut v, &mut d, "ddp");
        assert_eq!(d.text(), "dos\nuno\ntres");
    }

    #[test]
    fn dw_d_and_j() {
        let mut v = VimState::default();
        let mut d = doc("uno dos\n  tres");
        keys(&mut v, &mut d, "dw");
        assert_eq!(d.text(), "dos\n  tres");
        keys(&mut v, &mut d, "J");
        assert_eq!(d.text(), "dos tres");
        d.set_cursor(3);
        keys(&mut v, &mut d, "D");
        assert_eq!(d.text(), "dos");
    }

    #[test]
    fn only_i_enters_insert() {
        for c in ['a', 'A', 'o', 'O', 's', 'c'] {
            let mut v = VimState::default();
            let mut d = doc("abc");
            ch(&mut v, &mut d, c);
            assert_eq!(v.mode, VimMode::Normal, "{c} no debe entrar en Insert");
            assert_eq!(d.text(), "abc");
        }
    }

    #[test]
    fn i_after_a_dangling_prefix_still_enters_insert() {
        let mut v = VimState::default();
        let mut d = doc("abc");
        ch(&mut v, &mut d, 'd');
        ch(&mut v, &mut d, 'i');
        assert_eq!(v.mode, VimMode::Insert);
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
