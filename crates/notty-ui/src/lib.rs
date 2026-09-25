//! notty-ui: ventana Win32, render Direct2D/DirectWrite y lógica de edición en pantalla.

pub mod chrome_text;
pub mod clipboard;
mod doc_io;
pub mod editor;
mod hex;
pub mod layout;
mod keymap;
mod path_prompt;
mod prompt;
mod raw_doc;
pub mod render;
mod search_prompt;
pub mod settings_window;
mod theme;
mod viewport;
mod vim;
mod vim_cmd;
pub mod window;
pub mod workspace;

pub use chrome_text::*;
pub use doc_io::{OpenedDoc, open_as_document, save_document};
pub use editor::EditorState;
pub use hex::{hex_char, hex_row, hex_rows};
pub use keymap::{EditorAction, Modifiers, action_for_vk};
pub use path_prompt::{PathPromptState, Purpose};
pub use prompt::Prompt;
pub use raw_doc::{RawDoc, open_raw_doc};
pub use render::Renderer;
pub use search_prompt::SearchState;
pub use theme::{Palette, Rgba, is_dark, palette};
pub use viewport::Viewport;
pub use vim::{VimMode, VimOutcome, VimState};
pub use vim_cmd::{VimCmd, parse_vim_cmd};
pub use workspace::Workspace;
