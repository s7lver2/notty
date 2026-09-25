use std::path::Path;
use std::time::SystemTime;

pub fn mtime(path: &Path) -> std::io::Result<SystemTime> {
    std::fs::metadata(path)?.modified()
}

pub fn changed_since(path: &Path, since: SystemTime) -> bool {
    match mtime(path) {
        Ok(t) => t > since,
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn unchanged_file_reports_false() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "x").unwrap();
        let t = mtime(&p).unwrap();
        assert!(!changed_since(&p, t));
    }

    #[test]
    fn rewriting_the_file_reports_true() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "x").unwrap();
        let t = mtime(&p).unwrap();
        sleep(Duration::from_millis(20));
        std::fs::write(&p, "y").unwrap();
        assert!(changed_since(&p, t));
    }

    #[test]
    fn missing_file_counts_as_changed() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("no-existe.txt");
        assert!(changed_since(&p, std::time::SystemTime::now()));
    }
}
