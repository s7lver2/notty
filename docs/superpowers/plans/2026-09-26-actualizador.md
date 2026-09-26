# Actualizador (notty-update, integración en notty, release.ps1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in, Ed25519-verified update checker against GitHub Releases, wire it into notty's status bar/panel, and script releases locally with `tools/release.ps1`.

**Architecture:** A new networking-free-by-default `notty-update` crate holds pure logic (`is_newer`, `due`, JSON parsing, signature verification) plus the actual WinHTTP calls, kept separate from `notty-ui` per the survey's advice to keep new HTTP dependencies out of the render-heavy crate. `notty-config` gains an `[updates]` section. `notty` wires a background thread + status bar notice + panel. `tools/release.ps1` and a `notty-sign` bin (gated by a `sign` feature on `notty-update`) build and sign releases, kept out of the shipped `notty`/`notty-setup` binaries.

**Tech Stack:** Rust, `windows` crate WinHTTP bindings (`Win32_Networking_WinHttp`), `serde_json`, `ed25519-dalek` (verify-only in the default build; signing only behind the `sign` feature), PowerShell, `gh` CLI.

## Global Constraints

- No network access unless `[updates].check = true`, set only by the tutorial's Privacidad step or Ajustes → Acerca de → "Buscar ahora"/toggle.
- The private signing key never enters the repo — lives at `%USERPROFILE%\.notty-release\ed25519.key`, generated once via `notty-sign --keygen`.
- One version source of truth: read from `notty`'s own `Cargo.toml` `[package].version` (no shared `[workspace.package] version` exists per the survey — `release.ps1` reads `crates/notty/Cargo.toml`, not the workspace root).
- If signature verification fails, delete the downloaded files and show an error. Never execute unverified binaries.
- Silent failure only for the automatic background check (network errors, rate limits, bad JSON just update `last_check` and stop); the manual "Buscar ahora" always shows a clear message on failure.

---

### Task 1: `notty-config` `[updates]` section

**Files:**
- Modify: `crates/notty-config/src/model.rs`
- Test: same file, extend existing test module

**Interfaces:**
- Produces: `pub struct UpdatesConfig { pub check: bool, pub last_check: u64 }` (both default `false`/`0`), added as `pub updates: UpdatesConfig` field on `Config`.

- [ ] **Step 1: Write the failing test**

Add near the existing `parses_partial_toml_with_defaults` test in `crates/notty-config/src/model.rs`:
```rust
#[test]
fn updates_config_defaults_to_opt_out() {
    let cfg = Config::default();
    assert!(!cfg.updates.check);
    assert_eq!(cfg.updates.last_check, 0);
}

#[test]
fn partial_toml_with_updates_section_parses() {
    let toml_str = "[updates]\ncheck = true\nlast_check = 1234567890\n";
    let cfg: Config = toml::from_str(toml_str).unwrap();
    assert!(cfg.updates.check);
    assert_eq!(cfg.updates.last_check, 1234567890);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p notty-config updates_config`
Expected: FAIL — `Config` has no field `updates`.

- [ ] **Step 3: Implement**

In `crates/notty-config/src/model.rs`, following the exact `HotkeyConfig` pattern:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatesConfig {
    pub check: bool,
    pub last_check: u64,
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self { check: false, last_check: 0 }
    }
}
```
Add `pub updates: UpdatesConfig,` to `Config` (alongside `ui`, `files`, `hotkey`). Update `lib.rs`'s `pub use model::{...}` list to include `UpdatesConfig`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p notty-config`
Expected: PASS (all existing tests plus the 2 new ones).

- [ ] **Step 5: Commit**

```bash
git add crates/notty-config/src
git commit -m "feat(updater): add opt-in [updates] config section"
```

---

### Task 2: `notty-update` crate — pure logic (`is_newer`, `due`, release JSON parsing, `verify`)

**Files:**
- Create: `crates/notty-update/Cargo.toml`
- Create: `crates/notty-update/src/lib.rs`
- Create: `crates/notty-update/src/release.rs`
- Create: `crates/notty-update/src/semver.rs`
- Create: `crates/notty-update/src/sign.rs`
- Test: each of the three logic files, inline

