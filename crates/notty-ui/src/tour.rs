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
//! `WM_PAINT`, via `Renderer::current_frame`), y `Tour` se limita a recordar el
//! rectángulo del objetivo anterior para poder animar la transición entre los dos.

use std::time::{Duration, Instant};

use crate::layout::{Frame, Rect};
use crate::{Anim, Renderer};

const SPOTLIGHT_MS: u64 = 450;
const TIP_W: f32 = 210.0;
const TIP_PAD: f32 = 12.0;

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
    /// Esc, clic fuera del foco, o "Hecho" en la última parada: hay que soltar el
    /// `Option<Tour>` que lo mantiene vivo.
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Hit {
    #[default]
    None,
    Next,
    Back,
    Dot(usize),
}

pub struct Tour {
    current: usize,
    prev_rect: Option<Rect>,
    anim: Option<Anim>,
    animations_enabled: bool,
    hits: Vec<(Rect, Hit)>,
}

impl Tour {
    /// Arranca en la primera parada, sin animación de entrada (el foco aparece ya en
    /// su sitio la primera vez; solo los cambios *entre* paradas se animan).
    pub fn start(animations_enabled: bool) -> Tour {
        Tour { current: 0, prev_rect: None, anim: None, animations_enabled, hits: Vec::new() }
    }

    fn current_rect(&self, frame: &Frame) -> Rect {
        (STOPS[self.current].rect)(frame)
    }

    fn begin_transition(&mut self, frame: &Frame, from: usize, now: Instant) {
        self.prev_rect = Some((STOPS[from].rect)(frame));
        self.anim = Some(Anim::new_maybe(now, Duration::from_millis(SPOTLIGHT_MS), self.animations_enabled));
    }

    /// Avanza una parada. Si ya estaba en la última ("Hecho"), no cambia de parada y
    /// devuelve `TourInput::Closed` para que `window.rs` suelte el recorrido.
    pub fn next(&mut self, frame: &Frame, now: Instant) -> TourInput {
        if self.current + 1 >= STOPS.len() {
            return TourInput::Closed;
        }
        let from = self.current;
        self.current += 1;
        self.begin_transition(frame, from, now);
        TourInput::None
    }

    pub fn back(&mut self, frame: &Frame, now: Instant) {
        if self.current == 0 {
            return;
        }
        let from = self.current;
        self.current -= 1;
        self.begin_transition(frame, from, now);
    }

    fn go_to(&mut self, frame: &Frame, target: usize, now: Instant) {
        if target == self.current || target >= STOPS.len() {
            return;
        }
        let from = self.current;
        self.current = target;
        self.begin_transition(frame, from, now);
    }

    /// Esc: cierra siempre. Cualquier otra tecla no hace nada aquí (las flechas de
    /// navegación son clics en el globo, como en la maqueta).
    pub fn handle_key(&mut self, vk: u32) -> TourInput {
        if vk == 0x1B { TourInput::Closed } else { TourInput::None }
    }

    /// Clic en `(x, y)` (DIPs de la ventana principal). Dentro del globo (Siguiente,
    /// Atrás, un punto de progreso) navega; fuera del foco actual, cierra — clic
    /// *dentro* del propio foco no hace nada (es la zona que se está señalando, no
    /// hay que robarle el clic a la app de debajo mientras el recorrido está activo).
    pub fn handle_click(&mut self, x: f32, y: f32, frame: &Frame, now: Instant) -> TourInput {
        for &(r, h) in self.hits.iter().rev() {
            if r.contains(x, y) {
                match h {
                    Hit::Next => return self.next(frame, now),
                    Hit::Back => {
                        self.back(frame, now);
                        return TourInput::None;
                    }
                    Hit::Dot(i) => {
                        self.go_to(frame, i, now);
                        return TourInput::None;
                    }
                    Hit::None => return TourInput::None,
                }
            }
        }
        if self.current_rect(frame).contains(x, y) { TourInput::None } else { TourInput::Closed }
    }

    /// Dibuja la vela + foco + globo de la parada actual, interpolando desde
    /// `prev_rect` si hay una transición en curso. Registra las zonas clicables
    /// (Siguiente/Atrás/puntos) para el siguiente `handle_click`.
    pub fn draw(&mut self, r: &Renderer, frame: &Frame, dark: bool, now: Instant) {
        self.hits.clear();
        let pal = crate::theme::palette(dark);
        let (w, h) = r.size_dips();
        let full = Rect::new(0.0, 0.0, w, h);

        let target = self.current_rect(frame);
        let hole = match (self.prev_rect, self.anim) {
            (Some(prev), Some(a)) if !a.is_done(now) => Rect::new(
                a.value(now, prev.left - 4.0, target.left - 4.0),
                a.value(now, prev.top - 4.0, target.top - 4.0),
                a.value(now, prev.right + 4.0, target.right + 4.0),
                a.value(now, prev.bottom + 4.0, target.bottom + 4.0),
            ),
            _ => Rect::new(target.left - 4.0, target.top - 4.0, target.right + 4.0, target.bottom + 4.0),
        };

        r.begin_overlay();
        r.fill_veil_with_hole(full, hole, 6.0, crate::theme::Rgba(0.039, 0.039, 0.047, 0.62), pal.accent);
        self.draw_tooltip(r, pal, hole, full);
        r.end_paint();
    }

