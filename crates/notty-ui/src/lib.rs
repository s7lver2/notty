//! notty-ui: ventana Win32, render Direct2D/DirectWrite y lógica de edición en pantalla.

mod doc_io;
mod keymap;
mod viewport;

pub use doc_io::{OpenedDoc, open_as_document, save_document};
pub use keymap::{EditorAction, Modifiers, action_for_vk};
pub use viewport::Viewport;
