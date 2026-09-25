pub fn hex_row(offset: usize, bytes: &[u8]) -> String {
    let mut hex = String::new();
    for (i, b) in bytes.iter().enumerate() {
        hex.push_str(&format!("{b:02X} "));
        if i == 7 {
            hex.push(' ');
        }
    }
    for i in bytes.len()..16 {
        hex.push_str("   ");
        if i == 7 {
            hex.push(' ');
        }
    }
    let ascii: String = bytes.iter().map(|&b| if (0x20..=0x7E).contains(&b) { b as char } else { '.' }).collect();
    format!("{offset:08x}   {hex} {ascii}")
}

/// Divide una secuencia de bytes en filas de 16 y las formatea. Función interna
/// reutilizada por `hex_rows`, testeable sin necesidad de un `RawDoc`/archivo real.
fn hex_rows_from_bytes(bytes: &[u8]) -> Vec<String> {
    bytes.chunks(16).enumerate().map(|(i, chunk)| hex_row(i * 16, chunk)).collect()
}

pub fn hex_rows(doc: &crate::RawDoc) -> Vec<String> {
    let all: Vec<u8> = (0..doc.len()).map(|i| doc.byte(i)).collect();
    hex_rows_from_bytes(&all)
}

pub fn hex_char(c: char) -> Option<u8> {
    c.to_digit(16).map(|d| d as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_a_short_row_with_ascii() {
        let row = hex_row(0, b"MZ\x90\x00");
        assert!(row.starts_with("00000000"));
        assert!(row.contains("4D 5A 90 00"));
        assert!(row.ends_with("MZ.."));
    }

    #[test]
    fn non_printable_bytes_become_dots() {
        let row = hex_row(0, &[0, 1, 65]);
        assert!(row.ends_with("..A"));
    }

    #[test]
    fn offset_is_eight_hex_digits() {
        let row = hex_row(0x1234, b"a");
        assert!(row.starts_with("00001234"));
    }

    #[test]
    fn hex_rows_splits_into_blocks_of_sixteen() {
        let bytes: Vec<u8> = (0..20).collect();
        let rows = hex_rows_from_bytes(&bytes);
        assert_eq!(rows.len(), 2);
        assert!(rows[1].starts_with("00000010"));
    }

    #[test]
    fn hex_char_parses_digits_and_letters() {
        assert_eq!(hex_char('0'), Some(0));
        assert_eq!(hex_char('9'), Some(9));
        assert_eq!(hex_char('a'), Some(10));
        assert_eq!(hex_char('F'), Some(15));
        assert_eq!(hex_char('g'), None);
    }
}
