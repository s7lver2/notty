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
/// `disabled` quita secuencias sueltas (Ajustes → Ligaduras, interruptor de cada una).
pub fn resolve(overrides: &std::collections::BTreeMap<String, String>, disabled: &[String]) -> Vec<(String, char)> {
    let mut table: Vec<(String, char)> =
        entries(overrides, disabled).into_iter().filter(|e| e.enabled).map(|e| (e.seq, e.glyph)).collect();
    table.sort_by_key(|(s, _)| std::cmp::Reverse(s.chars().count()));
    table
}

/// Una fila de la cuadrícula de Ajustes → Ligaduras.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub seq: String,
    pub glyph: char,
    /// De `BUILTIN` (aunque `overrides` le haya cambiado el carácter).
    pub builtin: bool,
    pub enabled: bool,
}

/// Todas las ligaduras en el orden en que se enseñan: primero las de serie, luego
/// las propias por orden alfabético.
pub fn entries(overrides: &std::collections::BTreeMap<String, String>, disabled: &[String]) -> Vec<Entry> {
    let mut out: Vec<Entry> =
        BUILTIN.iter().map(|(s, c)| Entry { seq: s.to_string(), glyph: *c, builtin: true, enabled: true }).collect();
    for (seq, repl) in overrides {
        let Some(c) = repl.chars().next() else { continue };
        match out.iter_mut().find(|e| e.seq == *seq) {
            Some(e) => e.glyph = c,
            None => out.push(Entry { seq: seq.clone(), glyph: c, builtin: false, enabled: true }),
        }
    }
    for e in &mut out {
        e.enabled = !disabled.iter().any(|d| *d == e.seq);
    }
    out
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
        resolve(&Default::default(), &[])
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
        let table = resolve(&overrides, &[]);
        let out = display_text("a ~> b", &table);
        assert!(out.contains('↝'));
    }

    #[test]
    fn disabled_sequences_are_left_out() {
        let table = resolve(&Default::default(), &["->".to_string()]);
        assert_eq!(display_text("a -> b", &table), "a -> b");
        assert!(display_text("a => b", &table).contains('⇒'));
    }

    #[test]
    fn entries_mark_builtins_and_custom_ones() {
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert("|>".to_string(), "▷".to_string());
        overrides.insert("->".to_string(), "➜".to_string());
        let e = entries(&overrides, &["|>".to_string()]);
        assert_eq!(e.len(), BUILTIN.len() + 1);
        let arrow = e.iter().find(|x| x.seq == "->").unwrap();
        assert!(arrow.builtin && arrow.enabled && arrow.glyph == '➜');
        let pipe = e.last().unwrap();
        assert!(!pipe.builtin && !pipe.enabled && pipe.glyph == '▷');
    }

    #[test]
    fn override_can_replace_a_builtin_symbol() {
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert("->".to_string(), "➜".to_string());
        let table = resolve(&overrides, &[]);
        let out = display_text("a -> b", &table);
        assert!(out.contains('➜'));
        assert!(!out.contains('→'));
    }
}
