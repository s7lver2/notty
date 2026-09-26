//! Historial de rutas usadas (abiertas o guardadas desde la línea de ruta), para que
//! las sugerencias pongan primero lo que se usa a menudo y hace poco.
//! Formato de `%APPDATA%\notty\path_usage.txt`: `veces<TAB>unix_segundos<TAB>ruta`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ENTRIES: usize = 500;

#[derive(Debug, Clone, Default)]
pub struct Usage {
    entries: Vec<(String, u32, u64)>,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn norm(p: &str) -> String {
    p.trim_end_matches('\\').to_lowercase()
}

pub fn usage_path() -> PathBuf {
    match std::env::var_os("APPDATA") {
        Some(a) => PathBuf::from(a).join("notty").join("path_usage.txt"),
        None => PathBuf::from("notty-path-usage.txt"),
    }
}

impl Usage {
    pub fn load_from(file: &Path) -> Usage {
        let Ok(text) = std::fs::read_to_string(file) else { return Usage::default() };
        let entries = text
            .lines()
            .filter_map(|l| {
                let mut it = l.splitn(3, '\t');
                let count = it.next()?.parse().ok()?;
                let last = it.next()?.parse().ok()?;
                let path = it.next()?.to_string();
                Some((path, count, last))
            })
            .collect();
        Usage { entries }
    }

    pub fn load() -> Usage {
        Self::load_from(&usage_path())
    }

    pub fn record(&mut self, path: &Path, now: u64) {
        let p = path.display().to_string();
        let key = norm(&p);
        match self.entries.iter_mut().find(|(e, _, _)| norm(e) == key) {
            Some(e) => {
                e.1 = e.1.saturating_add(1);
                e.2 = now;
            }
            None => self.entries.push((p, 1, now)),
        }
        if self.entries.len() > MAX_ENTRIES {
            self.entries.sort_by_key(|e| std::cmp::Reverse(e.2));
            self.entries.truncate(MAX_ENTRIES);
        }
    }

    pub fn save_to(&self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text: String = self.entries.iter().map(|(p, c, t)| format!("{c}\t{t}\t{p}\n")).collect();
        std::fs::write(file, text)
    }

    /// Puntuación de `candidate` (archivo o carpeta): cada uso de esa ruta, o de algo
    /// dentro de ella si es carpeta, suma, y pesa menos cuanto más antiguo es.
    pub fn score(&self, candidate: &str, now: u64) -> f64 {
        let key = norm(candidate);
        let prefix = format!("{key}\\");
        self.entries
            .iter()
            .filter(|(p, _, _)| {
                let n = norm(p);
                n == key || n.starts_with(&prefix)
            })
            .map(|(_, count, last)| {
                let age_days = now.saturating_sub(*last) as f64 / 86_400.0;
                *count as f64 / (1.0 + age_days / 7.0)
            })
            .sum()
    }
}

/// Apunta un uso de `path` en el historial en disco. Si falla, no pasa nada: solo
/// se pierde un poco de orden en las sugerencias.
pub fn record_path_use(path: &Path) {
    let file = usage_path();
    let mut u = Usage::load_from(&file);
    u.record(path, now_secs());
    let _ = u.save_to(&file);
}

pub(crate) fn current_secs() -> u64 {
    now_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_counts_and_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("u.txt");
        let mut u = Usage::default();
        u.record(Path::new(r"C:\a\b.txt"), 100);
        u.record(Path::new(r"c:\A\B.TXT"), 200);
        u.save_to(&file).unwrap();
        let back = Usage::load_from(&file);
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0].1, 2);
        assert_eq!(back.entries[0].2, 200);
    }

    #[test]
    fn folders_score_from_files_inside_and_recent_beats_old() {
        let mut u = Usage::default();
        let now = 1_000_000_000;
        u.record(Path::new(r"C:\proyectos\x.txt"), now);
        u.record(Path::new(r"C:\viejo\y.txt"), now - 86_400 * 365);
        assert!(u.score(r"C:\proyectos", now) > u.score(r"C:\viejo", now));
        assert_eq!(u.score(r"C:\nada", now), 0.0);
        // "C:\pro" no es un prefijo de carpeta de "C:\proyectos".
        assert_eq!(u.score(r"C:\pro", now), 0.0);
    }
}
