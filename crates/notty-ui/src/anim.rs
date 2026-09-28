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

/// `cubic-bezier(x1, y1, x2, y2)` de CSS evaluada en `t` (tiempo `0..=1`): se
/// resuelve `x(s) = t` por Newton (con bisección de respaldo) y se devuelve `y(s)`.
/// `y` puede salirse de `0..=1` (curvas con rebote como `Curve::Spring`).
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 || t >= 1.0 {
        return t;
    }
    let bez = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    };
    let d_bez = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * a + 6.0 * u * s * (b - a) + 3.0 * s * s * (1.0 - b)
    };
    let mut s = t;
    for _ in 0..8 {
        let x = bez(x1, x2, s) - t;
        if x.abs() < 1e-5 {
            return bez(y1, y2, s);
        }
        let dx = d_bez(x1, x2, s);
        if dx.abs() < 1e-6 {
            break;
        }
        s = (s - x / dx).clamp(0.0, 1.0);
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    s = t;
    for _ in 0..30 {
        let x = bez(x1, x2, s);
        if (x - t).abs() < 1e-5 {
            break;
        }
        if x < t {
            lo = s;
        } else {
            hi = s;
        }
        s = (lo + hi) / 2.0;
    }
    bez(y1, y2, s)
}

/// Curvas de la maqueta de Ajustes (`--spring` y `--out`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    Linear,
    /// `cubic-bezier(.2,.8,.2,1)`.
    Out,
    /// `cubic-bezier(.3,1.45,.5,1)`: se pasa un poco y vuelve.
    Spring,
    /// `ease-in-out` de CSS.
    InOut,
}

