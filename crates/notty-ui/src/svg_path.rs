//! Lector mínimo del atributo `d` de un `<path>` SVG (lo justo para los iconos de
//! trazo de Ajustes: M L H V C A Z, absolutos y relativos). Devuelve segmentos en
//! coordenadas absolutas del lienzo del icono (24×24); `render.rs` los convierte en
//! geometría Direct2D.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Cubic { c1: (f32, f32), c2: (f32, f32), to: (f32, f32) },
    Arc { to: (f32, f32), rx: f32, ry: f32, rot: f32, large: bool, sweep: bool },
    Close,
}

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl Lexer<'_> {
    fn skip_sep(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }

    fn peek_cmd(&mut self) -> Option<u8> {
        self.skip_sep();
        self.s.get(self.i).copied().filter(|c| c.is_ascii_alphabetic())
    }

    fn at_number(&mut self) -> bool {
        self.skip_sep();
        self.s.get(self.i).is_some_and(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.'))
    }

    fn number(&mut self) -> Option<f32> {
        self.skip_sep();
        let start = self.i;
        let s = self.s;
        if self.i < s.len() && matches!(s[self.i], b'-' | b'+') {
            self.i += 1;
        }
        let mut dot = false;
        while self.i < s.len() && (s[self.i].is_ascii_digit() || (s[self.i] == b'.' && !dot)) {
            dot |= s[self.i] == b'.';
            self.i += 1;
        }
        if self.i < s.len() && matches!(s[self.i], b'e' | b'E') {
            self.i += 1;
            if self.i < s.len() && matches!(s[self.i], b'-' | b'+') {
                self.i += 1;
            }
            while self.i < s.len() && s[self.i].is_ascii_digit() {
                self.i += 1;
            }
        }
        std::str::from_utf8(&s[start..self.i]).ok()?.parse().ok()
    }

    /// Las banderas de un arco pueden ir pegadas ("a1 1 0 011 1"): un solo dígito.
    fn flag(&mut self) -> Option<bool> {
        self.skip_sep();
        let c = *self.s.get(self.i)?;
        self.i += 1;
        match c {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }
}

/// Segmentos de `d`, o los que se pudieron leer antes del primer error.
pub fn parse(d: &str) -> Vec<Seg> {
    let mut lx = Lexer { s: d.as_bytes(), i: 0 };
    let mut out = Vec::new();
    let (mut cx, mut cy) = (0.0f32, 0.0f32);
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    let mut cmd = 0u8;
    loop {
        if let Some(c) = lx.peek_cmd() {
            cmd = c;
            lx.i += 1;
        } else if !lx.at_number() || cmd == 0 {
            break;
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (Some(x), Some(y)) = (lx.number(), lx.number()) else { break };
                cx = ox + x;
                cy = oy + y;
                sx = cx;
                sy = cy;
                out.push(Seg::Move(cx, cy));
                // Pares extra tras un M son L implícitos.
                cmd = if rel { b'l' } else { b'L' };
            }
            b'L' => {
                let (Some(x), Some(y)) = (lx.number(), lx.number()) else { break };
                cx = ox + x;
                cy = oy + y;
                out.push(Seg::Line(cx, cy));
            }
            b'H' => {
                let Some(x) = lx.number() else { break };
                cx = ox + x;
                out.push(Seg::Line(cx, cy));
            }
            b'V' => {
                let Some(y) = lx.number() else { break };
                cy = if rel { cy + y } else { y };
                out.push(Seg::Line(cx, cy));
            }
            b'C' => {
                let mut n = [0.0f32; 6];
                for v in &mut n {
                    match lx.number() {
                        Some(x) => *v = x,
                        None => return out,
                    }
                }
                let c1 = (ox + n[0], oy + n[1]);
                let c2 = (ox + n[2], oy + n[3]);
                cx = ox + n[4];
                cy = oy + n[5];
                out.push(Seg::Cubic { c1, c2, to: (cx, cy) });
            }
            b'A' => {
                let (Some(rx), Some(ry), Some(rot)) = (lx.number(), lx.number(), lx.number()) else { break };
                let (Some(large), Some(sweep)) = (lx.flag(), lx.flag()) else { break };
                let (Some(x), Some(y)) = (lx.number(), lx.number()) else { break };
                cx = ox + x;
                cy = oy + y;
                out.push(Seg::Arc { to: (cx, cy), rx, ry, rot, large, sweep });
            }
            b'Z' => {
                out.push(Seg::Close);
                cx = sx;
                cy = sy;
                cmd = 0;
            }
            _ => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_lines_and_implicit_repeats() {
        let segs = parse("m9 6 6 6-6 6");
        assert_eq!(segs, vec![Seg::Move(9.0, 6.0), Seg::Line(15.0, 12.0), Seg::Line(9.0, 18.0)]);
    }

    #[test]
    fn horizontal_vertical_and_close() {
        let segs = parse("M3 9h18V12h-2z");
        assert_eq!(segs, vec![Seg::Move(3.0, 9.0), Seg::Line(21.0, 9.0), Seg::Line(21.0, 12.0), Seg::Line(19.0, 12.0), Seg::Close]);
    }

    #[test]
    fn numbers_glued_by_signs_and_dots() {
        let segs = parse("M12 3c1.1 0 1.8-.8 1.8-1.7");
        let Seg::Cubic { c1, c2, to } = segs[1] else { panic!("{segs:?}") };
        assert_eq!(c1, (13.1, 3.0));
        assert!((c2.0 - 13.8).abs() < 1e-4 && (c2.1 - 2.2).abs() < 1e-4);
        assert!((to.0 - 13.8).abs() < 1e-4 && (to.1 - 1.3).abs() < 1e-4);
    }

    #[test]
    fn arcs_with_flags() {
        let segs = parse("M21 12a9 9 0 1 1-9-9");
        assert_eq!(segs[1], Seg::Arc { to: (12.0, 3.0), rx: 9.0, ry: 9.0, rot: 0.0, large: true, sweep: true });
        let packed = parse("M0 0a1 1 0 011 1");
        assert_eq!(packed[1], Seg::Arc { to: (1.0, 1.0), rx: 1.0, ry: 1.0, rot: 0.0, large: false, sweep: true });
    }

    #[test]
    fn several_subpaths() {
        let segs = parse("M6 10h.01M10 10h.01");
        assert_eq!(segs.iter().filter(|s| matches!(s, Seg::Move(..))).count(), 2);
    }

    #[test]
    fn garbage_stops_without_panicking() {
        assert!(parse("").is_empty());
        assert_eq!(parse("M1 1 L").len(), 1);
    }
}