**Interfaces:**
- Produces: `pub struct Release { pub tag: String, pub version: String, pub body: String, pub setup_url: String, pub sig_url: String }`, `pub fn parse_release(json: &str) -> Result<Release, ReleaseError>`, `pub fn is_newer(current: &str, candidate: &str) -> bool`, `pub fn due(last_check: u64, now: u64) -> bool`, `pub fn verify(bytes: &[u8], sig: &[u8;64], pubkey: &[u8;32]) -> bool`.

- [ ] **Step 1: Scaffold the crate**

`crates/notty-update/Cargo.toml`:
```toml
[package]
name = "notty-update"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = "1"
ed25519-dalek = "2"
windows = { version = "0.62.2", features = [
    "Win32_Networking_WinHttp",
    "Win32_Foundation",
] }

[dev-dependencies]
tempfile = { workspace = true }

[[bin]]
name = "notty-sign"
path = "src/bin/notty_sign.rs"
required-features = ["sign"]

[features]
sign = ["ed25519-dalek/rand_core", "dep:rand"]

[dependencies.rand]
version = "0.8"
optional = true
```

- [ ] **Step 2: Write failing tests for `is_newer` and `due`**

`crates/notty-update/src/semver.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SemVer(u64, u64, u64);

fn parse(v: &str) -> Option<SemVer> {
    let core = v.trim_start_matches('v').split('-').next()?; // drop -beta etc.
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some(SemVer(major, minor, patch))
}

pub fn is_newer(current: &str, candidate: &str) -> bool {
    if candidate.contains('-') {
        return false; // suffixed tags (prereleases) are always ignored
    }
    match (parse(current), parse(candidate)) {
        (Some(c), Some(n)) => n > c,
        _ => false,
    }
}

pub fn due(last_check: u64, now: u64) -> bool {
    now.saturating_sub(last_check) >= 24 * 3600
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_patch_is_newer() { assert!(is_newer("1.2.0", "1.2.1")); }

    #[test]
    fn same_version_is_not_newer() { assert!(!is_newer("1.2.0", "1.2.0")); }

    #[test]
    fn older_version_is_not_newer() { assert!(!is_newer("1.2.1", "1.2.0")); }

    #[test]
    fn prerelease_suffix_is_ignored() { assert!(!is_newer("1.2.0", "1.3.0-beta.1")); }

    #[test]
    fn v_prefix_is_accepted() { assert!(is_newer("1.2.0", "v1.3.0")); }

    #[test]
    fn due_after_24h() { assert!(due(0, 24 * 3600)); }

    #[test]
    fn not_due_before_24h() { assert!(!due(0, 24 * 3600 - 1)); }
}
```

- [ ] **Step 3: Run, confirm pass**

Run: `cargo test -p notty-update semver::tests`
Expected: PASS (7 tests) since implementation is included above — this crate follows write-then-verify since the logic is short; run the test immediately after writing both to confirm.

- [ ] **Step 4: Write failing tests for release JSON parsing**

