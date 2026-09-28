//! Medidas para `tools/bench.ps1`: solo se activa con la variable de entorno
//! `NOTTY_BENCH_LOG` (ruta del archivo donde escribirlas). Sin ella no cuesta más que
//! mirar un `OnceLock` por pintado.
//!
//! Formato: una línea `p <unix_ms> <duración_us>` por cada pintado de la ventana.

use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn log() -> Option<&'static Mutex<std::fs::File>> {
    static LOG: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
    LOG.get_or_init(|| {
        let path = std::env::var_os("NOTTY_BENCH_LOG")?;
        std::fs::OpenOptions::new().create(true).append(true).open(path).ok().map(Mutex::new)
    })
    .as_ref()
}

/// Un pintado de la ventana principal que ha tardado `dur`.
pub fn record_paint(dur: Duration) {
    let Some(f) = log() else { return };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let mut f = f.lock().unwrap_or_else(|e| e.into_inner());
    let _ = writeln!(f, "p {now} {}", dur.as_micros());
}

/// Marca de una fase del arranque (`m <unix_ms> <etiqueta>`), para ver dónde se va
/// el tiempo hasta el primer pintado.
pub fn mark(label: &str) {
    let Some(f) = log() else { return };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let mut f = f.lock().unwrap_or_else(|e| e.into_inner());
    let _ = writeln!(f, "m {now} {label}");
}