    fn draw_tooltip(&mut self, r: &Renderer, pal: &crate::theme::Palette, hole: Rect, full: Rect) {
        let stop = &STOPS[self.current];
        let kbd_h = if stop.kbd.is_empty() { 0.0 } else { 22.0 };
        let body_lines_h = 34.0;
        let tip_h = TIP_PAD * 2.0 + 15.0 + 3.0 + body_lines_h + kbd_h + 10.0 + 20.0;

        let mut tx = hole.left;
        let mut ty = hole.bottom + 10.0;
        if ty + tip_h > full.bottom {
            ty = hole.top - tip_h - 10.0;
        }
        tx = tx.clamp(10.0, (full.right - TIP_W - 10.0).max(10.0));
        let tip = Rect::new(tx, ty, tx + TIP_W, ty + tip_h);

        r.fill_round(tip, 8.0, pal.chrome_hi);
        r.stroke_round_rect(tip, 8.0, 1.0, pal.shadow_ring);

        let title_r = Rect::new(tip.left + TIP_PAD, tip.top + TIP_PAD, tip.right - TIP_PAD, tip.top + TIP_PAD + 16.0);
        r.text(stop.title, &r.fonts().ui_13, title_r, pal.text);
        let body_r = Rect::new(title_r.left, title_r.bottom + 3.0, title_r.right, title_r.bottom + 3.0 + body_lines_h);
        r.text(stop.body, &r.fonts().ui_11_5, body_r, pal.text_2);

        let mut kbd_y = body_r.bottom;
        if !stop.kbd.is_empty() {
            let mut kx = body_r.left;
            for k in stop.kbd {
                let kw = r.measure(k, &r.fonts().mono_11) + 10.0;
                let kr = Rect::new(kx, kbd_y, kx + kw, kbd_y + 18.0);
                r.fill_round(kr, 3.0, pal.hover);
                r.text(k, &r.fonts().mono_11, Rect::new(kr.left + 5.0, kr.top, kr.right - 5.0, kr.bottom), pal.text_2);
                kx += kw + 6.0;
            }
            kbd_y += kdb_row_h();
        }

        let nav_r = Rect::new(body_r.left, tip.bottom - TIP_PAD - 18.0, body_r.right, tip.bottom - TIP_PAD);
        let _ = kbd_y;

        // Puntos de progreso (izquierda).
        let dot_d = 5.0;
        let dot_gap = 4.0;
        let mut dx = nav_r.left;
        for i in 0..STOPS.len() {
            let on = i == self.current;
            let dw = if on { 14.0 } else { dot_d };
            let dr = Rect::new(dx, nav_r.top + (nav_r.height() - dot_d) / 2.0, dx + dw, nav_r.top + (nav_r.height() - dot_d) / 2.0 + dot_d);
            r.fill_round(dr, dot_d / 2.0, if on { pal.accent } else { pal.text_3 });
            self.hits.push((dr, Hit::Dot(i)));
            dx += dw + dot_gap;
        }

        // "Siguiente"/"Hecho" (derecha) + "← Atrás" si no es la primera parada.
        let is_last = self.current + 1 == STOPS.len();
        let next_label = if is_last { "Hecho" } else { "Siguiente →" };
        let next_w = r.measure(next_label, &r.fonts().ui_12);
        let next_r = Rect::new(nav_r.right - next_w, nav_r.top, nav_r.right, nav_r.bottom);
        r.text(next_label, &r.fonts().ui_12, next_r, pal.accent);
        self.hits.push((next_r, Hit::Next));

        if self.current > 0 {
            let back_label = "← Atrás";
            let back_w = r.measure(back_label, &r.fonts().ui_12);
            let back_r = Rect::new(next_r.left - 10.0 - back_w, nav_r.top, next_r.left - 10.0, nav_r.bottom);
            r.text(back_label, &r.fonts().ui_12, back_r, pal.text_2);
            self.hits.push((back_r, Hit::Back));
        }
    }
}

fn kdb_row_h() -> f32 {
    28.0
}
