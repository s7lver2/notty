use std::borrow::Cow;

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Windows1252,
}

impl TextEncoding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 con BOM",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::Windows1252 => "ANSI (1252)",
        }
    }

    fn bom(self) -> &'static [u8] {
        match self {
            Self::Utf8Bom => &[0xEF, 0xBB, 0xBF],
            Self::Utf16Le => &[0xFF, 0xFE],
            Self::Utf16Be => &[0xFE, 0xFF],
            Self::Utf8 | Self::Windows1252 => &[],
        }
    }

    fn coder(self) -> &'static Encoding {
        match self {
            Self::Utf8 | Self::Utf8Bom => UTF_8,
            Self::Utf16Le => UTF_16LE,
            Self::Utf16Be => UTF_16BE,
            Self::Windows1252 => WINDOWS_1252,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detected {
    Text(TextEncoding),
    Raw,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodecError {
    #[error("los bytes no son {0} válido")]
    Invalid(&'static str),
    #[error("el texto tiene caracteres que no caben en {0}")]
    Unmappable(&'static str),
    #[error("error de E/S: {0}")]
    Io(String),
}

/// Decide texto o raw a partir de los primeros bytes del archivo (~8 KB).
/// Texto: BOM, o UTF-8 válido sin bytes nulos. Todo lo demás: raw.
pub fn detect(sample: &[u8]) -> Detected {
    use TextEncoding::*;
    if sample.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Detected::Text(Utf8Bom);
    }
    if sample.starts_with(&[0xFF, 0xFE]) {
        return Detected::Text(Utf16Le);
    }
    if sample.starts_with(&[0xFE, 0xFF]) {
        return Detected::Text(Utf16Be);
    }
    if sample.contains(&0) {
        return Detected::Raw;
    }
    match std::str::from_utf8(sample) {
        Ok(_) => Detected::Text(Utf8),
        // La muestra puede cortar un carácter multibyte por la mitad.
        Err(e) if e.error_len().is_none() => Detected::Text(Utf8),
        Err(_) => Detected::Raw,
    }
}

/// Decodificación estricta: si un solo byte no encaja, error. Quita el BOM.
pub fn decode(bytes: &[u8], enc: TextEncoding) -> Result<String, CodecError> {
    let body = bytes.strip_prefix(enc.bom()).unwrap_or(bytes);
    enc.coder()
        .decode_without_bom_handling_and_without_replacement(body)
        .map(Cow::into_owned)
        .ok_or(CodecError::Invalid(enc.label()))
}

/// Solo para abrir en modo lectura cuando `decode` falla: cambia lo inválido por U+FFFD.
pub fn decode_lossy(bytes: &[u8], enc: TextEncoding) -> String {
    let body = bytes.strip_prefix(enc.bom()).unwrap_or(bytes);
    enc.coder().decode_without_bom_handling(body).0.into_owned()
}

pub fn encode(text: &str, enc: TextEncoding) -> Result<Vec<u8>, CodecError> {
    let mut out = enc.bom().to_vec();
    match enc {
        TextEncoding::Utf8 | TextEncoding::Utf8Bom => out.extend_from_slice(text.as_bytes()),
        // encoding_rs no codifica a UTF-16: se hace a mano.
        TextEncoding::Utf16Le => text.encode_utf16().for_each(|u| out.extend_from_slice(&u.to_le_bytes())),
        TextEncoding::Utf16Be => text.encode_utf16().for_each(|u| out.extend_from_slice(&u.to_be_bytes())),
        TextEncoding::Windows1252 => {
            let (bytes, _, unmappable) = WINDOWS_1252.encode(text);
            if unmappable {
                return Err(CodecError::Unmappable(enc.label()));
            }
            out.extend_from_slice(&bytes);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use TextEncoding::*;

    #[test]
    fn detects_boms() {
        assert_eq!(detect(&[0xEF, 0xBB, 0xBF, b'a']), Detected::Text(Utf8Bom));
        assert_eq!(detect(&[0xFF, 0xFE, b'a', 0]), Detected::Text(Utf16Le));
        assert_eq!(detect(&[0xFE, 0xFF, 0, b'a']), Detected::Text(Utf16Be));
    }

    #[test]
    fn plain_utf8_is_text() {
        assert_eq!(detect("hola ñ".as_bytes()), Detected::Text(Utf8));
    }

    #[test]
    fn empty_is_text() {
        assert_eq!(detect(&[]), Detected::Text(Utf8));
    }

    #[test]
    fn nul_bytes_mean_raw() {
        assert_eq!(detect(&[b'a', 0, b'b']), Detected::Raw);
    }

    #[test]
    fn invalid_utf8_means_raw() {
        assert_eq!(detect(&[0xC3, 0x28]), Detected::Raw);
    }

    #[test]
    fn utf8_cut_at_sample_end_is_still_text() {
        let s = "añ".as_bytes();
        assert_eq!(detect(&s[..2]), Detected::Text(Utf8));
    }

    #[test]
    fn round_trips_every_encoding() {
        for enc in [Utf8, Utf8Bom, Utf16Le, Utf16Be, Windows1252] {
            let bytes = encode("Año €", enc).unwrap();
            assert_eq!(decode(&bytes, enc).unwrap(), "Año €", "{}", enc.label());
        }
    }

    #[test]
    fn boms_are_written() {
        assert_eq!(encode("a", Utf8Bom).unwrap(), vec![0xEF, 0xBB, 0xBF, b'a']);
        assert_eq!(encode("a", Utf16Le).unwrap(), vec![0xFF, 0xFE, b'a', 0]);
        assert_eq!(encode("a", Utf16Be).unwrap(), vec![0xFE, 0xFF, 0, b'a']);
    }

    #[test]
    fn strict_decode_rejects_bad_bytes() {
        assert_eq!(decode(&[0xC3, 0x28], Utf8), Err(CodecError::Invalid("UTF-8")));
    }

    #[test]
    fn lossy_decode_replaces() {
        assert_eq!(decode_lossy(&[b'a', 0xFF], Utf8), "a\u{FFFD}");
    }

    #[test]
    fn unmappable_in_1252_is_an_error() {
        assert_eq!(encode("日", Windows1252), Err(CodecError::Unmappable("ANSI (1252)")));
    }
}
