use std::path::{Path, PathBuf};

pub fn recovery_dir() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(local) => PathBuf::from(local).join("notty").join("recovery"),
        None => PathBuf::from("notty-recovery"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryEntry {
    pub name: String,
    pub text: String,
}

/// Pensada para llamarse desde un `panic hook`: nunca usa `.unwrap()` y sigue
/// adelante con el resto de entradas aunque una falle.
pub fn dump_recovery(dir: &Path, entries: &[RecoveryEntry]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for e in entries {
        let safe_name: String = e.name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        let _ = std::fs::write(dir.join(format!("{safe_name}.txt")), &e.text);
    }
    Ok(())
}

pub fn list_recovery(dir: &Path) -> Vec<RecoveryEntry> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    read.flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "txt"))
        .filter_map(|e| {
            let text = std::fs::read_to_string(e.path()).ok()?;
            let name = e.path().file_stem()?.to_string_lossy().into_owned();
            Some(RecoveryEntry { name, text })
        })
        .collect()
}

pub fn clear_recovery(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn dump_then_list_round_trips() {
        let dir = tempdir().unwrap();
        let d = dir.path().join("recovery");
        let entries = vec![
            RecoveryEntry { name: "a".into(), text: "hola".into() },
            RecoveryEntry { name: "b".into(), text: "mundo".into() },
        ];
        dump_recovery(&d, &entries).unwrap();
        let mut listed = list_recovery(&d);
        listed.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].text, "hola");
        assert_eq!(listed[1].text, "mundo");
    }

    #[test]
    fn list_recovery_of_missing_dir_is_empty() {
        let dir = tempdir().unwrap();
        assert!(list_recovery(&dir.path().join("no-existe")).is_empty());
    }

    #[test]
    fn clear_recovery_removes_the_dir() {
        let dir = tempdir().unwrap();
        let d = dir.path().join("recovery");
        dump_recovery(&d, &[RecoveryEntry { name: "a".into(), text: "x".into() }]).unwrap();
        clear_recovery(&d).unwrap();
        assert!(!d.exists());
    }

    #[test]
    fn clear_recovery_of_missing_dir_is_ok() {
        let dir = tempdir().unwrap();
        assert!(clear_recovery(&dir.path().join("no-existe")).is_ok());
    }
}
