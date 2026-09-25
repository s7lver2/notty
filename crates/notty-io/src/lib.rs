//! notty-io: detección texto/raw, codificaciones, fin de línea y guardado atómico.

mod encoding;
mod eol;
mod fsutil;
mod open;
mod pathline;

pub use encoding::{CodecError, Detected, TextEncoding, decode, decode_lossy, detect, encode};
pub use eol::{LineEnding, convert, detect_eol};
pub use fsutil::{atomic_write, can_write, create_parent_dirs};
pub use open::{Opened, RawBytes, open, open_raw};
