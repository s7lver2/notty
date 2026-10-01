//! Aviso «notty ahora se actualiza desde essentials» (`crate::essentials_install`): el
//! mismo contenedor y la misma entrada que Novedades, con «Instalar essentials», la
//! descarga, «Abriendo el instalador…» y el error. Maqueta:
//! `docs/mockups/essentials/aviso-instalar.html`.

use std::time::{Duration, Instant};

use windows_numerics::{Matrix3x2, Vector2};

use super::{Hit, Renderer, ViewState};
use crate::essentials_install::Phase;
use crate::layout::{self, Rect};
use crate::theme::Palette;

#[derive(Clone)]
pub struct EssentialsNoticeView {
    pub opened: Instant,
    pub phase: Phase,
    pub animate: bool,
}

const W: f32 = 420.0;
const PAD: f32 = 20.0;
const THUMB: f32 = 46.0;
const GAP: f32 = 14.0;

/// Partido como en la maqueta (`text-wrap: balance`).
const TITLE: &str = "notty ahora se actualiza\ndesde essentials";
const BODY: &str = "essentials es la tienda de tus apps: instala, actualiza y desinstala notty y las demás desde un sitio. Sin cuentas ni telemetría. Mientras no la instales, notty no se actualiza.";

/// Bolsa de la tienda (lienzo 24×24, como los iconos de Ajustes).
pub const BOLSA: &str = "M5 8h14l-1.1 11.2a2 2 0 0 1-2 1.8H8.1a2 2 0 0 1-2-1.8zM9 10.5V7a3 3 0 0 1 6 0v3.5";
const GIRO: &str = "M21 12a9 9 0 1 1-9-9";
const CRUZ: &str = "M6 6l12 12M18 6 6 18";

fn ease(t: f32) -> f32 {
    crate::ease_out_cubic(t.clamp(0.0, 1.0))
}

impl Renderer {
    pub fn set_essentials_notice(&mut self, v: Option<EssentialsNoticeView>) {
        self.essentials_notice = v;
    }

    pub(super) unsafe fn draw_essentials_notice(&mut self, pal: &Palette, body: Rect, view: &ViewState) {
        let Some(n) = self.essentials_notice.clone() else { return };
        let now = Instant::now();
        let (win_w, _) = self.size_dips();
        let w = W.min(win_w - 32.0);
        let title_w = w - PAD * 2.0 - THUMB - GAP;
        let title = self.tr(TITLE);
        let text = self.tr(BODY);
        let title_h = self.measure_wrapped(title, &self.fonts.ui_18_semibold, title_w, Some(24.0));
        let head_h = title_h.max(THUMB);
        let body_h = self.measure_wrapped(text, &self.fonts.ui_12, w - PAD * 2.0, Some(18.0));
        let h = PAD + head_h + 14.0 + body_h + 20.0 + 32.0 + PAD;

        let open = crate::Anim::new_maybe(n.opened, Duration::from_millis(220), n.animate);
        let k = ease(open.progress(now));
        let cx = win_w / 2.0;
        let top = (body.top + (body.height() - h) / 2.0).max(body.top + 8.0) + (1.0 - k) * 10.0;
        let r = Rect::new(cx - w / 2.0, top, cx + w / 2.0, top + h);

        self.set_fade(k);
        self.draw_popup_shadow(r, layout::POPUP_RADIUS, k, pal.shadow);
        self.fill_round(r, layout::POPUP_RADIUS, pal.chrome_hi);
        self.stroke_round_rect(r, layout::POPUP_RADIUS, 1.0, pal.shadow_ring);
        self.hits.push((r, Hit::PopupBox));

        // Filas escalonadas como las de Novedades: cabecera, texto y pie.
        let row = |i: u64| {
            let a = crate::Anim::new_maybe(n.opened + Duration::from_millis(90 + 70 * i), Duration::from_millis(300), n.animate);
            let t = ease(a.progress(now));
            (t, (1.0 - t) * 8.0)
        };

        let (t, dy) = row(0);
        self.set_fade(k * t);
        let y = r.top + PAD + dy;
        let thumb = Rect::new(r.left + PAD, y + (head_h - THUMB) / 2.0, r.left + PAD + THUMB, y + (head_h + THUMB) / 2.0);
        self.fill_round(thumb, 11.0, pal.surface);
        self.stroke_svg(BOLSA, thumb.left + THUMB / 2.0, thumb.top + THUMB / 2.0, 24.0, 1.8, pal.accent, None);
        let tx = thumb.right + GAP;
        let ty = y + (head_h - title_h) / 2.0;
        self.text_wrapped(title, &self.fonts.ui_18_semibold, Rect::new(tx, ty, tx + title_w, ty + title_h), pal.text, Some(24.0));

        let (t, dy) = row(1);
        self.set_fade(k * t);
        let y = r.top + PAD + head_h + 14.0 + dy;
        self.text_wrapped(text, &self.fonts.ui_12, Rect::new(r.left + PAD, y, r.right - PAD, y + body_h), pal.text_2, Some(18.0));

        let (t, dy) = row(2);
        self.set_fade(k * t);
        let foot = Rect::new(r.left + PAD, r.bottom - PAD - 32.0 + dy, r.right - PAD, r.bottom - PAD + dy);
        self.draw_notice_foot(pal, foot, n.phase, view, now);
        self.set_fade(1.0);
    }

