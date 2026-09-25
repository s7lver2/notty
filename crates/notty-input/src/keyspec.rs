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
        _ => None,
    }
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
    fn empty_string_is_none() {
        assert_eq!(parse_key_spec(""), None);
    }
}
