//! Recorrido guiado (spotlight) sobre la ventana principal: un único foco que se
//! desliza y cambia de forma entre objetivos reales de la interfaz
//! (`docs/mockups/setup/tutorial.html` → `.spot`/`.tip`). No es una ventana: es una
//! capa que `window.rs` pinta encima de todo lo demás en el mismo `WM_PAINT`
//! (`Renderer::begin_overlay`/`end_paint`, sin repetir el dibujo del documento).
//!
//! Desviación del plan: la interfaz propuesta (`start(frame_provider: impl Fn() ->
//! Frame) -> Tour`, con `draw(&self, renderer, frame, now)` recibiendo también un
//! `Frame` aparte) es redundante — si `Tour` ya guarda cómo obtener el `Frame`, no
//! hace falta que además se le pase uno en cada `draw`. Aquí `Tour` no guarda ningún
//! proveedor: cada llamada (`draw`/`advance`/`back`) recibe el `Frame` ya resuelto de
//! esa pasada de pintado (que además es justo lo que `window.rs` ya tiene a mano en
//! `WM_PAINT`, via `Renderer::current_frame`), y `Tour` se limita a recordar qué
//! parada era la anterior para poder animar la transición entre las dos (su rect se
//! recalcula en cada `draw` a partir de esa misma parada, en vez de congelarse en el
//! instante del clic: así una parada apuntando a un elemento que cambia de tamaño con
//! la ventana sigue interpolando desde el sitio correcto aunque el usuario redimensione
//! a media transición).

use std::time::{Duration, Instant};

use crate::keyboard_widget::{GREEN, key_down};
use crate::layout::{Frame, Rect};
use crate::Renderer;

/// Cuánto tarda el foco (la "vela con agujero") en deslizarse y cambiar de forma de
/// una parada a otra.
const SPOTLIGHT_MS: u64 = 450;
/// Duración de salida/entrada del contenido del globo (título+cuerpo+teclas) al
/// cambiar de parada, y de la propia entrada del recorrido al arrancar: mismos
/// tiempos que el resto del sistema de asistentes de la app (`notty-setup::ui`,
/// `STEP_OUT_MS`/`STEP_IN_MS`; `welcome_window::STEP_TRANSITION_MS`), para que se
/// sienta como el mismo asistente en vez de un widget aparte.
const STEP_OUT_MS: u64 = 140;
const STEP_IN_MS: u64 = 180;
const TIP_W: f32 = 250.0;
const TIP_PAD: f32 = 12.0;

/// Progreso `0.0..=1.0` con la curva de easing, `dur` después de `start`. `1.0` de
/// inmediato si las animaciones del sistema están desactivadas.
fn eased(now: Instant, start: Instant, dur: Duration, enabled: bool) -> f32 {
    if !enabled {
        return 1.0;
    }
    let dur = crate::anim::scaled(dur);
    let p = (now.saturating_duration_since(start).as_secs_f32() / dur.as_secs_f32()).clamp(0.0, 1.0);
    crate::ease_out_cubic(p)
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

pub struct Stop {
    pub rect: fn(&Frame) -> Rect,
    pub title: &'static str,
    pub body: &'static str,
    pub kbd: &'static [&'static str],
}

