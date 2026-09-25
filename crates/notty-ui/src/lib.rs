//! notty-ui: ventana Win32, render Direct2D/DirectWrite y lógica de edición en pantalla.

pub mod clipboard;
mod doc_io;
pub mod editor;
mod gutter;
mod hints;
mod keymap;
mod path_prompt;
mod prompt;
pub mod render;
mod search_prompt;
pub mod settings_window;
mod status;
mod viewport;
mod vim;
mod vim_cmd;
pub mod window;
pub mod workspace;

pub use doc_io::{OpenedDoc, open_as_document, save_document};
pub use editor::EditorState;
pub use gutter::gutter_width;
pub use hints::hints_text;
pub use keymap::{EditorAction, Modifiers, action_for_vk};
pub use path_prompt::{PathPromptState, Purpose};
pub use prompt::Prompt;
pub use render::Renderer;
pub use search_prompt::SearchState;
pub use status::status_line;
pub use viewport::Viewport;
pub use vim::{VimMode, VimOutcome, VimState};
pub use vim_cmd::{VimCmd, parse_vim_cmd};
pub use workspace::Workspace;
