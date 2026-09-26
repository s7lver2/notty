#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// Traduce "Ctrl+Shift+H", "F3", "Ctrl+," etc. a (código de tecla virtual, modificadores).
/// Usa los mismos códigos VK_* que `notty_ui::keymap`.
pub fn parse_key_spec(s: &str) -> Option<(u32, Modifiers)> {
    if s.is_empty() {
        return None;
    }
    let mut m = Modifiers::default();
    let mut key = None;
    for part in s.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "" => continue,
            "ctrl" => m.ctrl = true,
            "shift" => m.shift = true,
            "alt" => m.alt = true,
            other => key = Some(key_code(other, part)?),
        }
    }
    key.map(|vk| (vk, m))
}

fn key_code(lower: &str, original: &str) -> Option<u32> {
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=12).contains(&n) {
            return Some(0x6F + n);
        }
    }
    if original.len() == 1 {
        let c = original.chars().next()?;
        return match c.to_ascii_uppercase() {
            'A'..='Z' => Some(c.to_ascii_uppercase() as u32),
            '0'..='9' => Some(c as u32),
            ',' => Some(0xBC),
            '.' => Some(0xBE),
            '-' => Some(0xBD),
            '=' => Some(0xBB),
            _ => None,
        };
    }
    match lower {
        "tab" => Some(0x09),
        "enter" | "return" => Some(0x0D),
        "esc" | "escape" => Some(0x1B),
        "space" => Some(0x20),
        "home" => Some(0x24),
        "end" => Some(0x23),
        "left" => Some(0x25),
        "right" => Some(0x27),
        "up" => Some(0x26),
        "down" => Some(0x28),
        "backspace" => Some(0x08),
        "delete" | "del" => Some(0x2E),
        "insert" | "ins" => Some(0x2D),
        "pageup" | "pgup" => Some(0x21),
        "pagedown" | "pgdn" => Some(0x22),
        _ => None,
    }
}

/// Inverso de `parse_key_spec`: "Ctrl+Shift+N". `None` si la tecla no tiene nombre
/// en `[keys]` (o es un modificador suelto), para no guardar algo que luego no se lee.
pub fn format_key_spec(vk: u32, m: Modifiers) -> Option<String> {
    let key = key_name(vk)?;
    let mut s = String::new();
    if m.ctrl {
        s.push_str("Ctrl+");
    }
    if m.alt {
        s.push_str("Alt+");
    }
    if m.shift {
        s.push_str("Shift+");
    }
    s.push_str(&key);
    Some(s)
}

/// Nombre de la tecla `vk` tal y como lo escribe `format_key_spec` ("N", "F3", "Tab").
pub fn key_name(vk: u32) -> Option<String> {
    let name = match vk {
        0x41..=0x5A | 0x30..=0x39 => return char::from_u32(vk).map(|c| c.to_string()),
        0x70..=0x7B => return Some(format!("F{}", vk - 0x6F)),
        0xBC => ",",
        0xBE => ".",
        0xBD => "-",
        0xBB => "=",
        0x09 => "Tab",
        0x0D => "Enter",
        0x1B => "Esc",
        0x20 => "Space",
        0x24 => "Home",
        0x23 => "End",
        0x25 => "Left",
        0x27 => "Right",
        0x26 => "Up",
        0x28 => "Down",
        0x08 => "Backspace",
        0x2E => "Delete",
        0x2D => "Insert",
        0x21 => "PageUp",
        0x22 => "PageDown",
        _ => return None,
    };
    Some(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(ctrl: bool, shift: bool, alt: bool) -> Modifiers {
        Modifiers { ctrl, shift, alt }
    }

    #[test]
    fn parses_single_letter_with_ctrl() {
        assert_eq!(parse_key_spec("Ctrl+S"), Some((0x53, m(true, false, false))));
    }

    #[test]
    fn parses_three_modifiers() {
        assert_eq!(parse_key_spec("Ctrl+Alt+V"), Some((0x56, m(true, false, true))));
    }

    #[test]
    fn is_case_insensitive() {
        assert_eq!(parse_key_spec("ctrl+shift+h"), parse_key_spec("CTRL+SHIFT+H"));
    }

    #[test]
    fn parses_function_keys() {
        assert_eq!(parse_key_spec("F3"), Some((0x72, m(false, false, false))));
        assert_eq!(parse_key_spec("Shift+F3"), Some((0x72, m(false, true, false))));
    }

    #[test]
    fn parses_comma_key() {
        assert_eq!(parse_key_spec("Ctrl+,"), Some((0xBC, m(true, false, false))));
    }

    #[test]
    fn parses_tab_key() {
        assert_eq!(parse_key_spec("Ctrl+Tab"), Some((0x09, m(true, false, false))));
    }

    #[test]
    fn unknown_key_name_is_none() {
        assert_eq!(parse_key_spec("Ctrl+Nope"), None);
    }

    #[test]
    fn format_round_trips_through_parse() {
        for spec in [
            "Ctrl+N", "Ctrl+Shift+Tab", "Ctrl+Alt+V", "Ctrl+,", "F3", "Shift+F12", "Alt+PageDown", "Ctrl+-", "Ctrl+=",
            "Ctrl+0",
        ] {
            let (vk, m) = parse_key_spec(spec).unwrap();
            assert_eq!(format_key_spec(vk, m).as_deref(), Some(spec));
        }
    }

    #[test]
    fn format_rejects_unnamed_keys() {
        assert_eq!(format_key_spec(0x11, m(true, false, false)), None);
        assert_eq!(format_key_spec(0xDE, m(true, false, false)), None);
    }

    #[test]
    fn empty_string_is_none() {
        assert_eq!(parse_key_spec(""), None);
    }
}
