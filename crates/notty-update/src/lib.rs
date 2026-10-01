//! notty-update: lógica pura de comprobación de actualizaciones (semver, parseo del
//! JSON de GitHub Releases, verificación Ed25519) más las llamadas WinHTTP reales.
//! Sin red por defecto: quien la usa decide cuándo llamar a `http::latest_release`.

mod release;
mod semver;
mod sign;

pub use release::{Release, ReleaseError, parse_release, parse_release_with};
pub use semver::{due, is_newer};
pub use sign::verify;

pub mod http;
pub mod notepad;

/// "owner/repo" de GitHub Releases contra el que se comprueban actualizaciones.
pub const REPO: &str = "s7lver2/notty";

/// Clave pública Ed25519 contra la que se verifica `notty-setup.exe` antes de
/// ejecutarlo. La privada correspondiente vive en `%USERPROFILE%\.notty-release\ed25519.key`
/// (generada con `notty-sign --keygen`) y nunca se comitea.
pub const PUBKEY: [u8; 32] = [51, 182, 247, 144, 128, 138, 166, 110, 113, 122, 139, 194, 108, 248, 62, 17, 7, 196, 199, 207, 255, 46, 54, 194, 18, 251, 14, 57, 139, 2, 199, 90];

/// "owner/repo" de essentials, la tienda que actualiza notty desde la 1.1.
pub const ESSENTIALS_REPO: &str = "s7lver2/essentials";

/// Clave pública Ed25519 de essentials (`%USERPROFILE%\.essentials-release\app.key`):
/// con ella se verifica `essentials-setup.exe` antes de ejecutarlo.
pub const ESSENTIALS_PUBKEY: [u8; 32] = [
    0xec, 0x9d, 0xc7, 0x94, 0x69, 0x19, 0x20, 0xf4, 0x36, 0x24, 0x67, 0x59, 0xeb, 0xd1, 0xfc, 0xdf,
    0xf7, 0x35, 0x92, 0xb9, 0xda, 0xf0, 0xcc, 0xba, 0x02, 0x51, 0xef, 0xda, 0x11, 0x82, 0xb7, 0xf1,
];

/// Reemplázalo por la clave pública real generada con `notty-sign --keygen`
/// antes de la primera release firmada.
pub const PUBKEY_PLACEHOLDER_UNSET: [u8; 32] = [0u8; 32];
