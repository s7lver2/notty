/// Solo se usa para decidir qué inserta Enter y qué muestra la barra de estado.
/// El texto se guarda tal cual; solo `convert` lo cambia, y solo si el usuario lo pide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Crlf,
    Lf,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Crlf => "\r\n",
            Self::Lf => "\n",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Crlf => "CRLF",
            Self::Lf => "LF",
        }
    }
}

pub fn detect_eol(text: &str) -> LineEnding {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    if lf > crlf { LineEnding::Lf } else { LineEnding::Crlf }
}

pub fn convert(text: &str, to: LineEnding) -> String {
    let lf = text.replace("\r\n", "\n");
    match to {
        LineEnding::Lf => lf,
        LineEnding::Crlf => lf.replace('\n', "\r\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_majority() {
        assert_eq!(detect_eol("a\r\nb\r\n"), LineEnding::Crlf);
        assert_eq!(detect_eol("a\nb\n"), LineEnding::Lf);
        assert_eq!(detect_eol("a\r\nb\nc\n"), LineEnding::Lf);
    }

    #[test]
    fn no_line_breaks_defaults_to_crlf() {
        assert_eq!(detect_eol("sin saltos"), LineEnding::Crlf);
    }

    #[test]
    fn converts_mixed_text() {
        assert_eq!(convert("a\r\nb\nc", LineEnding::Lf), "a\nb\nc");
        assert_eq!(convert("a\r\nb\nc", LineEnding::Crlf), "a\r\nb\r\nc");
    }

    #[test]
    fn labels() {
        assert_eq!(LineEnding::Crlf.label(), "CRLF");
        assert_eq!(LineEnding::Lf.as_str(), "\n");
    }
}
