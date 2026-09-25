//! Interpolación con el tiempo para las animaciones cortas de la interfaz (menús,
//! sugerencias, pestañas, el interruptor de Ajustes...). Sin acceso al reloj: todo
//! recibe `now` como parámetro, para poder testear con instantes fijos.

use std::time::{Duration, Instant};

/// Ease-out cúbico: arranca rápido y frena hacia el final, la sensación estándar de
/// "algo que aparece" en vez de un lineal (que se siente mecánico) o un ease-in
/// (que se siente lento de arrancar, mal para algo que el usuario acaba de pedir).
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

#[derive(Debug, Clone, Copy)]
pub struct Anim {
    start: Instant,
    duration: Duration,
    /// Si es `false`, `progress`/`value` devuelven el estado final desde `start`
    /// (animaciones del sistema desactivadas en Windows: ver `system_animations_enabled`).
    enabled: bool,
}

impl Anim {
    pub fn new(start: Instant, duration: Duration) -> Self {
        Self { start, duration, enabled: true }
    }

    pub fn new_maybe(start: Instant, duration: Duration, enabled: bool) -> Self {
        Self { start, duration, enabled }
    }

    /// `0.0..=1.0` lineal en el tiempo transcurrido, sin la curva de easing.
    pub fn progress(&self, now: Instant) -> f32 {
        if !self.enabled || self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.start).as_secs_f32();
        (elapsed / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    pub fn is_done(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }

    /// Valor interpolado entre `from` y `to`, con la curva de easing aplicada.
    pub fn value(&self, now: Instant, from: f32, to: f32) -> f32 {
        let t = ease_out_cubic(self.progress(now));
        from + (to - from) * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn progress_is_zero_at_start_and_one_when_done() {
        let start = Instant::now();
        let a = Anim::new(start, Duration::from_millis(100));
        assert_eq!(a.progress(start), 0.0);
        assert_eq!(a.progress(start + Duration::from_millis(100)), 1.0);
        assert_eq!(a.progress(start + Duration::from_millis(500)), 1.0); // no se pasa de 1
    }

    #[test]
    fn progress_is_linear_in_time_before_easing() {
        let start = Instant::now();
        let a = Anim::new(start, Duration::from_millis(100));
        assert_eq!(a.progress(start + Duration::from_millis(50)), 0.5);
    }

    #[test]
    fn is_done_matches_progress_reaching_one() {
        let start = Instant::now();
        let a = Anim::new(start, Duration::from_millis(100));
        assert!(!a.is_done(start + Duration::from_millis(99)));
        assert!(a.is_done(start + Duration::from_millis(100)));
    }

    #[test]
    fn ease_out_cubic_starts_fast_and_settles() {
        // Ease-out: a mitad de tiempo ya se ha recorrido más de la mitad del camino.
        assert!(ease_out_cubic(0.5) > 0.5);
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
    }

    #[test]
    fn eased_value_interpolates_from_to() {
        let start = Instant::now();
        let a = Anim::new(start, Duration::from_millis(100));
        assert_eq!(a.value(start, 10.0, 20.0), 10.0);
        assert_eq!(a.value(start + Duration::from_millis(100), 10.0, 20.0), 20.0);
    }

    #[test]
    fn reduced_motion_skips_straight_to_the_end() {
        let start = Instant::now();
        let a = Anim::new_maybe(start, Duration::from_millis(100), false);
        assert!(a.is_done(start)); // sin animación: "terminada" desde el instante 0
        assert_eq!(a.value(start, 10.0, 20.0), 20.0);
    }
}
