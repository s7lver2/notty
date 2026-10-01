use std::fmt;
use std::path::{Path, PathBuf};

use crate::Config;

#[derive(Debug, Clone, PartialEq)]
pub enum LoadResult {
    Loaded(Config),
    Defaulted(Config, String),
    Missing(Config),
}

impl fmt::Display for LoadResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loaded(_) => write!(f, "Loaded"),
            Self::Defaulted(_, msg) => write!(f, "Defaulted({msg})"),
            Self::Missing(_) => write!(f, "Missing"),
        }
    }
}

pub fn default_path() -> PathBuf {
    match std::env::var_os("APPDATA") {
        Some(appdata) => PathBuf::from(appdata).join("notty").join("config.toml"),
        None => PathBuf::from("notty-config.toml"),
    }
}

pub fn load(path: &Path) -> LoadResult {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return LoadResult::Missing(Config::default()),
    };
    match toml::from_str::<Config>(&text) {
        Ok(cfg) => LoadResult::Loaded(cfg),
        Err(e) => LoadResult::Defaulted(Config::default(), e.to_string()),
    }
}

/// Guarda `cfg`. `[updates] managed_by` lo escribe essentials, no notty: se conserva el
/// que haya en disco, para que un guardado de notty (abierto desde antes) no lo borre.
pub fn save(cfg: &Config, path: &Path) -> std::io::Result<()> {
    let mut cfg = cfg.clone();
    if let LoadResult::Loaded(disk) = load(path) {
        cfg.updates.managed_by = disk.updates.managed_by;
    }
    save_as_is(&cfg, path)
}

/// Guarda `cfg` tal cual, `managed_by` incluido. Solo para «Volver a gestionar yo las
/// actualizaciones», que es lo único de notty que cambia esa clave.
pub fn save_as_is(cfg: &Config, path: &Path) -> std::io::Result<()> {
    let text = toml::to_string_pretty(cfg).expect("Config siempre serializa");
    backup_if_broken(path)?;
    notty_io::create_parent_dirs(path)?;
    notty_io::atomic_write(path, text.as_bytes())
}

/// `config.toml.bak` junto a `path`.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

/// Si el archivo actual no se puede leer como configuración (lo que hizo que `load`
/// devolviera `Defaulted`), se copia a `.bak` antes de escribir encima: el usuario no
/// pierde lo que tenía. Si no existe (primer arranque) no hay nada que guardar.
fn backup_if_broken(path: &Path) -> std::io::Result<()> {
    let Ok(bytes) = std::fs::read(path) else { return Ok(()) };
    let parses = std::str::from_utf8(&bytes).is_ok_and(|t| toml::from_str::<Config>(t).is_ok());
    if parses {
        return Ok(());
    }
    std::fs::write(backup_path(path), &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn missing_file_returns_defaults() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        match load(&p) {
            LoadResult::Missing(cfg) => assert_eq!(cfg, Config::default()),
            other => panic!("esperaba Missing, fue {other:?}"),
        }
    }

    #[test]
    fn valid_file_loads_its_values() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        fs::write(&p, "[ui]\npreset = \"zen\"\n").unwrap();
        match load(&p) {
            LoadResult::Loaded(cfg) => assert_eq!(cfg.ui.preset, crate::Preset::Zen),
            other => panic!("esperaba Loaded, fue {other:?}"),
        }
    }

    #[test]
    fn broken_file_falls_back_to_defaults_with_error_message() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        fs::write(&p, "esto no es toml válido [[[").unwrap();
        match load(&p) {
            LoadResult::Defaulted(cfg, msg) => {
                assert_eq!(cfg, Config::default());
                assert!(!msg.is_empty());
            }
            other => panic!("esperaba Defaulted, fue {other:?}"),
        }
    }

    #[test]
    fn broken_file_is_never_overwritten_by_load() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        fs::write(&p, "roto [[[").unwrap();
        load(&p);
        assert_eq!(fs::read_to_string(&p).unwrap(), "roto [[[");
    }

    #[test]
    fn saving_over_a_broken_file_backs_it_up_first() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        fs::write(&p, "roto [[[").unwrap();
        save(&Config::default(), &p).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("config.toml.bak")).unwrap(), "roto [[[");
        assert!(matches!(load(&p), LoadResult::Loaded(_)));
        // Guardar otra vez sobre uno ya válido no toca la copia.
        save(&Config::default(), &p).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("config.toml.bak")).unwrap(), "roto [[[");
    }

    #[test]
    fn saving_without_existing_file_creates_it_and_no_backup() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("sub").join("config.toml");
        save(&Config::default(), &p).unwrap();
        assert!(p.exists());
        assert!(!backup_path(&p).exists());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.ui.preset = crate::Preset::Clasica;
        crate::apply_preset(&mut cfg.ui, crate::Preset::Clasica);
        save(&cfg, &p).unwrap();
        match load(&p) {
            LoadResult::Loaded(loaded) => assert_eq!(loaded.ui.preset, crate::Preset::Clasica),
            other => panic!("esperaba Loaded, fue {other:?}"),
        }
    }

    #[test]
    fn default_path_ends_with_notty_config_toml() {
        let p = default_path();
        assert_eq!(p.file_name().unwrap(), "config.toml");
        assert_eq!(p.parent().unwrap().file_name().unwrap(), "notty");
    }
}
