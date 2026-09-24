//! notty-io: detección texto/raw, codificaciones, fin de línea y guardado atómico.

mod encoding;
mod eol;

pub use encoding::{CodecError, Detected, TextEncoding, decode, decode_lossy, detect, encode};
pub use eol::{LineEnding, convert, detect_eol};
