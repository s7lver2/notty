//! notty-config: config.toml, presets y guardado. Sin nada de Windows.

mod model;
mod preset;
mod storage;

pub use model::{Config, Files, MenuBar, Preset, TabsPosition, Theme, UiConfig};
pub use preset::apply_preset;
// TODO(Task 3): storage aún no implementado; se reactiva en la Task 3.
// pub use storage::{LoadResult, default_path, load, save};
