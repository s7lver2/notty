use std::io;
use std::path::{Path, PathBuf};

use notty_core::Document;
use notty_io::{CodecError, LineEnding, Opened, TextEncoding};

pub struct OpenedDoc {
    pub document: Document,
    pub encoding: TextEncoding,
    pub eol: LineEnding,
    pub writable: bool,
    pub lossy: bool,
    pub path: PathBuf,
}

pub fn open_as_document(path: &Path) -> io::Result<OpenedDoc> {
    match notty_io::open(path)? {
        Opened::Text { text, encoding, eol, writable, lossy } => {
            let document = Document::new(&text, eol.as_str());
            Ok(OpenedDoc { document, encoding, eol, writable: writable && !lossy, lossy, path: path.to_path_buf() })
        }
        Opened::Raw { .. } => Err(io::Error::new(io::ErrorKind::InvalidData, "no es texto: ábrelo en modo raw")),
    }
}

pub fn save_document(doc: &Document, path: &Path, encoding: TextEncoding) -> Result<(), CodecError> {
    let bytes = notty_io::encode(&doc.text(), encoding)?;
    notty_io::atomic_write(path, &bytes).map_err(|e| CodecError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn opens_utf8_file_as_document() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        fs::write(&p, "hola\r\nmundo").unwrap();
        let opened = open_as_document(&p).unwrap();
        assert_eq!(opened.document.text(), "hola\r\nmundo");
        assert_eq!(opened.encoding, notty_io::TextEncoding::Utf8);
        assert_eq!(opened.eol, notty_io::LineEnding::Crlf);
        assert!(opened.writable);
        assert!(!opened.lossy);
        assert_eq!(opened.path, p);
    }

    #[test]
    fn opening_binary_file_is_an_error() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("bin.dat");
        fs::write(&p, [0u8, 1, 2, 3]).unwrap();
        assert!(open_as_document(&p).is_err());
    }

    #[test]
    fn opening_missing_file_is_an_error() {
        let dir = tempdir().unwrap();
        assert!(open_as_document(&dir.path().join("no-existe.txt")).is_err());
    }

    #[test]
    fn save_document_round_trips_utf8() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("out.txt");
        let doc = notty_core::Document::new("línea uno\r\nlínea dos", "\r\n");
        save_document(&doc, &p, notty_io::TextEncoding::Utf8).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "línea uno\r\nlínea dos");
    }

    #[test]
    fn save_document_writes_bom_for_utf16() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("out16.txt");
        let doc = notty_core::Document::new("hola", "\n");
        save_document(&doc, &p, notty_io::TextEncoding::Utf16Le).unwrap();
        let bytes = fs::read(&p).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
    }

    #[test]
    fn save_document_rejects_unmappable_chars() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("out.txt");
        let doc = notty_core::Document::new("日本語", "\n");
        assert!(save_document(&doc, &p, notty_io::TextEncoding::Windows1252).is_err());
    }
}
