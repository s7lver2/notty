//! notty-core: buffer, edición, historial y búsqueda. Sin nada de Windows.

mod buffer;
mod document;
mod edit;
mod history;
mod search;

pub use buffer::Buffer;
pub use document::{Document, Selection};
pub use edit::Edit;
pub use history::History;
pub use search::{SearchError, SearchOptions, find_all, replace_all};
