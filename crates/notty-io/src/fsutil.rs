use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Guardado que nunca deja el original a medias: se escribe `<nombre>.tmp~`,
/// se fuerza a disco y se renombra encima. Si algo falla, el original sigue intacto.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "la ruta no tiene nombre de archivo"))?;
    let mut tmp_name = name.to_os_string();
    tmp_name.push(".tmp~");
    let tmp = path.with_file_name(tmp_name);

    let result = (|| -> io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Crea las carpetas que falten para `path`. Devuelve true si creó alguna.
pub fn create_parent_dirs(path: &Path) -> io::Result<bool> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() && !parent.exists() => {
            fs::create_dir_all(parent)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// ¿Podemos escribir el archivo? Cubre atributo de solo lectura, permisos y bloqueos.
/// Abrir para escritura sin truncar no modifica el archivo.
pub fn can_write(path: &Path) -> bool {
    fs::OpenOptions::new().write(true).open(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn atomic_write_creates_and_overwrites() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        atomic_write(&p, b"uno").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"uno");
        atomic_write(&p, b"dos").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"dos");
        assert!(!dir.path().join("a.txt.tmp~").exists());
    }

    #[test]
    fn failed_write_keeps_target_and_leaves_no_temp_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("carpeta");
        fs::create_dir(&target).unwrap();
        assert!(atomic_write(&target, b"x").is_err());
        assert!(target.is_dir());
        assert!(!dir.path().join("carpeta.tmp~").exists());
    }

    #[test]
    fn create_parent_dirs_reports_creation() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("x").join("y").join("z.txt");
        assert!(create_parent_dirs(&p).unwrap());
        assert!(dir.path().join("x").join("y").is_dir());
        assert!(!create_parent_dirs(&p).unwrap());
    }

    #[test]
    #[allow(clippy::permissions_set_readonly_false)]
    fn can_write_detects_readonly() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        fs::write(&p, "x").unwrap();
        assert!(can_write(&p));
        let mut perms = fs::metadata(&p).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&p, perms.clone()).unwrap();
        assert!(!can_write(&p));
        perms.set_readonly(false);
        fs::set_permissions(&p, perms).unwrap();
    }
}
