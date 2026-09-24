//! notty-io: detección texto/raw, codificaciones, fin de línea y guardado atómico.

mod encoding;

pub use encoding::{CodecError, Detected, TextEncoding, decode, decode_lossy, detect, encode};
