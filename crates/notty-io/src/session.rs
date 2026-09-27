//! Sesión anterior (Ajustes → Archivos → "Reabrir archivos anteriores"): qué rutas
//! quedaron abiertas la última vez, para reabrirlas juntas al arrancar sin ningún
//! archivo en la línea de comandos. Formato de `%APPDATA%\notty\session.txt`: una
//! ruta por línea, en el orden de las pestañas; la primera línea es la que estaba
//! activa. Solo se guardan documentos con ruta: uno sin guardar nunca (temporal, o
//! "sin título" recién creado) no tiene nada que reabrir.

use std::path::{Path, PathBuf};

pub fn session_path() -> PathBuf {
    match std::env::var_os("APPDATA") {
        Some(a) => PathBuf::from(a).join("notty").join("session.txt"),
        None => PathBuf::from("notty-session.txt"),
    }
}

/// Rutas a reabrir, en orden; la primera es la que estaba activa. Vacío si no hay
/// sesión guardada.
pub fn load_session(file: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    text.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect()
}

/// Guarda qué documentos con ruta quedaron abiertos (`paths`, en el orden de sus
/// pestañas) y cuál de ellos estaba activo (se reordena al principio del archivo).
/// Sin ninguna ruta, borra la sesión anterior en vez de dejar un archivo vacío. Si
/// falla al escribir, no pasa nada: solo se pierde la sesión la próxima vez.
pub fn save_session(paths: &[PathBuf], active: usize, file: &Path) {
    if paths.is_empty() {
        let _ = std::fs::remove_file(file);
        return;
    }
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut ordered: Vec<&PathBuf> = Vec::with_capacity(paths.len());
    ordered.extend(paths.get(active));
    ordered.extend(paths.iter().enumerate().filter(|(i, _)| *i != active).map(|(_, p)| p));
    let text: String = ordered.iter().map(|p| format!("{}\n", p.display())).collect();
    let _ = std::fs::write(file, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_with_the_active_path_first() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("session.txt");
        let paths = vec![PathBuf::from(r"C:\a.txt"), PathBuf::from(r"C:\b.txt"), PathBuf::from(r"C:\c.txt")];
        save_session(&paths, 1, &file);
        assert_eq!(load_session(&file), vec![PathBuf::from(r"C:\b.txt"), PathBuf::from(r"C:\a.txt"), PathBuf::from(r"C:\c.txt")]);
    }

    #[test]
    fn empty_paths_delete_a_previous_session() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("session.txt");
        save_session(&[PathBuf::from(r"C:\a.txt")], 0, &file);
        assert!(file.exists());
        save_session(&[], 0, &file);
        assert!(!file.exists());
    }

    #[test]
    fn missing_file_loads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_session(&dir.path().join("no-existe.txt")).is_empty());
    }
}
