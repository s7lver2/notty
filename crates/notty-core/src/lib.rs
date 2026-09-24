//! notty-core: buffer, edición, historial y búsqueda. Sin nada de Windows.

mod buffer;
mod document;
mod edit;
mod history;

pub use buffer::Buffer;
pub use document::{Document, Selection};
pub use edit::Edit;
pub use history::History;
