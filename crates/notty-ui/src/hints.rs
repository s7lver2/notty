/// Texto de la barra de atajos estilo nano. `raw` tiene prioridad sobre `vim`
/// porque la vista raw usa su propio conjunto de teclas, sea cual sea el modo de texto.
pub fn hints_text(vim: bool, raw: bool) -> &'static str {
    if raw {
        "^S Guardar   ^Shift+H Ver como texto   ←→↑↓ Moverse"
    } else if vim {
        "i Insertar   hjkl Moverse   /Buscar   :w Guardar   ^Alt+V Salir de vim"
    } else {
        "^S Guardar   ^F Buscar   ^H Reemplazar   ^O Abrir   ^Alt+V Vim"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_mode_hints() {
        assert!(hints_text(false, false).contains("^S Guardar"));
        assert!(hints_text(false, false).contains("^Alt+V Vim"));
    }

    #[test]
    fn raw_mode_hints_mention_moving_bytes() {
        assert!(hints_text(false, true).contains("Ver como texto"));
    }

    #[test]
    fn vim_mode_hints_mention_insert() {
        assert!(hints_text(true, false).contains("Insertar"));
    }

    #[test]
    fn raw_takes_priority_over_vim() {
        assert_eq!(hints_text(true, true), hints_text(false, true));
    }
}
