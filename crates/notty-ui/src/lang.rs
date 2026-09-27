//! Idioma efectivo de la interfaz (Ajustes → Idioma): `Lang::Auto` sigue el de
//! Windows la primera vez que arranca notty; `Es`/`En` lo fuerzan. Solo hay dos
//! idiomas de verdad (`strings::t` no sabe qué es `Auto`): `resolve` es el único
//! sitio que decide a cuál cae `Auto`.

use notty_config::Lang;

/// Idioma de Windows para la sesión actual del usuario (`GetUserDefaultLocaleName`,
/// p.ej. "es-ES" o "en-US"): español si empieza por "es", inglés en cualquier otro
/// caso (falta de esa API, o cualquier otro idioma sin traducción todavía).
pub fn detect_system_lang() -> Lang {
    let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
    let len = unsafe { windows::Win32::Globalization::GetUserDefaultLocaleName(&mut buf) };
    if len <= 0 {
        return Lang::En;
    }
    let name = String::from_utf16_lossy(&buf[..(len as usize - 1).min(buf.len())]);
    if name.to_lowercase().starts_with("es") { Lang::Es } else { Lang::En }
}

/// `Auto` resuelto a un idioma de verdad; `Es`/`En` se devuelven tal cual.
pub fn resolve(lang: Lang) -> Lang {
    match lang {
        Lang::Auto => detect_system_lang(),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_langs_resolve_to_themselves() {
        assert_eq!(resolve(Lang::Es), Lang::Es);
        assert_eq!(resolve(Lang::En), Lang::En);
    }

    #[test]
    fn auto_never_resolves_to_auto() {
        assert_ne!(resolve(Lang::Auto), Lang::Auto);
    }
}
