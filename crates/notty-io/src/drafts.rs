use std::path::PathBuf;
use std::time::SystemTime;

pub fn drafts_dir() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(local) => PathBuf::from(local).join("notty").join("drafts"),
        None => PathBuf::from("notty-drafts"),
    }
}

/// "AAAA-MM-DD_HHMM<ext>", calculado a mano (sin `chrono`) a partir del reloj UTC.
/// Como es solo para nombrar archivos de borrador, no hace falta la hora local exacta.
pub fn draft_filename(now: SystemTime, ext: &str) -> String {
    let secs = now.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = secs / 86_400;
    let secs_of_day = secs % 86_400;
    let (hh, mm) = (secs_of_day / 3600, (secs_of_day % 3600) / 60);
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02}_{hh:02}{mm:02}{ext}")
}

/// Algoritmo de Howard Hinnant para convertir "días desde 1970-01-01" a (año, mes, día).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn drafts_dir_ends_with_notty_drafts() {
        let p = drafts_dir();
        assert_eq!(p.file_name().unwrap(), "drafts");
        assert_eq!(p.parent().unwrap().file_name().unwrap(), "notty");
    }

    #[test]
    fn draft_filename_formats_date_and_time() {
        // 2024-01-15 10:30:00 UTC
        let t = UNIX_EPOCH + Duration::from_secs(1_705_314_600);
        assert_eq!(draft_filename(t, ".txt"), "2024-01-15_1030.txt");
    }

    #[test]
    fn draft_filename_uses_the_given_extension() {
        let t = UNIX_EPOCH + Duration::from_secs(0);
        assert!(draft_filename(t, ".md").ends_with(".md"));
    }
}
