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
}