`crates/notty-update/src/release.rs`:
```rust
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub version: String,
    pub body: String,
    pub setup_url: String,
    pub sig_url: String,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    body: Option<String>,
    assets: Vec<GhAsset>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    #[error("JSON inválido: {0}")]
    InvalidJson(String),
    #[error("faltan los assets notty-setup.exe / notty-setup.exe.sig")]
    MissingAssets,
}

pub fn parse_release(json: &str) -> Result<Release, ReleaseError> {
    let gh: GhRelease = serde_json::from_str(json).map_err(|e| ReleaseError::InvalidJson(e.to_string()))?;
    let setup_url = gh.assets.iter().find(|a| a.name == "notty-setup.exe").map(|a| a.browser_download_url.clone());
    let sig_url = gh.assets.iter().find(|a| a.name == "notty-setup.exe.sig").map(|a| a.browser_download_url.clone());
    let (setup_url, sig_url) = match (setup_url, sig_url) {
        (Some(s), Some(g)) => (s, g),
        _ => return Err(ReleaseError::MissingAssets),
    };
    let version = gh.tag_name.trim_start_matches('v').to_string();
    Ok(Release { tag: gh.tag_name, version, body: gh.body.unwrap_or_default(), setup_url, sig_url })
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_SAMPLE: &str = r#"{
        "tag_name": "v1.3.0",
        "body": "Notas de la versión",
        "assets": [
            {"name": "notty-setup.exe", "browser_download_url": "https://example.com/notty-setup.exe"},
            {"name": "notty-setup.exe.sig", "browser_download_url": "https://example.com/notty-setup.exe.sig"}
        ]
    }"#;

    #[test]
    fn parses_real_sample() {
        let r = parse_release(REAL_SAMPLE).unwrap();
        assert_eq!(r.version, "1.3.0");
        assert_eq!(r.tag, "v1.3.0");
        assert!(r.setup_url.ends_with("notty-setup.exe"));
    }

    #[test]
    fn missing_assets_errors() {
        let json = r#"{"tag_name": "v1.3.0", "body": "", "assets": []}"#;
        assert!(matches!(parse_release(json), Err(ReleaseError::MissingAssets)));
    }

    #[test]
    fn invalid_json_errors() {
        assert!(matches!(parse_release("not json"), Err(ReleaseError::InvalidJson(_))));
    }
}
```
Add `thiserror = { workspace = true }` to `notty-update`'s `Cargo.toml` dependencies.

- [ ] **Step 5: Run tests**

Run: `cargo test -p notty-update release::tests`
Expected: PASS (3 tests).

- [ ] **Step 6: Write failing tests for `verify`**

`crates/notty-update/src/sign.rs`:
```rust
use ed25519_dalek::{Signature, VerifyingKey};

pub fn verify(bytes: &[u8], sig: &[u8; 64], pubkey: &[u8; 32]) -> bool {
    let Ok(vk) = VerifyingKey::from_bytes(pubkey) else { return false };
    let signature = Signature::from_bytes(sig);
    vk.verify_strict(bytes, &signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use ed25519_dalek::pkcs8::spki::rand_core::OsRng;

    #[test]
    fn valid_signature_verifies() {
        let sk = SigningKey::generate(&mut OsRng);
        let pubkey = sk.verifying_key().to_bytes();
        let msg = b"notty-setup.exe bytes go here";
        let sig = sk.sign(msg).to_bytes();
        assert!(verify(msg, &sig, &pubkey));
    }

    #[test]
    fn altered_bytes_fail() {
        let sk = SigningKey::generate(&mut OsRng);
        let pubkey = sk.verifying_key().to_bytes();
        let sig = sk.sign(b"original").to_bytes();
        assert!(!verify(b"altered!", &sig, &pubkey));
    }

    #[test]
    fn wrong_pubkey_fails() {
        let sk = SigningKey::generate(&mut OsRng);
        let other = SigningKey::generate(&mut OsRng);
        let msg = b"message";
        let sig = sk.sign(msg).to_bytes();
        assert!(!verify(msg, &sig, &other.verifying_key().to_bytes()));
    }
}
```
Note: the `pkcs8::spki::rand_core::OsRng` re-export path may differ by `ed25519-dalek` version — if it doesn't resolve, add `rand_core = "0.6"` as a dev-dependency and use `rand_core::OsRng` directly instead.

- [ ] **Step 7: Run tests**

Run: `cargo test -p notty-update sign::tests`
Expected: PASS (3 tests).

- [ ] **Step 8: Wire `lib.rs`**

```rust
mod release;
mod semver;
mod sign;

pub use release::{parse_release, Release, ReleaseError};
pub use semver::{due, is_newer};
pub use sign::verify;

pub mod http; // Task 3
```

- [ ] **Step 9: Commit**

```bash
git add crates/notty-update
git commit -m "feat(updater): notty-update pure logic (semver, release parsing, verify)"
```

---

### Task 3: WinHTTP networking (`latest_release`, `download`)

**Files:**
- Create: `crates/notty-update/src/http.rs`

**Interfaces:**
- Consumes: `parse_release` (Task 2).
- Produces: `pub fn latest_release(owner_repo: &str, user_agent: &str) -> Result<Release, HttpError>`, `pub fn download(url: &str, dest: &std::path::Path, on_progress: impl FnMut(u64, u64)) -> Result<(), HttpError>`.

