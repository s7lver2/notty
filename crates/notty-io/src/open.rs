use std::fs;
use std::io::{self, Read};
use std::ops::Deref;
use std::path::Path;

use memmap2::Mmap;

use crate::{Detected, LineEnding, TextEncoding, can_write, decode, decode_lossy, detect, detect_eol};

const SAMPLE: u64 = 8 * 1024;

/// Bytes de un archivo raw: mapeado en memoria (sin copiar) o, si está vacío, un Vec.
pub enum RawBytes {
    Mapped(Mmap),
    Owned(Vec<u8>),
}

impl Deref for RawBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            Self::Mapped(m) => m,
            Self::Owned(v) => v,
        }
    }
}

pub enum Opened {
    Text { text: String, encoding: TextEncoding, eol: LineEnding, writable: bool, lossy: bool },
    Raw { bytes: RawBytes, writable: bool },
}

pub fn open(path: &Path) -> io::Result<Opened> {
    let mut file = fs::File::open(path)?;
    let mut sample = Vec::with_capacity(SAMPLE as usize);
    Read::by_ref(&mut file).take(SAMPLE).read_to_end(&mut sample)?;
    drop(file);

    match detect(&sample) {
        Detected::Raw => open_raw(path),
        Detected::Text(encoding) => {
            let bytes = fs::read(path)?;
            let (text, lossy) = match encoding {
                // UTF-8: se valida y se reutiliza el mismo buffer, sin copiar el archivo
                // otra vez (en uno de 50 MB eran 50 MB más y ~20 ms).
                TextEncoding::Utf8 | TextEncoding::Utf8Bom => {
                    let mut bytes = bytes;
                    if encoding == TextEncoding::Utf8Bom {
                        bytes.drain(..3);
                    }
                    match String::from_utf8(bytes) {
                        Ok(text) => (text, false),
                        Err(e) => (decode_lossy(e.as_bytes(), TextEncoding::Utf8), true),
                    }
                }
                _ => match decode(&bytes, encoding) {
                    Ok(text) => (text, false),
                    Err(_) => (decode_lossy(&bytes, encoding), true),
                },
            };
            let eol = detect_eol(&text);
            let writable = !lossy && can_write(path);
            Ok(Opened::Text { text, encoding, eol, writable, lossy })
        }
    }
}

pub fn open_raw(path: &Path) -> io::Result<Opened> {
    let file = fs::File::open(path)?;
    let bytes = if file.metadata()?.len() == 0 {
        RawBytes::Owned(Vec::new())
    } else {
        // SAFETY: si otro programa trunca el archivo mientras está mapeado, leer
        // podría fallar. Es el precio de abrir binarios de GB al instante; notty
        // vigila cambios externos (Plan 6) y reabre el archivo si cambia.
        RawBytes::Mapped(unsafe { Mmap::map(&file)? })
    };
    Ok(Opened::Raw { bytes, writable: can_write(path) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn opens_utf8_text() {
        let dir = tempdir().unwrap();
        let p = write(dir.path(), "a.txt", b"hola\r\nmundo");
        match open(&p).unwrap() {
            Opened::Text { text, encoding, eol, writable, lossy } => {
                assert_eq!(text, "hola\r\nmundo");
                assert_eq!(encoding, TextEncoding::Utf8);
                assert_eq!(eol, LineEnding::Crlf);
                assert!(writable);
                assert!(!lossy);
            }
            Opened::Raw { .. } => panic!("esperaba texto"),
        }
    }

    #[test]
    fn opens_utf16_with_bom() {
        let dir = tempdir().unwrap();
        let bytes = crate::encode("hola\nñ", TextEncoding::Utf16Le).unwrap();
        let p = write(dir.path(), "u16.txt", &bytes);
        match open(&p).unwrap() {
            Opened::Text { text, encoding, eol, .. } => {
                assert_eq!(text, "hola\nñ");
                assert_eq!(encoding, TextEncoding::Utf16Le);
                assert_eq!(eol, LineEnding::Lf);
            }
            Opened::Raw { .. } => panic!("esperaba texto"),
        }
    }

    #[test]
    fn empty_file_is_text() {
        let dir = tempdir().unwrap();
        let p = write(dir.path(), "vacio.txt", b"");
        assert!(matches!(open(&p).unwrap(), Opened::Text { .. }));
    }

    #[test]
    fn binary_opens_raw_with_same_bytes() {
        let dir = tempdir().unwrap();
        let data = [0x89, b'P', b'N', b'G', 0, 0, 1];
        let p = write(dir.path(), "logo.png", &data);
        match open(&p).unwrap() {
            Opened::Raw { bytes, writable } => {
                assert_eq!(&*bytes, &data[..]);
                assert!(writable);
            }
            Opened::Text { .. } => panic!("esperaba raw"),
        }
    }

    #[test]
    fn bad_bytes_after_sample_open_lossy_and_read_only() {
        let dir = tempdir().unwrap();
        let mut data = vec![b'a'; 9000];
        data.push(0xFF);
        let p = write(dir.path(), "raro.txt", &data);
        match open(&p).unwrap() {
            Opened::Text { text, writable, lossy, .. } => {
                assert!(lossy);
                assert!(!writable);
                assert!(text.ends_with('\u{FFFD}'));
            }
            Opened::Raw { .. } => panic!("esperaba texto lossy"),
        }
    }

    #[test]
    #[allow(clippy::permissions_set_readonly_false)]
    fn readonly_file_is_not_writable() {
        let dir = tempdir().unwrap();
        let p = write(dir.path(), "ro.txt", b"x");
        let mut perms = fs::metadata(&p).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&p, perms.clone()).unwrap();
        assert!(matches!(open(&p).unwrap(), Opened::Text { writable: false, .. }));
        perms.set_readonly(false);
        fs::set_permissions(&p, perms).unwrap();
    }

    #[test]
    fn open_raw_forces_raw_view() {
        let dir = tempdir().unwrap();
        let p = write(dir.path(), "a.txt", b"hola");
        match open_raw(&p).unwrap() {
            Opened::Raw { bytes, .. } => assert_eq!(&*bytes, b"hola"),
            Opened::Text { .. } => panic!("esperaba raw"),
        }
    }

    #[test]
    fn open_raw_of_empty_file_works() {
        let dir = tempdir().unwrap();
        let p = write(dir.path(), "vacio.bin", b"");
        match open_raw(&p).unwrap() {
            Opened::Raw { bytes, .. } => assert!(bytes.is_empty()),
            Opened::Text { .. } => panic!("esperaba raw"),
        }
    }
}
