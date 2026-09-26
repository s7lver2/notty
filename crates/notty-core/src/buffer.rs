use std::fmt;
use std::ops::Range;

use ropey::Rope;

/// Texto del documento. Todos los índices son de char, no de byte.
/// Guarda los saltos de línea tal cual (`\r\n` o `\n`); ropey cuenta `\r\n` como uno.
#[derive(Debug, Clone, Default)]
pub struct Buffer {
    rope: Rope,
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        Self { rope: Rope::from_str(text) }
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        self.rope.insert(char_idx, text);
    }

    pub fn remove(&mut self, range: Range<usize>) {
        if !range.is_empty() {
            self.rope.remove(range);
        }
    }

    pub fn slice(&self, range: Range<usize>) -> String {
        self.rope.slice(range).to_string()
    }

    /// (línea, columna), ambas empezando en 0.
    pub fn line_col(&self, char_idx: usize) -> (usize, usize) {
        let line = self.rope.char_to_line(char_idx);
        (line, char_idx - self.rope.line_to_char(line))
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.rope.line_to_char(line.min(self.len_lines() - 1))
    }

    pub fn byte_to_char(&self, byte_idx: usize) -> usize {
        self.rope.byte_to_char(byte_idx)
    }

    pub fn char_to_byte(&self, char_idx: usize) -> usize {
        self.rope.char_to_byte(char_idx)
    }
}

impl fmt::Display for Buffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for chunk in self.rope.chunks() {
            f.write_str(chunk)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_remove_round_trip() {
        let mut b = Buffer::new("hola");
        b.insert(4, " mundo");
        assert_eq!(b.to_string(), "hola mundo");
        b.remove(0..5);
        assert_eq!(b.to_string(), "mundo");
    }

    #[test]
    fn crlf_counts_as_one_line_break() {
        let b = Buffer::new("ab\r\ncd");
        assert_eq!(b.len_lines(), 2);
        assert_eq!(b.line_col(1), (0, 1));
        assert_eq!(b.line_col(4), (1, 0));
        assert_eq!(b.line_start(1), 4);
    }

    #[test]
    fn empty_buffer_has_one_line() {
        assert_eq!(Buffer::new("").len_lines(), 1);
    }

    #[test]
    fn byte_to_char_handles_multibyte() {
        let b = Buffer::new("ña");
        assert_eq!(b.byte_to_char(2), 1);
        assert_eq!(b.slice(0..1), "ñ");
    }
}
