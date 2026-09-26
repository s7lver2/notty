//! Sustitución visual de secuencias de texto por un carácter (p.ej. "->" por "→"):
//! solo cambia lo que se dibuja, nunca el documento. Ajustes → Apariencia →
//! "Ligaduras" activa/desactiva el conjunto fijo de abajo; `Config::ligature_overrides`
//! deja añadir o pisar entradas propias (ver `resolve`).

/// Conjunto fijo, de más a menos larga (importa el orden: `resolve` prueba primero
/// las más largas para que una secuencia no tape a otra que la contiene).
pub const BUILTIN: &[(&str, char)] = &[
    ("->", '→'),
    ("<-", '←'),
    ("=>", '⇒'),
    ("<=", '≤'),
    (">=", '≥'),
    ("!=", '≠'),
    ("::", '∷'),
];

/// Mezcla `BUILTIN` con `overrides` (que puede añadir secuencias nuevas o pisar una
/// ya existente con otro carácter), ordenado por longitud de secuencia descendente
/// para que `display_text` compare primero las más largas. Solo toma el primer
/// carácter de cada valor de `overrides`: no tiene sentido "sustituir por más de un
/// carácter" cuando el objetivo es no descuadrar las columnas del texto monoespaciado.
pub fn resolve(overrides: &std::collections::BTreeMap<String, String>) -> Vec<(String, char)> {
    let mut table: Vec<(String, char)> = BUILTIN.iter().map(|(s, c)| (s.to_string(), *c)).collect();
    for (seq, repl) in overrides {
        let Some(c) = repl.chars().next() else { continue };
        match table.iter_mut().find(|(s, _)| s == seq) {
            Some(entry) => entry.1 = c,
            None => table.push((seq.clone(), c)),
        }
    }
    table.sort_by_key(|(s, _)| std::cmp::Reverse(s.chars().count()));
    table
}

/// `text` con cada secuencia de `table` sustituida por su carácter, seguido de
/// tantos espacios como caracteres le sobren a la secuencia original: así el
/// carácter que viene después de la sustitución cae en la misma columna que antes
/// (clave para no descuadrar el cursor/selección, que siguen midiendo en caracteres
/// del texto real, no de esta versión "de pantalla").
pub fn display_text(text: &str, table: &[(String, char)]) -> String {
    if table.is_empty() {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    'outer: while i < chars.len() {
        for (seq, repl) in table {
            let n = seq.chars().count();
            if n > 0 && i + n <= chars.len() && chars[i..i + n].iter().collect::<String>() == *seq {
                out.push(*repl);
                out.extend(std::iter::repeat_n(' ', n - 1));
                i += n;
                continue 'outer;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin_table() -> Vec<(String, char)> {
        resolve(&Default::default())
    }

    #[test]
    fn replaces_arrow_and_keeps_column_count() {
        let out = display_text("a -> b", &builtin_table());
        assert_eq!(out.chars().count(), "a -> b".chars().count());
        assert!(out.contains('→'));
    }

    #[test]
    fn longest_match_wins_over_a_shorter_prefix() {
        // "!=" no debe partirse en "!" + "=" sueltos si hubiera una entrada de un
        // carácter que empezara igual; con el conjunto fijo de hoy ya se prueba solo
        // con que salga el símbolo correcto y no, por ejemplo, dos sustituciones.
        let out = display_text("a != b", &builtin_table());
        assert_eq!(out.chars().filter(|c| *c == '≠').count(), 1);
    }

    #[test]
    fn text_without_any_sequence_is_unchanged() {
        let out = display_text("fn main() {}", &builtin_table());
        assert_eq!(out, "fn main() {}");
    }

    #[test]
    fn empty_table_returns_the_original_text() {
        assert_eq!(display_text("a -> b", &[]), "a -> b");
    }

    #[test]
    fn override_can_add_a_new_sequence() {
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert("~>".to_string(), "↝".to_string());
        let table = resolve(&overrides);
        let out = display_text("a ~> b", &table);
        assert!(out.contains('↝'));
    }

    #[test]
    fn override_can_replace_a_builtin_symbol() {
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert("->".to_string(), "➜".to_string());
        let table = resolve(&overrides);
        let out = display_text("a -> b", &table);
        assert!(out.contains('➜'));
        assert!(!out.contains('→'));
    }
}
