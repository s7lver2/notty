//! Comparación de versiones semánticas propia (sin dependencia `semver`): solo se
//! necesita "¿la candidata es más nueva?" y las reglas de la spec (prerelease con
//! sufijo `-algo` nunca es "más nueva", el prefijo `v` se acepta y se ignora).

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
    fn newer_patch_is_newer() {
        assert!(is_newer("1.2.0", "1.2.1"));
    }

    #[test]
    fn same_version_is_not_newer() {
        assert!(!is_newer("1.2.0", "1.2.0"));
    }

    #[test]
    fn older_version_is_not_newer() {
        assert!(!is_newer("1.2.1", "1.2.0"));
    }

    #[test]
    fn prerelease_suffix_is_ignored() {
        assert!(!is_newer("1.2.0", "1.3.0-beta.1"));
    }

    #[test]
    fn v_prefix_is_accepted() {
        assert!(is_newer("1.2.0", "v1.3.0"));
    }

    #[test]
    fn due_after_24h() {
        assert!(due(0, 24 * 3600));
    }

    #[test]
    fn not_due_before_24h() {
        assert!(!due(0, 24 * 3600 - 1));
    }
}
