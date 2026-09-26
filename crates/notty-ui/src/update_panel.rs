//! Estado puro del aviso/panel de actualización disponible (Task 4 del plan del
//! actualizador, `docs/superpowers/plans/2026-09-26-actualizador.md`). No dibuja
//! nada ni toca la red — eso lo hacen `render.rs` (el aviso en la barra de estado
//! y el panel) y `notty`/`window.rs` (el hilo de fondo que llama a `notty_update`).
//! Aquí solo vive qué hay que mostrar y qué botón se ha pulsado.

use notty_update::Release;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadPhase {
    Downloading,
    Verifying,
    Failed,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateState {
    /// Release más nueva encontrada (`is_newer` ya comprobado antes de guardarla).
    pub available: Option<Release>,
    /// Si el panel de notas de versión está abierto (clic en el aviso de la barra
    /// de estado, o en "Buscar ahora" desde Ajustes cuando ya hay una encontrada).
    pub panel_open: bool,
    /// Progreso de la descarga en curso (`bytes_leidos, total`), si hay una.
    pub progress: Option<(u64, u64)>,
    /// Fase de la descarga/verificación en curso, si la hay.
    pub phase: Option<DownloadPhase>,
    /// Mensaje de error a mostrar (firma inválida, sin red en la comprobación
    /// manual...). Se limpia en cuanto se sabe algo nuevo.
    pub error: Option<String>,
    /// "Buscar ahora" en curso (el hilo aún no ha contestado).
    pub checking: bool,
    /// La última comprobación manual dijo que no hay nada más nuevo.
    pub up_to_date: bool,
}

/// Lo que enseña Ajustes → Actualizaciones, resuelto a partir de `UpdateState`.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdatePhase {
    /// Sin comprobar todavía en esta sesión, o ya al día.
    Idle,
    Checking,
    Found,
    /// `(bytes leídos, total)`; `total == 0` si el servidor no lo dijo.
    Downloading(u64, u64),
    Error(String),
}

impl UpdateState {
    pub fn phase_for_settings(&self) -> UpdatePhase {
        if self.checking {
            return UpdatePhase::Checking;
        }
        match (&self.phase, &self.error, &self.available) {
            (Some(DownloadPhase::Downloading | DownloadPhase::Verifying), _, _) => {
                let (d, t) = self.progress.unwrap_or((0, 0));
                UpdatePhase::Downloading(d, t)
            }
            (_, Some(e), _) if !self.up_to_date => UpdatePhase::Error(e.clone()),
            (_, _, Some(_)) => UpdatePhase::Found,
            _ => UpdatePhase::Idle,
        }
    }
}

impl UpdateState {
    /// Texto de la barra de estado, o `None` si no hay nada que avisar. Si no hay
    /// ninguna release encontrada pero sí un error de un chequeo manual ("Buscar
    /// ahora"), se muestra ese error aquí: la spec pide que el chequeo manual
    /// "siempre muestre un mensaje claro en caso de fallo" (nunca en silencio, a
    /// diferencia del automático).
    pub fn notice_text(&self) -> Option<String> {
        match (&self.available, &self.error) {
            (Some(r), _) => Some(format!("Actualización {} disponible", r.version)),
            (None, Some(err)) => Some(format!("Actualizaciones: {err}")),
            (None, None) => None,
        }
    }

    /// Se llama cuando el chequeo (automático o manual) encuentra una release más
    /// nueva. Limpia cualquier error previo: hay noticias mejores que dar.
    pub fn set_available(&mut self, release: Release) {
        self.available = Some(release);
        self.error = None;
        self.checking = false;
        self.up_to_date = false;
    }

    /// Arranca una comprobación manual: se olvida el resultado anterior.
    pub fn start_check(&mut self) {
        self.checking = true;
        self.up_to_date = false;
        if self.phase != Some(DownloadPhase::Downloading) {
            self.error = None;
        }
    }

    /// Abre/cierra el panel. No hace nada si no hay ninguna release encontrada
    /// (no hay nada que mostrar en el panel).
    pub fn toggle_panel(&mut self) {
        if self.available.is_some() {
            self.panel_open = !self.panel_open;
        }
    }