This task is not unit-testable (real network I/O against api.github.com) — verify manually.

- [ ] **Step 1: Implement `latest_release`**

```rust
use windows::Win32::Networking::WinHttp::*;
use windows::core::HSTRING;

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("sin red o error HTTP: {0}")]
    Network(String),
    #[error(transparent)]
    Release(#[from] crate::release::ReleaseError),
}

pub fn latest_release(owner_repo: &str, user_agent: &str) -> Result<crate::Release, HttpError> {
    let json = get(
        "api.github.com",
        &format!("/repos/{owner_repo}/releases/latest"),
        user_agent,
        "application/vnd.github+json",
    )?;
    Ok(crate::parse_release(&json)?)
}

fn get(host: &str, path: &str, user_agent: &str, accept: &str) -> Result<String, HttpError> {
    unsafe {
        let session = WinHttpOpen(&HSTRING::from(user_agent), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, None, None, 0)
            .map_err(|e| HttpError::Network(e.to_string()))?;
        let connect = WinHttpConnect(session, &HSTRING::from(host), WINHTTP_INTERNET_DEFAULT_HTTPS_PORT, None)
            .map_err(|e| HttpError::Network(e.to_string()))?;
        let request = WinHttpOpenRequest(
            connect, &HSTRING::from("GET"), &HSTRING::from(path), None, None, None, WINHTTP_FLAG_SECURE,
        ).map_err(|e| HttpError::Network(e.to_string()))?;

        let headers = format!("Accept: {accept}\r\n");
        WinHttpAddRequestHeaders(request, &HSTRING::from(headers.as_str()), WINHTTP_ADDREQ_FLAG_ADD)
            .map_err(|e| HttpError::Network(e.to_string()))?;

        WinHttpSendRequest(request, None, None, 0, 0, 0, None)
            .map_err(|e| HttpError::Network(e.to_string()))?;
        WinHttpReceiveResponse(request, None).map_err(|e| HttpError::Network(e.to_string()))?;

        let mut buf = Vec::new();
        loop {
            let mut available: u32 = 0;
            WinHttpQueryDataAvailable(request, Some(&mut available)).map_err(|e| HttpError::Network(e.to_string()))?;
            if available == 0 { break; }
            let mut chunk = vec![0u8; available as usize];
            let mut read: u32 = 0;
            WinHttpReadData(request, &mut chunk, Some(&mut read)).map_err(|e| HttpError::Network(e.to_string()))?;
            chunk.truncate(read as usize);
            buf.extend_from_slice(&chunk);
        }
        String::from_utf8(buf).map_err(|e| HttpError::Network(e.to_string()))
    }
}
```

- [ ] **Step 2: Implement `download` with progress**

```rust
pub fn download(url: &str, dest: &std::path::Path, mut on_progress: impl FnMut(u64, u64)) -> Result<(), HttpError> {
    let parsed = url::Url::parse(url).map_err(|e| HttpError::Network(e.to_string()))?;
    // Reuse the same WinHttp session/connect/request pattern as `get`, but stream to `dest`
    // via std::fs::File + WinHttpQueryDataAvailable/WinHttpReadData in a loop, calling
    // on_progress(bytes_so_far, content_length) each chunk. content_length comes from the
    // Content-Length response header via WinHttpQueryHeaders(WINHTTP_QUERY_CONTENT_LENGTH).
    // On any error mid-stream, delete the partial file before returning Err.
    todo!("implemented directly against the pattern above — no new concepts, just streaming to disk")
}
```
Do not leave `todo!()` in the final code — implement it fully, following the exact `get()` structure above but writing bytes to a `File` instead of a `Vec<u8>`, and deleting the partial file (`std::fs::remove_file(dest).ok()`) in every error path. Add `url = "2"` to `Cargo.toml` if not already resolvable another way, or parse host/path manually with `str::split_once` since GitHub release asset URLs are simple HTTPS URLs (avoids a new dependency — prefer this given `ponytail` conventions).

- [ ] **Step 3: Manual verification**

