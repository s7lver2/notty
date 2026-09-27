//! notty-config: config.toml, presets y guardado. Sin nada de Windows.

mod model;
mod preset;
mod storage;

pub use model::{
    AccentColor, Config, Files, FilesConfig, FontFamily, HotkeyConfig, HotkeyMechanism, Lang, MenuBar, OnCloseUnsaved,
    Preset, TabsPosition, TempMode, Theme, UiConfig, UpdatesConfig,
};
pub use preset::apply_preset;
pub use storage::{LoadResult, default_path, load, save};
