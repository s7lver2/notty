use notty_core::Document;
use notty_io::{LineEnding, TextEncoding};

/// Texto de la barra de estado: "Texto · Ln <línea 1-based>, Col <col 1-based> · <codificación> · <CRLF/LF>".
pub fn status_line(doc: &Document, encoding: TextEncoding, eol: LineEnding) -> String {
    let (line, col) = doc.line_col();
    format!("Texto · Ln {}, Col {} · {} · {}", line + 1, col + 1, encoding.label(), eol.label())
}

#[cfg(test)]
mod tests {
    use super::*;
    use notty_core::Document;
    use notty_io::{LineEnding, TextEncoding};

    #[test]
    fn status_line_for_saved_text_file() {
        let mut doc = Document::new("hola\nmundo", "\n");
        doc.set_cursor(7);
        let s = status_line(&doc, TextEncoding::Utf8, LineEnding::Lf);
        // idx 7 en "hola\nmundo" cae en la 'n' de "mundo" (col 2 0-based -> 3 1-based).
        assert_eq!(s, "Texto · Ln 2, Col 3 · UTF-8 · LF");
    }

    #[test]
    fn status_line_uses_1_based_line_and_col() {
        let doc = Document::new("abc", "\r\n");
        let s = status_line(&doc, TextEncoding::Windows1252, LineEnding::Crlf);
        assert_eq!(s, "Texto · Ln 1, Col 1 · ANSI (1252) · CRLF");
    }
}
