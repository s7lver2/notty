//! notty-ui: ventana Win32, render Direct2D/DirectWrite y lógica de edición en pantalla.

mod anim;
pub mod chrome_text;
pub mod clipboard;
mod closing;
mod conflict;
pub mod context_menu;
mod doc_io;
pub mod editor;
mod keyboard_widget;
mod hex;
pub mod layout;
mod keymap;
pub mod ligature;
pub mod menu;
pub mod native_dialog;
mod path_prompt;
mod prompt;
mod raw_doc;
pub mod render;
mod search_prompt;
mod svg_path;
mod text_input;
pub mod syntax;
pub mod splits;
pub mod settings_model;
pub mod settings_window;
mod settings_pages;
mod settings_ui;
pub mod step_rail;
pub mod tab_anim;
mod theme;
pub mod tour;
pub mod update_panel;
mod viewport;
mod vim;
mod vim_cmd;
pub mod welcome_window;
pub mod window;
pub mod workspace;

pub use anim::{Anim, Curve, Tweens, cubic_bezier, ease_out_cubic};
pub use chrome_text::*;
pub use closing::{CloseChoice, CloseRequest, has_unsaved};
pub use conflict::{ConflictAnswer, ConflictState};
pub use doc_io::{OpenedDoc, open_as_document, same_file, save_document};
pub use editor::EditorState;
pub use hex::{hex_char, hex_row, hex_rows};
pub use keymap::{EditorAction, Modifiers, action_for_vk};
pub use path_prompt::{OVERWRITE_QUESTION, OverwriteChoice, PathPromptState, Purpose};
pub use prompt::Prompt;
pub use raw_doc::{RawDoc, open_raw_doc};
pub use render::{AboutContent, Hit, Renderer, UpdatePanelContent, ViewState};
pub use search_prompt::SearchState;
pub use text_input::char_from_utf16_unit;
pub use theme::{Palette, Rgba, is_dark, palette};
pub use update_panel::{DownloadPhase, UpdatePhase, UpdateState};
pub use viewport::Viewport;
pub use vim::{VimMode, VimOutcome, VimState};
pub use vim_cmd::{VimCmd, parse_vim_cmd};
pub use workspace::Workspace;

static APP_VERSION: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// Versión de la app (la de `crates/notty`, que es la que se publica). La fija
/// `notty::main` al arrancar: `env!("CARGO_PKG_VERSION")` aquí dentro daría la de
/// este crate (0.1.0), y con ella "Buscar ahora" comparaba contra una versión falsa.
pub fn set_app_version(v: &'static str) {
    let _ = APP_VERSION.set(v);
}

pub fn app_version() -> &'static str {
    APP_VERSION.get().copied().unwrap_or(env!("CARGO_PKG_VERSION"))
}
