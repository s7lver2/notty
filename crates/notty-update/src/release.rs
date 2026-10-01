//! Parseo del JSON de "GitHub Releases -> latest" a nuestro `Release` propio.

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
    #[error("faltan los assets del instalador y su .sig")]
    MissingAssets,
}

pub fn parse_release(json: &str) -> Result<Release, ReleaseError> {
    parse_release_with(json, "notty-setup.exe")
}

/// Como `parse_release`, pero con el instalador `setup` (y `setup.sig`) de otra app.
pub fn parse_release_with(json: &str, setup: &str) -> Result<Release, ReleaseError> {
    let gh: GhRelease = serde_json::from_str(json).map_err(|e| ReleaseError::InvalidJson(e.to_string()))?;
    let sig = format!("{setup}.sig");
    let setup_url = gh.assets.iter().find(|a| a.name == setup).map(|a| a.browser_download_url.clone());
    let sig_url = gh.assets.iter().find(|a| a.name == sig).map(|a| a.browser_download_url.clone());
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
