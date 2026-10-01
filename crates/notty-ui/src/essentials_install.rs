//! Desde la 1.1, notty se actualiza desde essentials. Si essentials no está instalado,
//! el aviso de arranque y Ajustes → Actualizaciones ofrecen «Instalar essentials»:
//! se descarga `essentials-setup.exe` de su última release, se verifica su firma con
//! `notty_update::ESSENTIALS_PUBKEY` y se abre. El estado es global (lo leen la
//! ventana principal y Ajustes) y el hilo despierta a la principal al cambiar.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    Idle,
    /// Bytes descargados y totales (0 si no se saben).
    Downloading(u64, u64),
    /// El instalador ya se lanzó: el aviso enseña «Abriendo el instalador…» un momento.
    Opening(Instant),
    Error,
}

static PHASE: Mutex<Phase> = Mutex::new(Phase::Idle);
/// Cada descarga lleva un número; «Cancelar» lo sube y el hilo viejo ya no cuenta.
static RUN: AtomicU32 = AtomicU32::new(0);
static DIRTY: AtomicBool = AtomicBool::new(false);

pub fn phase() -> Phase {
    *PHASE.lock().unwrap()
}

/// Si cambió el estado desde la última vez (la ventana principal repinta).
pub fn take_dirty() -> bool {
    DIRTY.swap(false, Ordering::AcqRel)
}

fn set(run: u32, p: Phase) {
    if RUN.load(Ordering::Acquire) != run {
        return;
    }
    *PHASE.lock().unwrap() = p;
    DIRTY.store(true, Ordering::Release);
    crate::window::wake_for_ipc();
}

/// essentials registra `essentials.exe` en App Paths (HKCU al instalarse). En el
/// perfil de pruebas (`NOTTY_PRUEBAS`) no se mira el registro: manda `NOTTY_ESSENTIALS=1`.
pub fn installed() -> bool {
    if std::env::var_os("NOTTY_PRUEBAS").is_some() {
        return std::env::var("NOTTY_ESSENTIALS").is_ok_and(|v| v == "1");
    }
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    use windows::core::w;
    let key = w!(r"Software\Microsoft\Windows\CurrentVersion\App Paths\essentials.exe");
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .any(|root| unsafe { RegGetValueW(root, key, None, RRF_RT_REG_SZ, None, None, None) }.is_ok())
}

/// «Instalar essentials» / «Reintentar». No hace nada si ya hay una en marcha.
pub fn start() {
    {
        let mut p = PHASE.lock().unwrap();
        if matches!(*p, Phase::Downloading(..) | Phase::Opening(_)) {
            return;
        }
        *p = Phase::Downloading(0, 0);
    }
    let run = RUN.fetch_add(1, Ordering::AcqRel) + 1;
    DIRTY.store(true, Ordering::Release);
    std::thread::spawn(move || {
        let ok = match std::env::var("NOTTY_ESSENTIALS_SIMULAR") {
            Ok(s) if std::env::var_os("NOTTY_PRUEBAS").is_some() => simulate(run, s == "ok"),
            _ => download_and_open(run),
        };
        set(run, if ok { Phase::Opening(Instant::now()) } else { Phase::Error });
    });
}

/// «Cancelar»: la descarga sigue en su hilo, pero ya no cuenta ni abre nada.
pub fn cancel() {
    RUN.fetch_add(1, Ordering::AcqRel);
    *PHASE.lock().unwrap() = Phase::Idle;
    DIRTY.store(true, Ordering::Release);
}

/// Tras «Abriendo el instalador…», el aviso se cierra solo.
pub fn finish_opening() {
    let mut p = PHASE.lock().unwrap();
    if matches!(*p, Phase::Opening(_)) {
        *p = Phase::Idle;
    }
}

fn download_and_open(run: u32) -> bool {
    let ua = format!("notty/{}", crate::app_version());
    let Ok(release) = notty_update::http::latest_release_with(notty_update::ESSENTIALS_REPO, "essentials-setup.exe", &ua) else {
        return false;
    };
    let dir = std::env::temp_dir().join("notty-essentials");
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let setup = dir.join("essentials-setup.exe");
    let sig = dir.join("essentials-setup.exe.sig");
    let mut last = Instant::now();
    let got = notty_update::http::download(&release.setup_url, &setup, |done, total| {
        if last.elapsed().as_millis() >= 33 || done == total {
            last = Instant::now();
            set(run, Phase::Downloading(done, total));
        }
    })
    .is_ok()
        && notty_update::http::download(&release.sig_url, &sig, |_, _| {}).is_ok();
    let verified = got
        && match (std::fs::read(&setup), std::fs::read(&sig)) {
            (Ok(bytes), Ok(s)) => <[u8; 64]>::try_from(s.as_slice()).is_ok_and(|s| notty_update::verify(&bytes, &s, &notty_update::ESSENTIALS_PUBKEY)),
            _ => false,
        };
    let _ = std::fs::remove_file(&sig);
    // Nunca se ejecuta un binario sin verificar; cancelado, tampoco.
    if !verified || RUN.load(Ordering::Acquire) != run {
        let _ = std::fs::remove_file(&setup);
        return false;
    }
    std::process::Command::new(&setup).spawn().is_ok()
}

/// Perfil de pruebas: progreso falso en 3 s y, con `ok`, sin abrir nada.
fn simulate(run: u32, ok: bool) -> bool {
    const TOTAL: u64 = 4_200_000;
    for i in 1..=30u64 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if !ok && i == 18 {
            return false;
        }
        set(run, Phase::Downloading(TOTAL * i / 30, TOTAL));
    }
    ok
}