    /// Botón de 32 px alineado a la derecha en `right`; devuelve su borde izquierdo.
    fn notice_button(&mut self, pal: &Palette, right: f32, top: f32, label: &str, primary: bool, hit: Hit, view: &ViewState) -> f32 {
        let fmt = if primary { &self.fonts.ui_12_5_semibold } else { &self.fonts.ui_12_5 };
        let pad = if primary { 16.0 } else { 14.0 };
        let lw = self.measure(label, fmt);
        let btn = Rect::new(right - lw - pad * 2.0, top, right, top + 32.0);
        let hovered = view.hover == hit;
        let (bg, fg) = if primary {
            (if hovered { pal.accent.mix(pal.text, 0.12) } else { pal.accent }, pal.on_accent)
        } else {
            (if hovered { pal.press } else { pal.hover }, pal.text)
        };
        self.fill_round(btn, 7.0, bg);
        let fmt = if primary { &self.fonts.ui_12_5_semibold } else { &self.fonts.ui_12_5 };
        self.text(label, fmt, Rect::new(btn.left + pad, btn.top, btn.left + pad + lw + 1.0, btn.bottom), fg);
        self.hits.push((btn, hit));
        btn.left
    }

    /// Etiqueta de 16 px y barra de 6 px, centradas en el alto del pie.
    fn notice_progress(&self, pal: &Palette, left: f32, right: f32, top: f32, label: &str, pct: f32, spin: Option<f32>) {
        let ly = top + 2.0;
        let mut lx = left;
        if let Some(ang) = spin {
            let c = Vector2 { X: left + 7.0, Y: ly + 8.0 };
            self.set_transform(Matrix3x2::rotation_around(ang, c));
            self.stroke_svg(GIRO, c.X, c.Y, 14.0, 2.0, pal.accent, None);
            self.set_transform(Matrix3x2::identity());
            lx += 14.0 + 7.0;
        }
        self.text(label, &self.fonts.ui_12, Rect::new(lx, ly, right, ly + 16.0), pal.text_2);
        let track = Rect::new(left, ly + 22.0, right, ly + 28.0);
        self.fill_round(track, 3.0, pal.press);
        let fill = Rect::new(track.left, track.top, track.left + track.width() * pct.clamp(0.0, 1.0), track.bottom);
        if fill.width() > 0.5 {
            self.fill_round(fill, 3.0, pal.accent);
        }
    }

    fn draw_notice_foot(&mut self, pal: &Palette, foot: Rect, phase: Phase, view: &ViewState, now: Instant) {
        let spin = |since: Instant| (now.saturating_duration_since(since).as_secs_f32() % 0.9) / 0.9 * 360.0;
        match phase {
            Phase::Idle => {
                let (install, later) = (self.tr("Instalar essentials").to_string(), self.tr("Ahora no").to_string());
                let x = self.notice_button(pal, foot.right, foot.top, &install, true, Hit::EssentialsInstall, view);
                self.notice_button(pal, x - 8.0, foot.top, &later, false, Hit::EssentialsLater, view);
            }
            Phase::Downloading(done, total) => {
                let cancel = self.tr("Cancelar").to_string();
                let x = self.notice_button(pal, foot.right, foot.top, &cancel, false, Hit::EssentialsCancel, view);
                let pct = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                let label = format!("{} · {} %", self.tr("Descargando essentials"), (pct * 100.0).round() as u32);
                self.notice_progress(pal, foot.left, x - 16.0, foot.top, &label, pct, None);
            }
            Phase::Opening(since) => {
                self.notice_progress(pal, foot.left, foot.right, foot.top, self.tr("Abriendo el instalador…"), 1.0, Some(spin(since)));
            }
            Phase::Error => {
                let later = self.tr("Ahora no").to_string();
                let x = self.notice_button(pal, foot.right, foot.top, &later, false, Hit::EssentialsLater, view);
                let cy = foot.top + 16.0;
                let dot_x = foot.left + 10.0;
                self.fill_circle(dot_x, cy, 10.0, pal.danger.faded(0.14));
                self.stroke_svg(CRUZ, dot_x, cy, 12.0, 1.8, pal.danger, None);
                let mut lx = foot.left + 20.0 + 8.0;
                let msg = self.tr("No se pudo descargar").to_string();
                let msg = msg.as_str();
                let mw = self.measure(msg, &self.fonts.ui_12);
                let line = |l: f32, r: f32| Rect::new(l, cy - 8.0, r, cy + 8.0);
                self.text(msg, &self.fonts.ui_12, line(lx, lx + mw + 1.0), pal.danger);
                lx += mw + 8.0;
                let dw = self.measure("·", &self.fonts.ui_12);
                self.text("·", &self.fonts.ui_12, line(lx, lx + dw + 1.0), pal.danger.faded(0.6));
                lx += dw + 8.0;
                let retry = self.tr("Reintentar").to_string();
                let retry = retry.as_str();
                let rw = self.measure(retry, &self.fonts.ui_12_semibold);
                let rr = line(lx, (lx + rw + 1.0).min(x - 8.0));
                self.text(retry, &self.fonts.ui_12_semibold, rr, pal.danger);
                if view.hover == Hit::EssentialsRetry {
                    self.fill(Rect::new(rr.left, rr.bottom - 1.0, rr.left + rw, rr.bottom), pal.danger);
                }
                self.hits.push((Rect::new(rr.left - 4.0, foot.top, rr.right + 4.0, foot.bottom), Hit::EssentialsRetry));
            }
        }
    }
}
