use std::ops::Range;

/// Qué líneas del documento hay que dibujar. `first_line` es la primera línea
/// visible (scroll); `visible_lines` sale del alto de la ventana en píxeles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub first_line: usize,
    pub visible_lines: usize,
}

impl Viewport {
    pub fn new(line_height_px: f32, client_height_px: f32) -> Self {
        let lines = (client_height_px / line_height_px).floor() as usize;
        Self { first_line: 0, visible_lines: lines.max(1) }
    }

    /// Rango `[first_line, first_line + visible_lines)` recortado a `[0, total_lines)`.
    pub fn range(&self, total_lines: usize) -> Range<usize> {
        let start = self.first_line.min(total_lines);
        let end = (start + self.visible_lines).min(total_lines);
        start..end
    }

    /// Mueve `first_line` lo mínimo para que `line` quede dentro de lo visible.
    pub fn scroll_to_include(&mut self, line: usize, total_lines: usize) {
        if line < self.first_line {
            self.first_line = line;
        } else if line >= self.first_line + self.visible_lines {
            self.first_line = line + 1 - self.visible_lines;
        }
        let max_first = total_lines.saturating_sub(1);
        self.first_line = self.first_line.min(max_first);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_lines_from_pixel_height() {
        let v = Viewport::new(20.0, 205.0);
        assert_eq!(v.visible_lines, 10);
    }

    #[test]
    fn at_least_one_visible_line() {
        let v = Viewport::new(20.0, 5.0);
        assert_eq!(v.visible_lines, 1);
    }

    #[test]
    fn range_is_clamped_to_document() {
        let v = Viewport { first_line: 0, visible_lines: 10 };
        assert_eq!(v.range(3), 0..3);
    }

    #[test]
    fn range_starts_at_first_line() {
        let v = Viewport { first_line: 5, visible_lines: 4 };
        assert_eq!(v.range(100), 5..9);
    }

    #[test]
    fn scroll_down_to_include_line_below() {
        let mut v = Viewport { first_line: 0, visible_lines: 10 };
        v.scroll_to_include(15, 100);
        assert_eq!(v.first_line, 6);
        assert!(v.range(100).contains(&15));
    }

    #[test]
    fn scroll_up_to_include_line_above() {
        let mut v = Viewport { first_line: 20, visible_lines: 10 };
        v.scroll_to_include(5, 100);
        assert_eq!(v.first_line, 5);
    }

    #[test]
    fn already_visible_line_does_not_move_scroll() {
        let mut v = Viewport { first_line: 10, visible_lines: 10 };
        v.scroll_to_include(15, 100);
        assert_eq!(v.first_line, 10);
    }

    #[test]
    fn scroll_never_goes_past_document_end() {
        let mut v = Viewport { first_line: 0, visible_lines: 10 };
        v.scroll_to_include(4, 5);
        assert_eq!(v.first_line, 0);
    }
}
