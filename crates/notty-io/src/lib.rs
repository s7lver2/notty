//! notty-io: detección texto/raw, codificaciones, fin de línea y guardado atómico.

mod drafts;
mod encoding;
mod eol;
mod fsutil;
mod open;
mod pathline;
mod recovery;
mod usage;
mod watch;

pub use drafts::{draft_filename, drafts_dir};
pub use encoding::{CodecError, Detected, TextEncoding, decode, decode_lossy, detect, encode};
pub use eol::{LineEnding, convert, detect_eol};
pub use fsutil::{atomic_write, can_write, create_parent_dirs};
pub use open::{Opened, RawBytes, open, open_raw};
pub use pathline::{
    Entry, Hint, INVALID_CHARS, PathContext, has_invalid_chars, hint_for, home_dir, normalize, ranked_suggestions, suggestions,
};
pub use usage::{Usage, record_path_use, usage_path};
pub use recovery::{RecoveryEntry, clear_recovery, dump_recovery, list_recovery, recovery_dir};
pub use watch::{changed_since, mtime};
