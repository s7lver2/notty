//! notty-ui: ventana Win32, render Direct2D/DirectWrite y lógica de edición en pantalla.

pub mod clipboard;
mod doc_io;
pub mod editor;
mod keymap;
pub mod render;
mod status;
mod viewport;
pub mod window;

pub use doc_io::{OpenedDoc, open_as_document, save_document};
pub use editor::EditorState;
pub use keymap::{EditorAction, Modifiers, action_for_vk};
pub use render::Renderer;
pub use status::status_line;
pub use viewport::Viewport;