/// Copia tal cual de `docs/mockups/setup/tutorial.html` (función `stops`, líneas
/// 136-141), salvo la corrección de atajo del Task 4 Step 1: la maqueta dice
/// "Ctrl+T abre una nueva y Ctrl+W la cierra", pero el atajo real (`crates/
/// notty-input/src/command.rs`) es `Ctrl+N` para `NewTab` — `Ctrl+T` no está
/// enlazado a nada. El resto de cada parada (título, resto del cuerpo, a qué
/// elemento apunta) es el de la maqueta.
///
/// La primera parada apunta a `tabs_below` si esa banda existe (vista Clásica) o a
/// `titlebar` si no (vista Moderna, donde las pestañas viven en la propia barra de
/// título): `tabs_below` es un rect vacío en Moderna, así que señalarlo a pelo
/// dibujaría un foco invisible de tamaño cero en el preset por defecto.
pub const STOPS: &[Stop] = &[
    Stop {
        rect: |f| if f.tabs_below.is_empty() { f.titlebar } else { f.tabs_below },
        title: "Pestañas",
        body: "Ctrl+N abre una nueva y Ctrl+W la cierra. Arrastra para reordenar.",
        kbd: &["Ctrl+N", "Ctrl+W"],
    },
    Stop {
        rect: |f| f.settings_btn,
        title: "Ajustes",
        body: "Tema, estilo, fuente y modo de teclado, en cualquier momento.",
        kbd: &[],
    },
    Stop {
        rect: |f| f.body,
        title: "Abrir rápido",
        body: "Abre una ruta con autocompletado, sin diálogo.",
        kbd: &["Ctrl+O"],
    },
    Stop {
        rect: |f| f.status,
        title: "Barra de estado",
        body: "Línea, codificación y fin de línea. Haz clic en cualquiera para cambiarlo.",
        kbd: &[],
    },
    Stop {
        rect: |f| f.status,
        title: "Para curiosos",
        body: "Activa vim y muestra los bytes en crudo.",
        kbd: &["Ctrl+Alt+V", "Ctrl+Shift+H"],
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TourInput {
    /// El recorrido sigue.
    None,
    /// Esc, la ✕ del globo, o "Hecho" en la última parada: hay que soltar el
    /// `Option<Tour>` que lo mantiene vivo.
    Closed,
    /// El clic o la tecla no eran del recorrido: que los procese la app de debajo.
    Pass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Hit {
    #[default]
    None,
    Next,
    Back,
    Dot(usize),
    Close,
}

/// Transición en curso entre paradas: `from` es el índice de la que se está dejando
/// (para poder recalcular su rect y su contenido cada `draw`), `start` cuándo empezó.
#[derive(Clone, Copy)]
struct Transition {
    from: usize,
    start: Instant,
}

pub struct Tour {
    current: usize,
    transition: Option<Transition>,
    /// Cuándo arrancó el recorrido: el globo+vela se funden desde aquí en vez de
    /// aparecer de golpe en el primer fotograma (evita el "flash" al abrir).
    opened_at: Instant,
    animations_enabled: bool,
    hits: Vec<(Rect, Hit)>,
    /// Qué combinaciones de `kbd` de la parada actual ya ha pulsado el usuario.
    done: Vec<bool>,
}

fn label_vk(k: &str) -> u32 {
    match k {
        "Ctrl" => 0x11,
        "Shift" => 0x10,
        "Alt" => 0x12,
        s if s.chars().count() == 1 => s.chars().next().map_or(0, |c| c.to_ascii_uppercase() as u32),
        _ => 0,
    }
}

fn combo_vks(combo: &str) -> Vec<u32> {
    combo.split('+').map(label_vk).collect()
}

/// Dónde va el globo: debajo del foco, o encima; si no cabe en ninguno (el foco
/// ocupa casi toda la ventana, p.ej. el cuerpo del editor), dentro del propio foco.
fn tip_rect(hole: Rect, full: Rect, tip_h: f32, dx: f32) -> (Rect, bool) {
    let below = hole.bottom + 10.0;
    let above = hole.top - tip_h - 10.0;
    let (tx, ty, inside) = if below + tip_h <= full.bottom - 6.0 {
        (hole.left, below, false)
    } else if above >= 6.0 {
        (hole.left, above, false)
    } else {
        (hole.left + 16.0, hole.top + 16.0, true)
    };
    let tx = (tx + dx).clamp(10.0, (full.right - TIP_W - 10.0).max(10.0));
    (Rect::new(tx, ty, tx + TIP_W, ty + tip_h), inside)
}

impl Tour {
    /// Arranca en la primera parada. El foco y el globo entran con el mismo
    /// fundido+desplazamiento que una transición entre paradas (ver `draw`), en vez
    /// de aparecer ya a opacidad completa en el primer fotograma.
    pub fn start(animations_enabled: bool) -> Tour {
        Tour {
            current: 0,
            transition: None,
            opened_at: Instant::now(),
            animations_enabled,
            hits: Vec::new(),
            done: vec![false; STOPS[0].kbd.len()],
        }
    }

    fn current_rect(&self, frame: &Frame) -> Rect {
        (STOPS[self.current].rect)(frame)
    }

    /// Si hay alguna animación en curso (entrada, transición entre paradas, o el
    /// deslizamiento del foco) que necesite que seguir repintando a 60Hz para verse
    /// fluida en vez de a saltos (`window.rs` usa esto para mantener corriendo el
    /// `SetTimer` de animación mientras el recorrido esté activo).
    pub fn is_animating(&self, now: Instant) -> bool {
        if !self.animations_enabled {
            return false;
        }
        if now < self.opened_at + Duration::from_millis(STEP_IN_MS) {
            return true;
        }
        if let Some(t) = self.transition {
            if now < t.start + Duration::from_millis(SPOTLIGHT_MS.max(STEP_OUT_MS + STEP_IN_MS)) {
                return true;
            }
        }
        // El pulso continuo de las keycaps (Task #1) necesita repintar mientras la
        // parada actual señale una combinación de teclas, incluso en reposo.
        !STOPS[self.current].kbd.is_empty()
    }

    fn begin_transition(&mut self, from: usize, now: Instant) {
        self.transition = Some(Transition { from, start: now });
        self.done = vec![false; STOPS[self.current].kbd.len()];
    }

    /// Avanza una parada. Si ya estaba en la última ("Hecho"), no cambia de parada y
    /// devuelve `TourInput::Closed` para que `window.rs` suelte el recorrido.
    pub fn next(&mut self, now: Instant) -> TourInput {
        if self.current + 1 >= STOPS.len() {
            return TourInput::Closed;
        }
        let from = self.current;
        self.current += 1;
        self.begin_transition(from, now);
        TourInput::None
    }

    pub fn back(&mut self, now: Instant) {
        if self.current == 0 {
            return;
        }
        let from = self.current;
        self.current -= 1;
        self.begin_transition(from, now);
    }

    fn go_to(&mut self, target: usize, now: Instant) {
        if target == self.current || target >= STOPS.len() {
            return;
        }
        let from = self.current;
        self.current = target;
        self.begin_transition(from, now);
    }

    /// Esc cierra; cualquier otra tecla sigue hasta la app, para que las combinaciones
    /// que se enseñan (Ctrl+N, Ctrl+W...) hagan de verdad lo que dicen.
    pub fn handle_key(&mut self, vk: u32) -> TourInput {
        if vk == 0x1B { TourInput::Closed } else { TourInput::Pass }
    }

    /// Clic en `(x, y)` (DIPs de la ventana principal). Dentro del globo navega o
    /// cierra (✕); en cualquier otro sitio, el clic es de la app de debajo.
    pub fn handle_click(&mut self, x: f32, y: f32, now: Instant) -> TourInput {
        for &(r, h) in self.hits.iter().rev() {
            if r.contains(x, y) {
                match h {
                    Hit::Next => return self.next(now),
                    Hit::Back => {
                        self.back(now);
                        return TourInput::None;
                    }
                    Hit::Dot(i) => {
                        self.go_to(i, now);
                        return TourInput::None;
                    }
                    Hit::Close => return TourInput::Closed,
                    Hit::None => return TourInput::None,
                }
            }
        }
        TourInput::Pass
    }

    /// Dibuja la vela + foco + globo de la parada actual, interpolando desde la
    /// parada anterior si hay una transición en curso. Registra las zonas clicables
    /// (Siguiente/Atrás/puntos) para el siguiente `handle_click`.
    pub fn draw(&mut self, r: &Renderer, frame: &Frame, dark: bool, accent: notty_config::AccentColor, now: Instant) {
        self.hits.clear();
        let pal = &crate::theme::palette(dark, accent);
        let (w, h) = r.size_dips();
        let full = Rect::new(0.0, 0.0, w, h);

        // Fundido de entrada de toda la capa (vela+foco+globo): al arrancar el
        // recorrido, o mientras dura, este factor multiplica la opacidad de todo lo
        // que se dibuje a continuación (`Renderer::set_fade`). Antes la vela aparecía
        // ya a su opacidad final en el primer fotograma — con el foco ya en su sitio y
        // el anillo ya dibujado — lo que se veía como un flash en vez de una entrada;
        // ahora entra fundiéndose, igual que el resto de asistentes de la app.
        let open_t = eased(now, self.opened_at, Duration::from_millis(STEP_IN_MS), self.animations_enabled);

        let target = self.current_rect(frame);
        let hole = match self.transition {
            Some(t) if now < t.start + Duration::from_millis(SPOTLIGHT_MS) || !self.animations_enabled => {
                let prev = (STOPS[t.from].rect)(frame);
                let s = eased(now, t.start, Duration::from_millis(SPOTLIGHT_MS), self.animations_enabled);
                Rect::new(
                    lerp(prev.left - 4.0, target.left - 4.0, s),
                    lerp(prev.top - 4.0, target.top - 4.0, s),
                    lerp(prev.right + 4.0, target.right + 4.0, s),
                    lerp(prev.bottom + 4.0, target.bottom + 4.0, s),
                )
            }
            _ => Rect::new(target.left - 4.0, target.top - 4.0, target.right + 4.0, target.bottom + 4.0),
        };

        let kbd = STOPS[self.current].kbd;
        if let Some(i) = self.done.iter().position(|d| !d) {
            if combo_vks(kbd[i]).iter().all(|&vk| key_down(vk)) {
                self.done[i] = true;
            }
        }

        r.begin_overlay();
        r.set_fade(open_t);
        r.fill_veil_with_hole(full, hole, 6.0, crate::theme::Rgba(0.039, 0.039, 0.047, 0.62), pal.accent);
        self.draw_tooltip(r, pal, hole, full, now, open_t);
        // El fade es del `Renderer` compartido con el `paint()` normal de la ventana
        // (esta capa se pinta en la misma pasada, ver `window.rs`): hay que devolverlo
        // a 1.0 o el próximo fotograma normal (sin recorrido, o ya en reposo) heredaría
        // esta opacidad reducida y todo se vería atenuado sin motivo.
        r.set_fade(1.0);
        r.end_paint();
    }

    /// Dibuja el globo, con el contenido (título/cuerpo/teclas) saliendo 12px a la
    /// izquierda con fundido (`STEP_OUT_MS`) y el nuevo entrando desde la derecha
    /// (`STEP_IN_MS`, ease-out) cuando hay una transición en curso — mismo
    /// out-luego-in que `notty-setup::ui` y `welcome_window` usan para sus propios
    /// pasos, en vez del corte seco de antes (el contenido cambiaba de golpe aunque
    /// el foco ya se deslizaba suavemente).
    fn draw_tooltip(&mut self, r: &Renderer, pal: &crate::theme::Palette, hole: Rect, full: Rect, now: Instant, open_t: f32) {
        let out_d = Duration::from_millis(STEP_OUT_MS);
        let in_d = Duration::from_millis(STEP_IN_MS);

        // Igual que `notty-setup::ui::paint`: durante los primeros `STEP_OUT_MS` de una
        // transición se dibuja *la parada anterior* saliendo (fundido lineal 1→0,
        // deslizamiento 0→-12px); solo cuando esa salida termina empieza a entrar la
        // nueva (fundido+deslizamiento con ease-out). Si las animaciones están
        // desactivadas, o no hay transición, se pinta solo la parada actual a opacidad
        // completa, como antes.
        let in_start = match self.transition {
            Some(t) if self.animations_enabled && now < t.start + out_d => {
                let lin = now.saturating_duration_since(t.start).as_secs_f32() / out_d.as_secs_f32();
                self.draw_stop_content(r, pal, &STOPS[t.from], hole, full, -12.0 * lin, (1.0 - lin) * open_t, false, now);
                None
            }
            Some(t) => Some(t.start + out_d),
            None => None,
        };
        let v = match in_start {
            Some(start) => eased(now, start, in_d, self.animations_enabled),
            None => 1.0,
        };
        self.draw_stop_content(r, pal, &STOPS[self.current], hole, full, 12.0 * (1.0 - v), v * open_t, true, now);
    }

    /// Dibuja el globo de una parada dada (no necesariamente la actual: la saliente
    /// durante una transición también se dibuja con esta misma función) desplazado
    /// `dx` y con opacidad `fade`. `register_hits` es `false` para la parada saliente
    /// (no debe robarle el clic a la que está entrando).
    #[allow(clippy::too_many_arguments)]
    fn draw_stop_content(
        &mut self,
        r: &Renderer,
        pal: &crate::theme::Palette,
        stop: &Stop,
        hole: Rect,
        full: Rect,
        dx: f32,
        fade: f32,
        register_hits: bool,
        now: Instant,
    ) {
        if fade <= 0.0 {
            return;
        }
        r.set_fade(fade);

        let kbd_h = if stop.kbd.is_empty() { 0.0 } else { 28.0 };
        // El cuerpo se parte en líneas al ancho real del globo y el globo crece con él.
        let body_line_h = 11.5 * 1.45;
        let body_w = TIP_W - TIP_PAD * 2.0 - 18.0;
        let body = r.tr(stop.body);
        let body_lines_h = r.measure_wrapped(body, &r.fonts().ui_11_5, body_w, Some(body_line_h));
        let tip_h = TIP_PAD * 2.0 + 16.0 + 3.0 + body_lines_h + kbd_h + 8.0 + 18.0;

        let (tip, inside) = tip_rect(hole, full, tip_h, dx);
        let done: Vec<bool> = if register_hits { self.done.clone() } else { vec![false; stop.kbd.len()] };

        if !stop.kbd.is_empty() {
            let at_bottom = inside || tip.top + tip.height() / 2.0 < full.height() / 2.0;
            self.draw_keyboard(r, pal, stop.kbd, &done, full, at_bottom, dx, now);
        }

        r.fill_round(tip, 8.0, pal.chrome_hi);
        r.stroke_round_rect(tip, 8.0, 1.0, pal.shadow_ring);
        if register_hits {
            self.hits.push((tip, Hit::None));
        }
        let close_r = Rect::new(tip.right - 26.0, tip.top + 6.0, tip.right - 6.0, tip.top + 26.0);
        let (ccx, ccy) = (close_r.left + 10.0, close_r.top + 10.0);
        r.stroke_line(ccx - 4.0, ccy - 4.0, ccx + 4.0, ccy + 4.0, 1.2, pal.text_3);
        r.stroke_line(ccx + 4.0, ccy - 4.0, ccx - 4.0, ccy + 4.0, 1.2, pal.text_3);
        if register_hits {
            self.hits.push((close_r, Hit::Close));
        }

        let title_r = Rect::new(tip.left + TIP_PAD, tip.top + TIP_PAD, tip.right - TIP_PAD - 18.0, tip.top + TIP_PAD + 16.0);
        r.text(r.tr(stop.title), &r.fonts().ui_13, title_r, pal.text);
        let body_r = Rect::new(title_r.left, title_r.bottom + 3.0, title_r.left + body_w, title_r.bottom + 3.0 + body_lines_h);
        r.text_wrapped(body, &r.fonts().ui_11_5, body_r, pal.text_2, Some(body_line_h));

        let mut kbd_bottom = body_r.bottom;
        if !stop.kbd.is_empty() {
            kbd_bottom = self.draw_keycaps(r, pal, stop.kbd, &done, body_r.left, body_r.bottom, register_hits, now);
        }
        let _ = kbd_bottom;

        let nav_r = Rect::new(body_r.left, tip.bottom - TIP_PAD - 18.0, body_r.right, tip.bottom - TIP_PAD);

        // Puntos de progreso (izquierda).
        let dot_d = 5.0;
        let dot_gap = 4.0;
        let mut dxp = nav_r.left;
        for i in 0..STOPS.len() {
            let on = i == self.current;
            let dw = if on { 14.0 } else { dot_d };
            let dr = Rect::new(dxp, nav_r.top + (nav_r.height() - dot_d) / 2.0, dxp + dw, nav_r.top + (nav_r.height() - dot_d) / 2.0 + dot_d);
            r.fill_round(dr, dot_d / 2.0, if on { pal.accent } else { pal.text_3 });
            if register_hits {
                self.hits.push((dr, Hit::Dot(i)));
            }
            dxp += dw + dot_gap;
        }

        // "Siguiente"/"Hecho" (derecha) + "← Atrás" si no es la primera parada.
        let is_last = self.current + 1 == STOPS.len();
        let next_label = r.tr(if is_last { "Hecho" } else { "Siguiente →" });
        let next_w = r.measure(next_label, &r.fonts().ui_12);
        let next_r = Rect::new(nav_r.right - next_w, nav_r.top, nav_r.right, nav_r.bottom);
        r.text(next_label, &r.fonts().ui_12, next_r, pal.accent);
        if register_hits {
            self.hits.push((next_r, Hit::Next));
        }

        if self.current > 0 {
            let back_label = r.tr("← Atrás");
            let back_w = r.measure(back_label, &r.fonts().ui_12);
            let back_r = Rect::new(next_r.left - 10.0 - back_w, nav_r.top, next_r.left - 10.0, nav_r.bottom);
            r.text(back_label, &r.fonts().ui_12, back_r, pal.text_2);
            if register_hits {
                self.hits.push((back_r, Hit::Back));
            }
        }
    }

    /// Fila de "keycaps" (Task #1): un pequeño teclado esquemático con la combinación
    /// de la parada actual, en vez de solo texto monoespaciado. Cada tecla es una
    /// tecla-rect redondeada con su etiqueta centrada (`fill_round` + `text_center`,
    /// consistente con el resto de la paleta oscura minimalista de la app — nada de
    /// iconos ni un teclado completo, solo las teclas relevantes). Además de entrar
    /// con el mismo fundido que el resto del globo, la tecla pulsa suavemente (un
    /// halo de acento que sube y baja) para que se note cuál es la que hay que
    /// presionar sin recurrir a una animación de "tecla presionada" completa.
    #[allow(clippy::too_many_arguments)]
    fn draw_keycaps(
        &self,
        r: &Renderer,
        pal: &crate::theme::Palette,
        keys: &[&str],
        done: &[bool],
        left: f32,
        top: f32,
        animate: bool,
        now: Instant,
    ) -> f32 {
        const KEY_H: f32 = 22.0;
        const KEY_PAD_X: f32 = 8.0;
        const KEY_GAP: f32 = 6.0;
        let y = top + 6.0;

        // Pulso continuo (no ligado a ninguna transición): un seno lento entre 0 y 1,
        // contado desde que arrancó el recorrido, para que todas las teclas de la fila
        // respiren juntas. Se apaga (queda fijo a 0) si las animaciones del sistema
        // están desactivadas, para no animar nada en ese caso.
        let pulse = if animate && self.animations_enabled {
            let t = now.saturating_duration_since(self.opened_at).as_secs_f32();
            0.5 + 0.5 * (t * std::f32::consts::TAU * 0.9).sin()
        } else {
            0.0
        };

        let next = done.iter().position(|d| !d);
        let mut kx = left;
        for (i, k) in keys.iter().enumerate() {
            let is_done = done.get(i).copied().unwrap_or(false);
            let extra = if is_done { 14.0 } else { 0.0 };
            let kw = r.measure(k, &r.fonts().mono_11) + KEY_PAD_X * 2.0 + extra;
            let kr = Rect::new(kx, y, kx + kw, y + KEY_H);
            if pulse > 0.0 && next == Some(i) {
                let glow = crate::theme::Rgba(pal.accent.0, pal.accent.1, pal.accent.2, 0.10 + 0.16 * pulse);
                r.fill_round(Rect::new(kr.left - 2.0, kr.top - 2.0, kr.right + 2.0, kr.bottom + 2.0), 6.0, glow);
            }
            if is_done {
                r.fill_round(kr, 4.0, crate::theme::Rgba(GREEN.0, GREEN.1, GREEN.2, 0.22));
                r.text_center(&format!("{k} ✓"), &r.fonts().mono_11, kr, GREEN);
            } else {
                r.fill_round(kr, 4.0, pal.hover);
                r.text_center(k, &r.fonts().mono_11, kr, pal.text_2);
            }
            kx += kw + KEY_GAP;
        }
        y + KEY_H
    }

    /// Teclado completo aparte del globo: las teclas de la combinación que toca en
    /// azul (respirando), y en verde mientras se mantienen pulsadas.
    #[allow(clippy::too_many_arguments)]
    fn draw_keyboard(
        &self,
        r: &Renderer,
        pal: &crate::theme::Palette,
        keys: &[&str],
        done: &[bool],
        full: Rect,
        at_bottom: bool,
        dx: f32,
        now: Instant,
    ) {
        const PAD: f32 = 12.0;
        const CAPTION_H: f32 = 22.0;
        let u = crate::keyboard_widget::unit_for_width(full.width() - 64.0 - PAD * 2.0);
        let panel_w = u * crate::keyboard_widget::UNITS_W + PAD * 2.0;
        let panel_h = PAD + CAPTION_H + u * crate::keyboard_widget::UNITS_H + PAD;
        let left = full.left + (full.width() - panel_w) / 2.0 + dx;
        let top = if at_bottom { full.bottom - panel_h - 34.0 } else { full.top + 48.0 };
        let panel = Rect::new(left, top, left + panel_w, top + panel_h);

        let all_done = !done.is_empty() && done.iter().all(|d| *d);
        let target = done.iter().position(|d| !d).unwrap_or(keys.len().saturating_sub(1));
        let target_vks = keys.get(target).map(|k| combo_vks(k)).unwrap_or_default();
        let pulse = if self.animations_enabled {
            let t = now.saturating_duration_since(self.opened_at).as_secs_f32();
            0.5 + 0.5 * (t * std::f32::consts::TAU * 0.9).sin()
        } else {
            1.0
        };

        r.fill_round(panel, 10.0, pal.chrome_hi);
        r.stroke_round_rect(panel, 10.0, 1.0, pal.shadow_ring);

        let caption = if all_done {
            r.tr("¡Eso es!").to_string()
        } else {
            format!("{} {}", r.tr("Pulsa"), keys.get(target).map_or(String::new(), |k| k.replace('+', " + ")))
        };
        let cap_r = Rect::new(panel.left + PAD, panel.top + PAD - 2.0, panel.right - PAD, panel.top + PAD + CAPTION_H - 6.0);
        r.text_center(&caption, &r.fonts().ui_12_semibold, cap_r, if all_done { GREEN } else { pal.text });

        let pressed: Vec<u32> =
            target_vks.iter().copied().filter(|&vk| vk != 0 && (all_done || key_down(vk))).collect();
        crate::keyboard_widget::draw(r, pal, panel.left + PAD, panel.top + PAD + CAPTION_H, u, &target_vks, &pressed, pulse, 0.0);
    }
}
