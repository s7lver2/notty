//! Popup de Novedades (`crate::whats_new`): lista de lo nuevo de la versión, cada
//! novedad con una miniatura animada en bucle a la izquierda.

use std::time::{Duration, Instant};

use super::{Hit, Renderer, ViewState};
use crate::layout::{self, Rect};
use crate::theme::{Palette, Rgba};
use crate::whats_new::{Icon, Item};

/// Lo que el renderizador necesita para dibujar el popup.
#[derive(Clone)]
pub struct WhatsNewView {
    pub version: &'static str,
    pub items: &'static [Item],
    pub opened: Instant,
    /// `false` con las animaciones del sistema apagadas: todo sale ya en su sitio y
    /// las miniaturas quietas.
    pub animate: bool,
}

const W: f32 = 420.0;
const PAD: f32 = 20.0;
const THUMB: f32 = 46.0;
const GAP: f32 = 14.0;
const ROW_GAP: f32 = 16.0;

fn ease(t: f32) -> f32 {
    crate::ease_out_cubic(t.clamp(0.0, 1.0))
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Renderer {
    pub fn set_whats_new(&mut self, v: Option<WhatsNewView>) {
        self.whats_new = v;
    }

    /// Si el popup está abierto (las miniaturas piden frames continuamente).
    pub fn whats_new_open(&self) -> bool {
        self.whats_new.is_some()
    }

    pub(super) unsafe fn draw_whats_new(&mut self, pal: &Palette, body: Rect, view: &ViewState) {
        let Some(wn) = self.whats_new.clone() else { return };
        let now = Instant::now();
        let (win_w, _) = self.size_dips();
        let w = W.min(win_w - 32.0);
        let text_w = w - PAD * 2.0 - THUMB - GAP;

        // Alto: cabecera + filas (según lo que ocupe cada descripción) + botón.
        let rows_h: Vec<f32> = wn
            .items
            .iter()
            .map(|it| {
                let desc_h = self.measure_wrapped(self.tr(it.desc), &self.fonts.ui_11_5, text_w, Some(16.0));
                (20.0 + 2.0 + desc_h).max(THUMB)
            })
            .collect();
        let head_h = 30.0 + 18.0 + 18.0;
        let h = PAD + head_h + rows_h.iter().sum::<f32>() + ROW_GAP * (rows_h.len().saturating_sub(1)) as f32 + 22.0 + 32.0 + PAD;

        // Entrada: se funde y sube unos px.
        let open = crate::Anim::new_maybe(wn.opened, Duration::from_millis(220), wn.animate);
        let k = ease(open.progress(now));
        let cx = win_w / 2.0;
        let top = (body.top + (body.height() - h) / 2.0).max(body.top + 8.0) + (1.0 - k) * 10.0;
        let r = Rect::new(cx - w / 2.0, top, cx + w / 2.0, top + h);

        self.set_fade(k);
        self.draw_popup_shadow(r, layout::POPUP_RADIUS, k, pal.shadow);
        self.fill_round(r, layout::POPUP_RADIUS, pal.chrome_hi);
        self.stroke_round_rect(r, layout::POPUP_RADIUS, 1.0, pal.shadow_ring);
        self.hits.push((r, Hit::PopupBox));

        // Cabecera.
        let mut y = r.top + PAD;
        self.text(self.tr("Novedades"), &self.fonts.ui_18_semibold, Rect::new(r.left + PAD, y, r.right - PAD - 30.0, y + 26.0), pal.text);
        y += 30.0;
        let sub = format!("notty {}", wn.version);
        self.text(&sub, &self.fonts.ui_12, Rect::new(r.left + PAD, y, r.right - PAD, y + 18.0), pal.text_3);
        y += 18.0 + 18.0;

        let close = Rect::new(r.right - PAD - 22.0, r.top + PAD - 2.0, r.right - PAD + 2.0, r.top + PAD + 22.0);
        if view.hover == Hit::WhatsNewClose {
            self.fill_round(close, 5.0, pal.hover);
        }
        let (mx, my) = ((close.left + close.right) / 2.0, (close.top + close.bottom) / 2.0);
        self.stroke_line(mx - 4.5, my - 4.5, mx + 4.5, my + 4.5, 1.4, pal.text_2);
        self.stroke_line(mx + 4.5, my - 4.5, mx - 4.5, my + 4.5, 1.4, pal.text_2);
        self.hits.push((close, Hit::WhatsNewClose));

        // Filas, escalonadas 70 ms.
        for (i, (it, rh)) in wn.items.iter().zip(&rows_h).enumerate() {
            let a = crate::Anim::new_maybe(wn.opened + Duration::from_millis(90 + 70 * i as u64), Duration::from_millis(300), wn.animate);
            let t = ease(a.progress(now));
            let dy = (1.0 - t) * 8.0;
            self.set_fade(k * t);
            let thumb = Rect::new(r.left + PAD, y + dy, r.left + PAD + THUMB, y + dy + THUMB);
            self.fill_round(thumb, 11.0, pal.surface);
            let phase = if wn.animate { now.saturating_duration_since(wn.opened).as_secs_f32() } else { 0.0 };
            self.draw_thumb(it.icon, thumb, phase, pal);
            let tx = thumb.right + GAP;
            self.text(self.tr(it.title), &self.fonts.ui_12_5_semibold, Rect::new(tx, y + dy, tx + text_w, y + dy + 20.0), pal.text);
            self.text_wrapped(self.tr(it.desc), &self.fonts.ui_11_5, Rect::new(tx, y + dy + 22.0, tx + text_w, y + dy + rh), pal.text_2, Some(16.0));
            y += rh + ROW_GAP;
        }
        self.set_fade(k);

        // Botón.
        let label = self.tr("Entendido");
        let bw = self.measure(label, &self.fonts.ui_12_5_semibold) + 32.0;
        let btn = Rect::new(r.right - PAD - bw, r.bottom - PAD - 32.0, r.right - PAD, r.bottom - PAD);
        let hovered = view.hover == Hit::WhatsNewOk;
        self.fill_round(btn, 7.0, if hovered { pal.accent.mix(pal.text, 0.12) } else { pal.accent });
        let lw = bw - 32.0;
        self.text(label, &self.fonts.ui_12_5_semibold, Rect::new(btn.left + 16.0, btn.top, btn.left + 16.0 + lw, btn.bottom), pal.on_accent);
        self.hits.push((btn, Hit::WhatsNewOk));
        self.set_fade(1.0);
    }

    /// Miniatura animada de `icon` en `r`; `t` son los segundos desde que se abrió.
    fn draw_thumb(&self, icon: Icon, r: Rect, t: f32, pal: &Palette) {
        let (cx, cy) = ((r.left + r.right) / 2.0, (r.top + r.bottom) / 2.0);
        match icon {
            Icon::Markdown => {
                // Tres líneas de markdown crudo ("#", "**", "-") que se van convirtiendo
                // en texto con formato, una detrás de otra, y vuelven a empezar.
                let period = 3.2;
                let p = (t % period) / period;
                let rows: [(f32, f32, Rgba); 3] = [(4.5, 22.0, pal.text), (3.0, 26.0, pal.text_2), (3.0, 18.0, pal.text_2)];
                for (i, (th, len, c)) in rows.iter().enumerate() {
                    // 0 → crudo, 1 → renderizado; se deshace al final del ciclo.
                    let start = 0.08 + i as f32 * 0.14;
                    let m = ease((p - start) / 0.18) * (1.0 - ease((p - 0.86) / 0.12));
                    let y = r.top + 12.0 + i as f32 * 10.5;
                    let x0 = r.left + 8.0;
                    // La marca: se encoge y se desvanece.
                    let mark_w = lerp(5.0, 0.0, m);
                    if mark_w > 0.3 {
                        self.fill_round(Rect::new(x0, y - 1.5, x0 + mark_w, y + 1.5), 1.0, pal.text_3.faded(1.0 - m));
                    }
                    let bx = x0 + lerp(8.0, 0.0, m);
                    let h = lerp(2.4, *th, m);
                    let col = if i == 0 { pal.text_3.mix(pal.accent, m) } else { pal.text_3.mix(*c, m) };
                    self.fill_round(Rect::new(bx, y - h / 2.0, bx + lerp(len * 0.8, *len, m), y + h / 2.0), h / 2.0, col);
                }
            }
            Icon::Speed => {
                // Rayas de velocidad pasando de derecha a izquierda, a distinto ritmo.
                self.push_clip(Rect::new(r.left + 3.0, r.top + 3.0, r.right - 3.0, r.bottom - 3.0));
                let lanes: [(f32, f32, f32); 3] = [(-7.0, 1.4, 18.0), (0.0, 1.0, 26.0), (7.0, 1.8, 14.0)];
                for (i, (dy, speed, len)) in lanes.iter().enumerate() {
                    let span = r.width() + len;
                    let off = ((t * speed * 60.0 + i as f32 * 17.0) % span + span) % span;
                    let x1 = r.right - off;
                    let c = if i == 1 { pal.accent } else { pal.text_3 };
                    self.fill_round(Rect::new(x1, cy + dy - 1.2, x1 + len, cy + dy + 1.2), 1.2, c);
                }
                self.pop_clip();
            }
            Icon::Animations => {
                // Una onda y un punto que la recorre; los puntos de muestra marcan los "frames".
                let amp = 8.0;
                let x0 = r.left + 7.0;
                let x1 = r.right - 7.0;
                let wave = |x: f32| cy + (((x - x0) / (x1 - x0)) * std::f32::consts::TAU).sin() * amp;
                let steps = 16;
                for s in 0..steps {
                    let (a, b) = (lerp(x0, x1, s as f32 / steps as f32), lerp(x0, x1, (s + 1) as f32 / steps as f32));
                    self.stroke_line(a, wave(a), b, wave(b), 1.3, pal.text_3);
                }
                for s in 0..=6 {
                    let x = lerp(x0, x1, s as f32 / 6.0);
                    self.fill_circle(x, wave(x), 1.3, pal.text_3);
                }
                let p = (t / 1.8) % 1.0;
                let x = lerp(x0, x1, p);
                self.fill_circle(x, wave(x), 3.2, pal.accent);
            }
            Icon::Sparkle => {
                let s = 1.0 + ((t * 3.0).sin() * 0.15);
                let l = 9.0 * s;
                self.stroke_line(cx - l, cy, cx + l, cy, 1.6, pal.accent);
                self.stroke_line(cx, cy - l, cx, cy + l, 1.6, pal.accent);
            }
        }
    }
}
