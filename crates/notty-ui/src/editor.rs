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
