//! `%APPDATA%\essentials\appearance.toml`: el tema y el acento que essentials comparte
//! con sus apps (`theme`, `accent`, `shared`). Lo escribe essentials; notty lo lee y,
//! si lo sigue, cambia solo `theme` y `accent` cuando el usuario los toca en notty.

use std::path::{Path, PathBuf};

use crate::{AccentColor, Theme};

/// `%APPDATA%\essentials\appearance.toml`.
pub fn appearance_path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("essentials").join("appearance.toml"))
}

/// Nombre de cada acento en `appearance.toml`. «sistema» es el `Azul` de siempre.
const ACCENTS: [(&str, AccentColor); 8] = [
    ("sistema", AccentColor::Azul),
    ("verde", AccentColor::Verde),
    ("turquesa", AccentColor::Turquesa),
    ("morado", AccentColor::Morado),
    ("rosa", AccentColor::Rosa),
    ("rojo", AccentColor::Rojo),
    ("naranja", AccentColor::Naranja),
    ("amarillo", AccentColor::Amarillo),
];

const THEMES: [(&str, Theme); 3] = [("sistema", Theme::System), ("claro", Theme::Light), ("oscuro", Theme::Dark)];

/// Tema y acento del archivo, o `None` si no existe, no se entiende o `shared` no es `true`.
pub fn read_appearance(path: &Path) -> Option<(Theme, AccentColor)> {
    let table: toml::Table = toml::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    if table.get("shared")?.as_bool() != Some(true) {
        return None;
    }
    let theme = table.get("theme")?.as_str()?;
    let accent = table.get("accent")?.as_str()?;
    let theme = THEMES.iter().find(|(n, _)| *n == theme)?.1;
    let accent = ACCENTS.iter().find(|(n, _)| *n == accent)?.1;
    Some((theme, accent))
}

/// Cambia `theme` y `accent` del archivo conservando el resto (`shared`…), con
/// temporal + rename en la misma carpeta, como essentials.
pub fn write_appearance(path: &Path, theme: Theme, accent: AccentColor) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path)?;
    let mut table: toml::Table = toml::from_str(&text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let theme = THEMES.iter().find(|(_, t)| *t == theme).map_or("sistema", |(n, _)| *n);
    let accent = ACCENTS.iter().find(|(_, a)| *a == accent).map_or("sistema", |(n, _)| *n);
    table.insert("theme".to_string(), toml::Value::String(theme.to_string()));
    table.insert("accent".to_string(), toml::Value::String(accent.to_string()));
    let tmp = path.with_file_name("appearance.toml.tmp");
    std::fs::write(&tmp, toml::to_string(&table).expect("una tabla siempre serializa"))?;
    std::fs::rename(&tmp, path)
}