impl Curve {
    pub fn apply(self, t: f32) -> f32 {
        match self {
            Curve::Linear => t.clamp(0.0, 1.0),
            Curve::Out => cubic_bezier(0.2, 0.8, 0.2, 1.0, t),
            Curve::Spring => cubic_bezier(0.3, 1.45, 0.5, 1.0, t),
            Curve::InOut => cubic_bezier(0.42, 0.0, 0.58, 1.0, t),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
    curve: Curve,
}

impl Tween {
    fn value(&self, now: Instant) -> f32 {
        if now <= self.start {
            return self.from;
        }
        let p = now.saturating_duration_since(self.start).as_secs_f32() / self.duration.as_secs_f32().max(1e-6);
        if p >= 1.0 {
            return self.to;
        }
        self.from + (self.to - self.from) * self.curve.apply(p)
    }

    fn done(&self, now: Instant) -> bool {
        now >= self.start + self.duration
    }
}

/// Valores animados "hacia un objetivo", al estilo de `transition` de CSS: quien
/// dibuja pide `to(clave, objetivo, ...)` en cada fotograma y recibe el valor de
/// ahora; si el objetivo cambió, arranca una transición desde donde estuviera.
/// Así cada control se anima sin guardar su propio estado de animación.
#[derive(Debug)]
pub struct Tweens<K: std::hash::Hash + Eq + Copy> {
    map: std::collections::HashMap<K, Tween>,
    /// Sin animaciones (Windows con las animaciones apagadas): todo salta al objetivo.
    pub disabled: bool,
}

impl<K: std::hash::Hash + Eq + Copy> Default for Tweens<K> {
    fn default() -> Self {
        Self { map: std::collections::HashMap::new(), disabled: false }
    }
}

impl<K: std::hash::Hash + Eq + Copy> Tweens<K> {
    pub fn new(disabled: bool) -> Self {
        Self { map: std::collections::HashMap::new(), disabled }
    }

    /// Valor actual de `key` yendo hacia `target`. La primera vez que se pide una
    /// clave empieza ya en `target` (sin animar la aparición: para eso está `from_to`).
    pub fn to(&mut self, key: K, target: f32, ms: u64, curve: Curve, now: Instant) -> f32 {
        self.to_delayed(key, target, ms, 0, curve, now)
    }

    /// Como `to`, pero la transición empieza `delay_ms` después del cambio de objetivo
    /// (`transition-delay`).
    pub fn to_delayed(&mut self, key: K, target: f32, ms: u64, delay_ms: u64, curve: Curve, now: Instant) -> f32 {
        let still = Tween { from: target, to: target, start: now, duration: Duration::ZERO, curve };
        if self.disabled {
            self.map.insert(key, still);
            return target;
        }
        let tw = self.map.entry(key).or_insert(still);
        if tw.to != target {
            let cur = tw.value(now);
            *tw = Tween {
                from: cur,
                to: target,
                start: now + scaled_ms(delay_ms),
                duration: scaled_ms(ms),
                curve,
            };
        }
        tw.value(now)
    }

    /// Como `to`, pero si la clave es nueva arranca en `from` (animación de entrada).
    pub fn from_to(&mut self, key: K, from: f32, target: f32, ms: u64, curve: Curve, now: Instant) -> f32 {
        if !self.disabled && !self.map.contains_key(&key) {
            self.map.insert(key, Tween { from, to: target, start: now, duration: scaled_ms(ms), curve });
        }
        self.to(key, target, ms, curve, now)
    }

    /// Fija el valor sin animar.
    pub fn set(&mut self, key: K, value: f32, now: Instant) {
        self.map.insert(key, Tween { from: value, to: value, start: now, duration: Duration::ZERO, curve: Curve::Linear });
    }

    /// Arranca una transición de `from` a `to` ya mismo, aunque el objetivo no cambie
    /// (un destello que se repite, p.ej.).
    pub fn kick(&mut self, key: K, from: f32, to: f32, ms: u64, curve: Curve, now: Instant) {
        if self.disabled {
            self.set(key, to, now);
        } else {
            self.map.insert(key, Tween { from, to, start: now, duration: scaled_ms(ms), curve });
        }
    }

    pub fn remove(&mut self, key: K) {
        self.map.remove(&key);
    }

    /// Si queda alguna transición a medias (hay que seguir repintando).
    pub fn animating(&self, now: Instant) -> bool {
        self.map.values().any(|t| !t.done(now))
    }

    /// Olvida las transiciones terminadas cuyo valor es 0 (hover que ya se fue...),
    /// para que el mapa no crezca sin límite.
    pub fn prune(&mut self, now: Instant) {
        self.map.retain(|_, t| !(t.done(now) && t.to == 0.0));
    }
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
        Self { start, duration: scaled(duration), enabled: true }
    }

    pub fn new_maybe(start: Instant, duration: Duration, enabled: bool) -> Self {
        Self { start, duration: scaled(duration), enabled }
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


// --- Frecuencia y animaciones reducidas (Ajustes → Ventana) ---------------------------

use std::sync::atomic::{AtomicU32, Ordering};

/// Multiplicador de todas las duraciones (bits de un `f32`): 1.0, o 0.5 con
/// "Animaciones reducidas".
static DURATION_SCALE: AtomicU32 = AtomicU32::new(0x3F80_0000);
/// Milisegundos entre frames mientras hay algo animándose.
static FRAME_MS: AtomicU32 = AtomicU32::new(16);
/// Temporizadores (ventana, id) que pidieron resolución de 1 ms (`timeBeginPeriod`),
/// para devolverla al pararlos.
static HI_RES: std::sync::Mutex<Vec<(usize, usize)>> = std::sync::Mutex::new(Vec::new());

/// Aplica la frecuencia y el modo reducido de `ui`. Se llama al arrancar y cada vez
/// que cambia algo en Ajustes; afecta a las animaciones que empiecen desde entonces.
pub fn configure(ui: &notty_config::UiConfig) {
    let (scale, ms) = frame_settings(ui);
    DURATION_SCALE.store(scale.to_bits(), Ordering::Relaxed);
    FRAME_MS.store(ms, Ordering::Relaxed);
}

/// (multiplicador de duración, ms entre frames).
fn frame_settings(ui: &notty_config::UiConfig) -> (f32, u32) {
    use notty_config::AnimHz;
    if ui.reduced_motion {
        return (0.5, 33);
    }
    let ms = match ui.anim_hz {
        AnimHz::Hz30 => 33,
        AnimHz::Hz60 => 16,
        AnimHz::Hz120 => 8,
    };
    (1.0, ms)
}

pub(crate) fn scaled(d: Duration) -> Duration {
    d.mul_f32(f32::from_bits(DURATION_SCALE.load(Ordering::Relaxed)))
}

fn scaled_ms(ms: u64) -> Duration {
    scaled(Duration::from_millis(ms))
}

/// Arranca el temporizador de frames `id` de `hwnd` a la frecuencia configurada. Por
/// debajo de ~15 ms Windows redondea `SetTimer` a su resolución por defecto (15,6 ms):
/// para 120 Hz se pide resolución de 1 ms, solo mientras dure la animación.
pub fn start_frame_timer(hwnd: windows::Win32::Foundation::HWND, id: usize) {
    let ms = FRAME_MS.load(Ordering::Relaxed);
    unsafe {
        if ms < 15 {
            let mut hi = HI_RES.lock().unwrap_or_else(|e| e.into_inner());
            if !hi.contains(&(hwnd.0 as usize, id)) {
                hi.push((hwnd.0 as usize, id));
                let _ = windows::Win32::Media::timeBeginPeriod(1);
            }
        }
        let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(Some(hwnd), id, ms, None);
    }
}

/// Para el temporizador de `start_frame_timer` (y devuelve la resolución de 1 ms si la pidió).
pub fn stop_frame_timer(hwnd: windows::Win32::Foundation::HWND, id: usize) {
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(Some(hwnd), id);
        let mut hi = HI_RES.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = hi.iter().position(|&k| k == (hwnd.0 as usize, id)) {
            hi.swap_remove(i);
            let _ = windows::Win32::Media::timeEndPeriod(1);
        }
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
    fn cubic_bezier_hits_the_ends_and_spring_overshoots() {
        assert_eq!(Curve::Spring.apply(0.0), 0.0);
        assert_eq!(Curve::Spring.apply(1.0), 1.0);
        let peak = (1..100).map(|i| Curve::Spring.apply(i as f32 / 100.0)).fold(0.0f32, f32::max);
        assert!(peak > 1.02, "{peak}");
        assert!(Curve::Out.apply(0.5) > 0.5);
        assert!((cubic_bezier(0.0, 0.0, 1.0, 1.0, 0.3) - 0.3).abs() < 1e-3);
    }

    #[test]
    fn tween_starts_at_the_first_target_and_then_animates() {
        let now = Instant::now();
        let mut tw: Tweens<u8> = Tweens::default();
        assert_eq!(tw.to(1, 0.0, 100, Curve::Linear, now), 0.0);
        assert!(!tw.animating(now));
        assert_eq!(tw.to(1, 1.0, 100, Curve::Linear, now), 0.0);
        assert!(tw.animating(now));
        let mid = tw.to(1, 1.0, 100, Curve::Linear, now + Duration::from_millis(50));
        assert!((mid - 0.5).abs() < 1e-3);
        assert_eq!(tw.to(1, 1.0, 100, Curve::Linear, now + Duration::from_millis(100)), 1.0);
    }

    #[test]
    fn tween_retarget_starts_from_the_current_value() {
        let now = Instant::now();
        let mut tw: Tweens<u8> = Tweens::default();
        tw.to(1, 0.0, 100, Curve::Linear, now);
        tw.to(1, 1.0, 100, Curve::Linear, now);
        let half = now + Duration::from_millis(50);
        assert!((tw.to(1, 0.0, 100, Curve::Linear, half) - 0.5).abs() < 1e-3);
        assert!((tw.to(1, 0.0, 100, Curve::Linear, half + Duration::from_millis(50)) - 0.25).abs() < 1e-3);
    }

    #[test]
    fn disabled_tweens_jump_to_the_target() {
        let now = Instant::now();
        let mut tw: Tweens<u8> = Tweens { disabled: true, ..Default::default() };
        tw.to(1, 0.0, 100, Curve::Linear, now);
        assert_eq!(tw.to(1, 1.0, 100, Curve::Linear, now), 1.0);
        assert_eq!(tw.from_to(2, 0.0, 1.0, 100, Curve::Linear, now), 1.0);
        assert!(!tw.animating(now));
    }

    #[test]
    fn delayed_tween_waits_before_moving() {
        let now = Instant::now();
        let mut tw: Tweens<u8> = Tweens::default();
        tw.to(1, 0.0, 100, Curve::Linear, now);
        assert_eq!(tw.to_delayed(1, 1.0, 100, 50, Curve::Linear, now), 0.0);
        assert_eq!(tw.to_delayed(1, 1.0, 100, 50, Curve::Linear, now + Duration::from_millis(40)), 0.0);
        assert!(tw.to_delayed(1, 1.0, 100, 50, Curve::Linear, now + Duration::from_millis(100)) > 0.4);
    }

    #[test]
    fn reduced_motion_skips_straight_to_the_end() {
        let start = Instant::now();
        let a = Anim::new_maybe(start, Duration::from_millis(100), false);
        assert!(a.is_done(start)); // sin animación: "terminada" desde el instante 0
        assert_eq!(a.value(start, 10.0, 20.0), 20.0);
    }
}