    pub fn close_panel(&mut self) {
        self.panel_open = false;
    }

    /// Comprobación manual ("Buscar ahora") sin red o con error: mensaje claro,
    /// nunca en silencio (a diferencia del chequeo automático).
    pub fn set_manual_error(&mut self, msg: String) {
        self.error = Some(msg);
        self.phase = None;
        self.checking = false;
        self.up_to_date = false;
    }

    /// Comprobación manual sin errores, pero ya estás en la última versión: también
    /// hay que decir algo (el usuario lo pidió a propósito), aunque no haya panel.
    pub fn set_up_to_date(&mut self) {
        self.error = Some("ya tienes la última versión".to_string());
        self.phase = None;
        self.checking = false;
        self.up_to_date = true;
    }

    pub fn start_download(&mut self) {
        self.phase = Some(DownloadPhase::Downloading);
        self.progress = Some((0, 0));
        self.error = None;
    }

    pub fn set_progress(&mut self, done: u64, total: u64) {
        self.progress = Some((done, total));
    }

    pub fn start_verifying(&mut self) {
        self.phase = Some(DownloadPhase::Verifying);
    }

    /// La firma no cuadra: nunca se ejecuta el binario descargado. Ver Task 5 del
    /// plan y el `Global Constraints`: "Si la verificación falla, borra los
    /// archivos descargados y muestra un error".
    pub fn fail_verification(&mut self) {
        self.phase = Some(DownloadPhase::Failed);
        self.error = Some("La descarga no es auténtica".to_string());
    }

    pub fn download_error(&mut self, msg: String) {
        self.phase = Some(DownloadPhase::Failed);
        self.error = Some(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str) -> Release {
        Release { tag: format!("v{version}"), version: version.to_string(), body: "notas".to_string(), setup_url: String::new(), sig_url: String::new() }
    }

    #[test]
    fn no_notice_when_nothing_available() {
        assert_eq!(UpdateState::default().notice_text(), None);
    }

    #[test]
    fn notice_mentions_the_version() {
        let mut s = UpdateState::default();
        s.set_available(release("1.4.0"));
        assert_eq!(s.notice_text().as_deref(), Some("Actualización 1.4.0 disponible"));
    }

    #[test]
    fn toggle_panel_only_works_once_something_is_available() {
        let mut s = UpdateState::default();
        s.toggle_panel();
        assert!(!s.panel_open);
        s.set_available(release("1.4.0"));
        s.toggle_panel();
        assert!(s.panel_open);
        s.toggle_panel();
        assert!(!s.panel_open);
    }

    #[test]
    fn setting_available_clears_a_previous_error() {
        let mut s = UpdateState::default();
        s.set_manual_error("sin red".to_string());
        s.set_available(release("1.4.0"));
        assert_eq!(s.error, None);
    }

    #[test]
    fn settings_phase_follows_the_check() {
        let mut s = UpdateState::default();
        assert_eq!(s.phase_for_settings(), UpdatePhase::Idle);
        s.start_check();
        assert_eq!(s.phase_for_settings(), UpdatePhase::Checking);
        s.set_up_to_date();
        assert_eq!(s.phase_for_settings(), UpdatePhase::Idle);
        s.start_check();
        s.set_manual_error("sin red".to_string());
        assert_eq!(s.phase_for_settings(), UpdatePhase::Error("sin red".to_string()));
        s.start_check();
        s.set_available(release("9.0.0"));
        assert_eq!(s.phase_for_settings(), UpdatePhase::Found);
        s.start_download();
        s.set_progress(10, 100);
        assert_eq!(s.phase_for_settings(), UpdatePhase::Downloading(10, 100));
    }

    #[test]
    fn failed_verification_sets_the_auth_error_message() {
        let mut s = UpdateState::default();
        s.fail_verification();
        assert_eq!(s.error.as_deref(), Some("La descarga no es auténtica"));
        assert_eq!(s.phase, Some(DownloadPhase::Failed));
    }
}