```powershell
cargo run -p notty-update --example check_latest
```
(Add a throwaway `examples/check_latest.rs` printing `latest_release("s7lver2/notty", "notty/0.1.0")` — delete it after confirming it returns real data once the GitHub repo exists; if the repo doesn't exist yet, verify against a public repo temporarily, e.g. `"rust-lang/rust"`, then delete the example either way.)

- [ ] **Step 4: Commit**

```bash
git add crates/notty-update/src/http.rs
git commit -m "feat(updater): WinHTTP latest_release and download"
```

---

### Task 4: notty integration — background check, status bar, panel

**Files:**
- Modify: `crates/notty/src/main.rs` (spawn the background check thread)
- Modify: `crates/notty-ui/src/settings_window.rs` (Ajustes → Acerca de: toggle + "Buscar ahora")
- Create: `crates/notty-ui/src/update_panel.rs` (status bar notice + release-notes panel)
- Modify: `crates/notty-ui/Cargo.toml` (add `notty-update` path dependency)

**Interfaces:**
- Consumes: `notty_update::{is_newer, due, latest_release, download, verify}`, `notty_config::{Config, UpdatesConfig}`.
- Produces: a status bar notice "Actualización X.Y.Z disponible" wired per the spec's flow.

- [ ] **Step 1: Background check on startup**

In `crates/notty/src/main.rs`, after config is loaded and the main window is about to run: if `config.updates.check && notty_update::due(config.updates.last_check, now())`, spawn a `std::thread::spawn` that calls `notty_update::latest_release("<owner>/notty", &format!("notty/{}", env!("CARGO_PKG_VERSION")))`, and on success where `is_newer(env!("CARGO_PKG_VERSION"), &release.version)`, posts the result to the main window (same `PostMessageW`/`WM_APP` pattern as installer plan Task 6 Step 4 — or, if `notty-ui` already has an existing cross-thread channel for IPC messages, reuse that channel instead of inventing a second one: check `crates/notty/src/daemon.rs` and `notty-ipc` for the existing mechanism first). Always update and persist `last_check = now()` via `notty_config::save`, regardless of outcome (silent failure per spec).

- [ ] **Step 2: Status bar notice + panel**

`crates/notty-ui/src/update_panel.rs`: a small state struct holding `Option<Release>`; when set, the status bar (already rendered somewhere in `window.rs` — locate the existing status bar drawing code and add one more segment) shows "Actualización X.Y.Z disponible", clickable to open a panel with the release notes (`body`) and a "Actualizar" button. Reuse `notty-ui`'s existing panel/overlay pattern if one exists (check `settings_window.rs` for how it overlays the main window, or whether panels are a separate concept — if none exists, this is a new small floating D2D panel anchored above the status bar, styled with `palette(dark)`).

- [ ] **Step 3: "Actualizar" flow**

Clicking "Actualizar": spawn a thread that calls `download` for both `notty-setup.exe` and `notty-setup.exe.sig` into `%TEMP%\notty-update\`, with progress reported into the panel. On completion, `verify(&setup_bytes, &sig_bytes, PUBKEY)`. If verification fails: delete both files, show "La descarga no es auténtica" in the panel, stop. If it succeeds: save the current session (reuse notty's existing session-restore mechanism — locate it via `daemon.rs`/`crates/notty/src` session handling), then `CreateProcessW` (or `std::process::Command`) `notty-setup.exe --update --relaunch --from <current> --to <new>` and exit the current notty process.

- [ ] **Step 4: Ajustes → Acerca de**

In `crates/notty-ui/src/settings_model.rs`, add an "Acerca de" section (or extend an existing one) with a `Row::Toggle` bound to `config.updates.check` and a `Row::Link`-style "Buscar ahora" row that triggers the same check-now logic as Step 1 but synchronously-triggered (still runs on a background thread so the UI doesn't block; "synchronous" here means user-initiated, not blocking).

- [ ] **Step 5: PUBKEY placeholder**

Add to `crates/notty-update/src/lib.rs`:
```rust
/// Reemplázalo por la clave pública real generada con `notty-sign --keygen`
/// antes de la primera release firmada.
pub const PUBKEY_PLACEHOLDER_UNSET: [u8; 32] = [0u8; 32];
```
notty itself should define its own `const PUBKEY: [u8; 32] = [...]` (compiled into the `notty` binary, not `notty-update`, since it's the verifier's trust anchor) — leave a clearly marked `TODO`-free placeholder constant of all zeros in `crates/notty/src/main.rs` with a comment pointing at Task 6 (release tooling) as the place that documents how to fill it in for real; verification against all-zero key will simply always fail, which is safe-by-default.

- [ ] **Step 6: Manual verification**

- Enable updates in Ajustes → Acerca de, confirm `config.toml` on disk gets `[updates] check = true`.
- "Buscar ahora" against a repo with no releases yet (or a fake `owner/repo`) → confirm the manual-check error message appears, and it's a clear message not a crash.
- Confirm the automatic check never appears in the UI unless `check = true` — toggle it off, restart notty, confirm no network call happens (check via `read_network_requests`-equivalent or just code inspection: the `if config.updates.check` guard).

- [ ] **Step 7: Commit**

```bash
git add crates/notty crates/notty-ui
git commit -m "feat(updater): wire opt-in update check into notty's UI"
```

---

### Task 5: `notty-sign` binary + `--keygen`

**Files:**
- Create: `crates/notty-update/src/bin/notty_sign.rs`

**Interfaces:**
- Produces: `notty-sign --keygen` (writes `%USERPROFILE%\.notty-release\ed25519.key`, prints the public key bytes to paste into `PUBKEY`), `notty-sign --sign <file> --key <path> --out <file>.sig`.

- [ ] **Step 1: Implement**

```rust
use ed25519_dalek::{Signer, SigningKey};
use ed25519_dalek::pkcs8::spki::rand_core::OsRng;
use std::path::PathBuf;

fn default_key_path() -> PathBuf {
    let home = std::env::var("USERPROFILE").expect("USERPROFILE no definido");
    PathBuf::from(home).join(".notty-release").join("ed25519.key")
}

fn cmd_keygen() {
    let path = default_key_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let sk = SigningKey::generate(&mut OsRng);
    std::fs::write(&path, sk.to_bytes()).expect("no se pudo escribir la clave privada");
    println!("Clave privada escrita en {}", path.display());
    println!("Clave pública (pégala en PUBKEY): {:?}", sk.verifying_key().to_bytes());
}

fn cmd_sign(file: &str, key: Option<&str>, out: &str) {
    let key_path = key.map(PathBuf::from).unwrap_or_else(default_key_path);
    let key_bytes: [u8; 32] = std::fs::read(&key_path).expect("no se pudo leer la clave privada").try_into().expect("clave con longitud inesperada");
    let sk = SigningKey::from_bytes(&key_bytes);
    let data = std::fs::read(file).expect("no se pudo leer el archivo a firmar");
    let sig = sk.sign(&data);
    std::fs::write(out, sig.to_bytes()).expect("no se pudo escribir la firma");
    println!("Firma escrita en {out}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--keygen") => cmd_keygen(),
        Some("--sign") => {
            let file = args.get(1).expect("uso: notty-sign --sign <file> [--key <path>] --out <file>.sig");
            let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).expect("falta --out");
            let key = args.iter().position(|a| a == "--key").and_then(|i| args.get(i + 1)).map(String::as_str);
            cmd_sign(file, key, out);
        }
        _ => eprintln!("uso: notty-sign --keygen | notty-sign --sign <file> --out <file>.sig [--key <path>]"),
    }
}
```

- [ ] **Step 2: Manual verification**

```powershell
cargo run -p notty-update --features sign --bin notty-sign -- --keygen
cargo run -p notty-update --features sign --bin notty-sign -- --sign Cargo.lock --out Cargo.lock.sig
```
Expected: both commands succeed; a throwaway test using `notty_update::verify` against the printed pubkey and the generated `.sig` confirms the round-trip works (write this as a one-off manual check, not a permanent test, since it needs a real generated key on disk).
Clean up: `Remove-Item Cargo.lock.sig` afterward (test artifact, not for commit).

- [ ] **Step 3: Commit**

```bash
git add crates/notty-update/src/bin
git commit -m "feat(updater): notty-sign keygen and signing tool"
```

---

### Task 6: `tools/release.ps1`

**Files:**
- Create: `tools/release.ps1`

**Interfaces:**
- Consumes: `notty-sign` (Task 5), `notty-setup` (installer plan), WiX CLI, `gh` CLI.

- [ ] **Step 1: GitHub repo prerequisite**

Before this task can run for real: create the GitHub repo and add the remote. This is a one-time manual step, not scriptable safely (requires the user's `gh auth login` and a decision on repo visibility) — confirm with the user, then:
```bash
gh repo create <owner>/notty --private --source=. --remote=origin
```
Record the chosen `owner/repo` as a constant used by `notty` (Task 4 Step 1) and this script.

- [ ] **Step 2: Write the script**

`tools/release.ps1`:
```powershell
param(
    [Parameter(Mandatory=$true)][string]$Tag,   # e.g. v1.3.0
    [string]$NotesFile
)

