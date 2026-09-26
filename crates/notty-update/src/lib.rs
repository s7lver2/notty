//! notty-update: lógica pura de comprobación de actualizaciones (semver, parseo del
//! JSON de GitHub Releases, verificación Ed25519) más las llamadas WinHTTP reales.
//! Sin red por defecto: quien la usa decide cuándo llamar a `http::latest_release`.

mod release;
mod semver;
mod sign;

pub use release::{Release, ReleaseError, parse_release};
pub use semver::{due, is_newer};
pub use sign::verify;

pub mod http;
pub mod notepad;

/// Reemplázalo por la clave pública real generada con `notty-sign --keygen`
/// antes de la primera release firmada.
pub const PUBKEY_PLACEHOLDER_UNSET: [u8; 32] = [0u8; 32];
