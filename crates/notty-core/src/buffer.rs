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
        Self { rope: build_rope(text) }
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

    /// Todas las líneas sin su salto final, en una sola pasada por el rope (pedirlas
    /// una a una con `line_start` + `slice` es mucho más lento en archivos grandes).
    pub fn line_strings(&self) -> Vec<String> {
        self.rope
            .lines()
            .map(|l| {
                let mut s = l.to_string();
                s.truncate(s.trim_end_matches(['\r', '\n']).len());
                s
            })
            .collect()
    }

    pub fn byte_to_char(&self, byte_idx: usize) -> usize {
        self.rope.byte_to_char(byte_idx)
    }

    pub fn char_to_byte(&self, char_idx: usize) -> usize {
        self.rope.char_to_byte(char_idx)
    }
}

/// A partir de aquí el rope se construye en varios hilos, por trozos que luego se
/// unen (`Rope::append` es O(log n)): con 50 MB bajaba de ~46 ms a una fracción.
const PARALLEL_BYTES: usize = 4 * 1024 * 1024;

fn build_rope(text: &str) -> Rope {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8);
    if text.len() < PARALLEL_BYTES || threads < 2 {
        return Rope::from_str(text);
    }
    // Cortes en frontera de char y nunca entre el `\r` y el `\n` de un CRLF.
    let b = text.as_bytes();
    let mut cuts = vec![0];
    for k in 1..threads {
        let mut at = text.len() * k / threads;
        while at < text.len() && (!text.is_char_boundary(at) || (b[at] == b'\n' && b[at - 1] == b'\r')) {
            at += 1;
        }
        if at > *cuts.last().unwrap() && at < text.len() {
            cuts.push(at);
        }
    }
    cuts.push(text.len());
    let parts: Vec<Rope> = std::thread::scope(|s| {
        let handles: Vec<_> = cuts.windows(2).map(|w| s.spawn(move || Rope::from_str(&text[w[0]..w[1]]))).collect();
        handles.into_iter().map(|h| h.join().expect("construir un trozo del rope")).collect()
    });
    let mut parts = parts.into_iter();
    let mut rope = parts.next().unwrap_or_default();
    for p in parts {
        rope.append(p);
    }
    rope
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
    fn parallel_build_matches_single_rope() {
        let text = "línea ñ con crlf\r\n".repeat(PARALLEL_BYTES / 10);
        let par = Buffer::new(&text);
        let seq = Rope::from_str(&text);
        assert_eq!(par.len_chars(), seq.len_chars());
        assert_eq!(par.len_lines(), seq.len_lines());
        assert_eq!(par.to_string(), text);
        for l in [0, 1, 12345, par.len_lines() - 1] {
            assert_eq!(par.line_start(l), seq.line_to_char(l));
        }
    }

    #[test]
    fn line_strings_match_line_by_line() {
        let b = Buffer::new("uno\r\ndos\n\ntres\n");
        assert_eq!(b.line_strings(), vec!["uno", "dos", "", "tres", ""]);
        assert_eq!(b.line_strings().len(), b.len_lines());
    }

    #[test]
    fn byte_to_char_handles_multibyte() {
        let b = Buffer::new("ña");
        assert_eq!(b.byte_to_char(2), 1);
        assert_eq!(b.slice(0..1), "ñ");
    }
}