$ErrorActionPreference = "Stop"

# 1. Clean tree + version match
$status = git status --porcelain
if ($status) { throw "El árbol de trabajo no está limpio. Confirma o descarta los cambios primero." }

$version = ($Tag -replace '^v', '')
$cargoVersion = (Select-String -Path "crates\notty\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
if ($version -ne $cargoVersion) {
    throw "El tag $Tag no coincide con la versión de crates/notty/Cargo.toml ($cargoVersion)."
}

# 2. Build
cargo build --release -p notty -p notty-legacy
if ($LASTEXITCODE -ne 0) { throw "Fallo compilando notty/notty-legacy" }

wix build installer\notty.wxs `
    -d NottyExePath=target\release\notty.exe `
    -d NottyLegacyExePath=target\release\notepad_legacy.exe `
    -o installer\notty.msi
if ($LASTEXITCODE -ne 0) { throw "Fallo construyendo el MSI" }

cargo build --release -p notty-setup
if ($LASTEXITCODE -ne 0) { throw "Fallo compilando notty-setup" }

$setupExe = "target\release\notty-setup.exe"

# 3. Sign
cargo run --release -p notty-update --features sign --bin notty-sign -- `
    --sign $setupExe --out "$setupExe.sig"
if ($LASTEXITCODE -ne 0) { throw "Fallo firmando notty-setup.exe" }

# 4. Publish
if (-not $NotesFile) { throw "Pasa -NotesFile con las notas de la versión." }
gh release create $Tag $setupExe "$setupExe.sig" --notes-file $NotesFile
if ($LASTEXITCODE -ne 0) { throw "Fallo publicando la release en GitHub" }

Write-Host "Release $Tag publicada." -ForegroundColor Green
```

- [ ] **Step 2b: Extract the version-check into a testable function**

Since PowerShell scripts aren't covered by `cargo test`, extract the tag/version comparison into a small function with its own inline check, per the spec's "extraída a función testeable si procede":
```powershell
function Test-TagMatchesVersion {
    param([string]$Tag, [string]$CargoVersion)
    return ($Tag -replace '^v', '') -eq $CargoVersion
}
```
Replace the inline comparison in Step 2 with a call to this function. Add a throwaway Pester-free manual check at the bottom of the file guarded by `if ($MyInvocation.InvocationName -eq '.') { ... }` is overkill for this project's "no tests unless asked" convention — instead just verify manually in Step 3 below (this project's testing convention, per house rules, is manual verification for scripts, `cargo test` only for Rust logic).

- [ ] **Step 3: Manual verification (dry run without publishing)**

Comment out the final `gh release create` line temporarily, run:
```powershell
.\tools\release.ps1 -Tag v0.1.0 -NotesFile CHANGELOG.md
```
Expected: fails fast with a clear message if the tree is dirty or the tag doesn't match; otherwise builds `notty.msi` and `notty-setup.exe` + `.sig` successfully. Restore the `gh release create` line after confirming.

- [ ] **Step 4: Commit**

```bash
git add tools/release.ps1
git commit -m "feat(updater): local release script (build, sign, publish)"
```

---

## Manual checklist (full, run once at the end of this plan)

- [ ] Actualización 0.x → 0.y con sesión restaurada
- [ ] Firma alterada rechazada (corrupt the `.sig` file manually and confirm the panel shows the auth error)
- [ ] Sin red / rate limit → fallo silencioso en automático, mensaje claro en manual
