use std::path::PathBuf;

pub struct PathContext {
    pub home: PathBuf,
    pub current_dir: Option<PathBuf>,
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Normaliza lo que el usuario va escribiendo en la línea de ruta: separadores,
/// espacios sobrantes, `~`, `.` (carpeta del archivo actual) y `%VARIABLES%`.
pub fn normalize(raw: &str, ctx: &PathContext) -> String {
    let mut s = raw.trim_start().replace('/', "\\");
    while s.contains(r"\\") {
        s = s.replace(r"\\", r"\");
    }
    if s == "." || s.starts_with(r".\") {
        if let Some(dir) = &ctx.current_dir {
            let rest = s.strip_prefix('.').unwrap_or("");
            s = format!("{}{}", dir.display(), rest);
        }
    } else if s == "~" || s.starts_with(r"~\") {
        let rest = s.strip_prefix('~').unwrap_or("");
        s = format!("{}{}", ctx.home.display(), rest);
    }
    expand_env(&s)
}

fn expand_env(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(val) => out.push_str(&val),
                    Err(_) => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

use std::path::Path;

pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

fn split_last(typed: &str) -> (&str, &str) {
    match typed.rfind('\\') {
        Some(i) => (&typed[..i], &typed[i + 1..]),
        None => ("", typed),
    }
}

/// `true` si `last` es un patrón (`*.py`, `notas?.txt`) en vez de un prefijo literal.
pub fn is_glob(last: &str) -> bool {
    last.contains('*') || last.contains('?')
}

/// Coincidencia de comodines estilo shell: `*` = cualquier secuencia (incluida
/// vacía), `?` = un carácter cualquiera. Insensible a mayúsculas/minúsculas (se
/// espera que `pattern`/`text` ya vengan en minúsculas, como hace `suggestions`).
/// Por carácter (no por byte), para no romper nombres con acentos.
fn glob_match(pattern: &[char], text: &[char]) -> bool {
    match (pattern.first(), text.first()) {
        (None, None) => true,
        (Some('*'), _) => glob_match(&pattern[1..], text) || (!text.is_empty() && glob_match(pattern, &text[1..])),
        (Some('?'), Some(_)) => glob_match(&pattern[1..], &text[1..]),
        (Some(p), Some(t)) if p == t => glob_match(&pattern[1..], &text[1..]),
        _ => false,
    }
}

pub fn suggestions(typed: &str, max: usize) -> Vec<Entry> {
    ranked_suggestions(typed, max, &crate::usage::Usage::default())
}

/// Como `suggestions`, pero lo más usado (y reciente) según `usage` va primero; el
/// resto sigue en el orden de siempre (carpetas primero, luego por nombre).
pub fn ranked_suggestions(typed: &str, max: usize, usage: &crate::usage::Usage) -> Vec<Entry> {
    // Sin nada tras la última `\` se listan todas las entradas de la carpeta: no hace
    // falta teclear una letra para empezar a ver sugerencias.
    let (parent, last) = split_last(typed);
    if parent.is_empty() {
        return Vec::new();
    }
    // Con la `\` final: `read_dir("C:")` leería la carpeta actual de C:, no la raíz.
    let Ok(read) = std::fs::read_dir(&typed[..=parent.len()]) else { return Vec::new() };
    let last_lower = last.to_lowercase();
    let glob = is_glob(&last_lower);
    let pattern: Vec<char> = last_lower.chars().collect();
    let mut entries: Vec<Entry> = read
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let name_lower = name.to_lowercase();
            let matches = if glob {
                glob_match(&pattern, &name_lower.chars().collect::<Vec<_>>())
            } else {
                name_lower.starts_with(&last_lower)
            };
            matches.then(|| {
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                Entry { name, is_dir }
            })
        })
        .collect();
    let now = crate::usage::current_secs();
    let dir = &typed[..=parent.len()];
    let mut scored: Vec<(f64, Entry)> =
        entries.drain(..).map(|e| (usage.score(&format!("{dir}{}", e.name), now), e)).collect();
    scored.sort_by(|(sa, a), (sb, b)| {
        sb.partial_cmp(sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.is_dir.cmp(&a.is_dir))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    scored.truncate(max);
    scored.into_iter().map(|(_, e)| e).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    Empty,
    New,
    Exists,
    Dir,
    DirNew,
}

pub fn hint_for(typed: &str) -> Hint {
    if typed.is_empty() {
        return Hint::Empty;
    }
    let path = Path::new(typed);
    if let Ok(meta) = std::fs::metadata(path) {
        return if meta.is_dir() { Hint::Dir } else { Hint::Exists };
    }
    let (parent, _) = split_last(typed);
    if parent.is_empty() || Path::new(parent).is_dir() { Hint::New } else { Hint::DirNew }
}

pub const INVALID_CHARS: &[char] = &[':', '*', '?', '"', '<', '>', '|'];

pub fn has_invalid_chars(segment: &str) -> bool {
    // Una unidad tipo "C:" es válida: solo cuenta como inválido el ':' cuando no
    // es el segundo carácter de un designador de unidad de una sola letra.
    if segment.len() == 2 && segment.ends_with(':') && segment.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    segment.chars().any(|c| INVALID_CHARS.contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx() -> PathContext {
        PathContext { home: PathBuf::from(r"C:\Users\ana"), current_dir: Some(PathBuf::from(r"C:\Users\ana\Documentos\notty")) }
    }

    #[test]
    fn converts_forward_slashes() {
        assert_eq!(normalize("Documentos/proyectos", &ctx()), r"Documentos\proyectos");
    }

    #[test]
    fn collapses_double_backslashes() {
        assert_eq!(normalize(r"Documentos\\proyectos", &ctx()), r"Documentos\proyectos");
    }

    #[test]
    fn trims_leading_spaces() {
        assert_eq!(normalize("   nota.txt", &ctx()), "nota.txt");
    }

    #[test]
    fn expands_home_tilde() {
        assert_eq!(normalize(r"~\Documentos", &ctx()), r"C:\Users\ana\Documentos");
    }

    #[test]
    fn expands_dot_to_current_dir() {
        assert_eq!(normalize(r".\config.toml", &ctx()), r"C:\Users\ana\Documentos\notty\config.toml");
    }

    #[test]
    fn expands_known_env_var() {
        // TEMP siempre existe en Windows; si el valor difiere entre máquinas, solo
        // comprobamos que ya no queda el literal "%TEMP%" en el resultado.
        let out = normalize("%TEMP%\\a.txt", &ctx());
        assert!(!out.contains("%TEMP%"));
    }

    #[test]
    fn unknown_env_var_is_left_as_is() {
        assert_eq!(normalize("%NO_EXISTE_ESTA_VAR%\\a.txt", &ctx()), r"%NO_EXISTE_ESTA_VAR%\a.txt");
    }

    use tempfile::tempdir;

    fn setup() -> tempfile::TempDir {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join("proyectos")).unwrap();
        std::fs::create_dir(dir.path().join("proyectos-viejos")).unwrap();
        std::fs::write(dir.path().join("presupuesto.txt"), "x").unwrap();
        dir
    }

    #[test]
    fn suggestions_are_prefix_filtered_dirs_first() {
        let dir = setup();
        let typed = dir.path().join("pro").to_string_lossy().to_string();
        let s = suggestions(&typed, 5);
        assert_eq!(s.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["proyectos", "proyectos-viejos"]);
        assert!(s.iter().all(|e| e.is_dir));
    }

    #[test]
    fn wildcard_star_matches_by_extension() {
        let dir = setup();
        std::fs::write(dir.path().join("a.py"), "x").unwrap();
        std::fs::write(dir.path().join("b.py"), "x").unwrap();
        std::fs::write(dir.path().join("c.txt"), "x").unwrap();
        let typed = dir.path().join("*.py").to_string_lossy().to_string();
        let s = suggestions(&typed, 10);
        assert_eq!(s.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["a.py", "b.py"]);
    }

    #[test]
    fn wildcard_question_mark_matches_one_char() {
        let dir = setup();
        std::fs::write(dir.path().join("v1.txt"), "x").unwrap();
        std::fs::write(dir.path().join("v2.txt"), "x").unwrap();
        std::fs::write(dir.path().join("v10.txt"), "x").unwrap();
        let typed = dir.path().join("v?.txt").to_string_lossy().to_string();
        let s = suggestions(&typed, 10);
        assert_eq!(s.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["v1.txt", "v2.txt"]);
    }

    #[test]
    fn wildcard_without_star_still_needs_exact_match() {
        let dir = setup();
        // "pro?" son exactamente 4 caracteres: ni "proyectos" (9) ni
        // "proyectos-viejos" encajan, aunque ambos empiecen por "pro".
        let typed = dir.path().join("pro?").to_string_lossy().to_string();
        let s = suggestions(&typed, 10);
        assert!(s.is_empty());
    }

    #[test]
    fn is_glob_detects_wildcards() {
        assert!(is_glob("*.py"));
        assert!(is_glob("v?.txt"));
        assert!(!is_glob("proyectos"));
    }

    #[test]
    fn suggestions_are_capped_at_max() {
        let dir = setup();
        let typed = dir.path().join("p").to_string_lossy().to_string();
        assert_eq!(suggestions(&typed, 2).len(), 2);
    }

    #[test]
    fn frequently_used_entries_come_first() {
        let dir = setup();
        let mut usage = crate::usage::Usage::default();
        let used = dir.path().join("presupuesto.txt");
        let now = crate::usage::current_secs();
        usage.record(&used, now);
        usage.record(&used, now);
        let typed = format!("{}\\", dir.path().display());
        let s = ranked_suggestions(&typed, 50, &usage);
        assert_eq!(s[0].name, "presupuesto.txt");
    }

    #[test]
    fn empty_last_segment_lists_the_whole_folder_dirs_first() {
        let dir = setup();
        let typed = format!("{}\\", dir.path().display());
        let s = suggestions(&typed, 50);
        assert!(!s.is_empty());
        assert!(s[0].is_dir);
        assert_eq!(s.len(), std::fs::read_dir(dir.path()).unwrap().count());
    }

    #[test]
    fn missing_parent_has_no_suggestions() {
        assert!(suggestions(r"Z:\no-existe-nunca\algo", 5).is_empty());
    }

    #[test]
    fn hint_empty_for_empty_input() {
        assert_eq!(hint_for(""), Hint::Empty);
    }

    #[test]
    fn hint_dir_for_existing_folder() {
        let dir = setup();
        assert_eq!(hint_for(&dir.path().join("proyectos").to_string_lossy()), Hint::Dir);
    }

    #[test]
    fn hint_exists_for_existing_file() {
        let dir = setup();
        assert_eq!(hint_for(&dir.path().join("presupuesto.txt").to_string_lossy()), Hint::Exists);
    }

    #[test]
    fn hint_new_when_parent_exists_but_file_does_not() {
        let dir = setup();
        assert_eq!(hint_for(&dir.path().join("nueva.txt").to_string_lossy()), Hint::New);
    }

    #[test]
    fn hint_dirnew_when_parent_is_missing_too() {
        let dir = setup();
        let typed = dir.path().join("nueva").join("sub").join("a.txt").to_string_lossy().to_string();
        assert_eq!(hint_for(&typed), Hint::DirNew);
    }

    #[test]
    fn invalid_chars_are_detected_in_last_segment_only() {
        assert!(!has_invalid_chars("C:"));
        assert!(has_invalid_chars("nota?.txt"));
        assert!(has_invalid_chars("a<b>.txt"));
        assert!(!has_invalid_chars("nota.txt"));
    }
}
