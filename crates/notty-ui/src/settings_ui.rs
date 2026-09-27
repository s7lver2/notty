//! Controles de la ventana de Ajustes en modo inmediato: cada fotograma se dibujan
//! desde cero a partir de `Config`, registran su zona de clic y piden sus valores
//! animados a `Tweens` (hover, interruptores, pastillas que se deslizan...). Las
//! duraciones y curvas son las del CSS de la maqueta (`Prototipo.dc.html`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

use notty_input::Command;
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;
use windows_numerics::{Matrix3x2, Vector2};

use crate::Renderer;
use crate::anim::{Curve, Tweens};
use crate::layout::Rect;
use crate::settings_model::{LinkAction, Page, SettingKey};
use crate::text_input::TextInput;
use crate::theme::{Palette, Rgba};

/// Iconos de trazo (lienzo 24×24) copiados de los `<svg>` de la maqueta.
pub(crate) mod icon {
    pub const APARIENCIA: &str = "M12 3a9 9 0 1 0 0 18c1.1 0 1.8-.8 1.8-1.7 0-.5-.2-.9-.5-1.2-.3-.3-.5-.7-.5-1.2 0-1 .8-1.7 1.8-1.7H17a4 4 0 0 0 4-4c0-4.4-4-8.2-9-8.2zM6.5 10.5a1 1 0 1 0 2 0a1 1 0 1 0-2 0M9.5 7a1 1 0 1 0 2 0a1 1 0 1 0-2 0M14 7.5a1 1 0 1 0 2 0a1 1 0 1 0-2 0";
    pub const VENTANA: &str = "M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM3 9h18";
    pub const TECLADO: &str = "M4.5 6h15a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-15a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2zM6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10";
    pub const CARPETA: &str = "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z";
    pub const RAYO: &str = "M13 2 4 14h7l-1 8 9-12h-7z";
    pub const ACTUALIZAR: &str = "M21 12a9 9 0 1 1-2.6-6.4M21 4v5h-5";
    pub const INFO: &str = "M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0M12 11v5M12 8h.01";
    pub const AYUDA: &str = "M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .9-1 1.6v.4M12 17h.01";
    pub const ARCHIVO: &str = "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5";
    pub const CHEVRON: &str = "m9 6 6 6-6 6";
    pub const ATRAS: &str = "m15 6-6 6 6 6";
    pub const CHECK: &str = "m5 12 5 5 9-10";
    pub const CRUZ: &str = "M6 6l12 12M18 6 6 18";
    pub const FLECHA: &str = "M5 12h14M13 6l6 6-6 6";
    pub const EXTERNO: &str = "M14 4h6v6M20 4l-9 9M18 14v6H4V6h6";
    pub const DESCARGA: &str = "M12 4v11M7 10l5 5 5-5M5 20h14";
    pub const GIRO: &str = "M21 12a9 9 0 1 1-9-9";
    pub const ESCUDO: &str = "M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6zM9 12l2 2 4-4";
    pub const BORRADOR: &str = "M5 4h11l3 3v13H5zM8 4v5h7V4M8 20v-6h8v6";
    pub const GOTA: &str = "M12 3c3 4 6 7 6 11a6 6 0 0 1-12 0c0-4 3-7 6-11z";
    pub const CIRCULO: &str = "M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0";
    pub const CIRCULO_PEQ: &str = "M8.5 12a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0-7 0";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum InputId {
    #[default]
    Sample,
    Search,
    LigSeq,
    LigGlyph,
}

/// Zonas de clic de la ventana de Ajustes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum Hit {
    #[default]
    None,
    Caption,
    Close,
    /// Fondo de la barra lateral (sin botón debajo): solo para saber que el ratón
    /// sigue encima y no plegarla.
    Rail,
    Nav(Page),
    EditConfig,
    Back,
    Crumb,
    Toggle(SettingKey),
    /// Opción `n` de un selector (segmentado, tarjetas, selector grande).
    Choice(SettingKey, u8),
    Link(LinkAction),
    /// Fila que abre una subpágina.
    Go(Page),
    Binding(Command),
    Font(u16),
    Input(InputId),
    Slider,
    LigToggle(u16),
    LigDelete(u16),
    LigAdd,
    /// Ajustes → Sintaxis: interruptor del lenguaje `n` de `syntax::LANGS`, elegirlo
    /// para la vista previa, y activar/desactivar todos.
    LangToggle(u16),
    LangPick(u16),
    LangAll(bool),
    /// Zonas sin acción que solo reaccionan al ratón (atajos de Ayuda...).
    Static(u16),
    ScrollThumb,
    ScrollTrack,
    CapSave,
    CapCancel,
    CapReset,
    CapCard,
    CapBackdrop,
}

/// Claves de `Tweens`: qué valor animado es cada uno.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TK {
    Hover(Hit),
    Press(Hit),
    /// Estado encendido/elegido de un control (interruptor, tarjeta, ✓).
    On(Hit),
    /// Posición (índice fraccionario) de la pastilla de un selector.
    Pill(SettingKey),
    RailOpen,
    RailLabel(u8),
    RailInd,
    /// Morfado `->` → `→` de la ligadura con este hash de secuencia.
    Morph(u64),
    /// Valores sueltos de las vistas previas (canal de números, colores...).
    Preview(u8),
    Flash(Command),
    Bg(Hit),
    Badge(Hit),
    Nudge(Hit),
    Focus(InputId),
}

/// Formatos de texto creados a demanda: `(familia, tamaño×10, seminegrita)`.
#[derive(Default)]
pub(crate) struct FormatCache(RefCell<HashMap<(Option<String>, u32, bool), IDWriteTextFormat>>);

impl FormatCache {
    pub fn get(&self, r: &Renderer, family: Option<&str>, size: f32, semibold: bool) -> IDWriteTextFormat {
        let key = (family.map(str::to_string), (size * 10.0).round() as u32, semibold);
        if let Some(f) = self.0.borrow().get(&key) {
            return f.clone();
        }
        let f = r
            .create_format(family, size, semibold)
            .or_else(|| r.create_format(None, size, semibold))
            .unwrap_or_else(|| r.fonts().ui_12.clone());
        self.0.borrow_mut().insert(key, f.clone());
        f
    }

}

/// Estado de un fotograma de dibujo.
pub(crate) struct Ui<'a> {
    pub r: &'a Renderer,
    pub pal: &'a Palette,
    pub tw: &'a mut Tweens<TK>,
    pub hits: &'a mut Vec<(Rect, Hit)>,
    pub fmts: &'a FormatCache,
    pub hover: Hit,
    pub pressed: Hit,
    pub now: Instant,
    /// Las zonas de clic se recortan a esto (el panel visible, sin la barra lateral).
    pub clip: Rect,
    /// Idioma efectivo (`notty_ui::lang::resolve`, nunca `Auto`): ver `Ui::tr`.
    pub lang: notty_config::Lang,
    xf: Vec<Matrix3x2>,
    fades: Vec<f32>,
}

impl<'a> Ui<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        r: &'a Renderer,
        pal: &'a Palette,
        tw: &'a mut Tweens<TK>,
        hits: &'a mut Vec<(Rect, Hit)>,
        fmts: &'a FormatCache,
        hover: Hit,
        pressed: Hit,
        now: Instant,
        base: Matrix3x2,
        lang: notty_config::Lang,
    ) -> Self {
        let fade = r.fade();
        Ui { r, pal, tw, hits, fmts, hover, pressed, now, clip: Rect::new(-1e6, -1e6, 1e6, 1e6), lang, xf: vec![base], fades: vec![fade] }
    }

    pub fn anim(&self) -> bool {
        !self.tw.disabled
    }

    /// Traduce `es` al idioma efectivo de esta ventana de Ajustes (ver `strings::tr`).
    /// Los textos comunes (`header`, `group`, `label`, `title_desc`, `toggle_row`,
    /// `seg_row`...) ya pasan por aquí solos: no hace falta envolver cada llamada.
    pub fn tr<'s>(&self, es: &'s str) -> &'s str {
        crate::strings::tr(self.lang, es)
    }

    // --- Pilas de transformación y fundido ---------------------------------------

    /// `local` se aplica antes que lo que ya hubiera (coordenadas locales).
    pub fn push_xf(&mut self, local: Matrix3x2) {
        let m = local * *self.xf.last().expect("pila de transformaciones");
        self.xf.push(m);
        self.r.set_transform(m);
    }

    pub fn pop_xf(&mut self) {
        if self.xf.len() > 1 {
            self.xf.pop();
        }
        self.r.set_transform(*self.xf.last().expect("pila de transformaciones"));
    }

    pub fn push_fade(&mut self, f: f32) {
        let v = self.fades.last().copied().unwrap_or(1.0) * f.clamp(0.0, 1.0);
        self.fades.push(v);
        self.r.set_fade(v);
    }

    pub fn pop_fade(&mut self) {
        if self.fades.len() > 1 {
            self.fades.pop();
        }
        self.r.set_fade(self.fades.last().copied().unwrap_or(1.0));
    }

    /// Entrada escalonada (`@keyframes enter`): fundido y `dy` px que se recogen.
    /// Hay que cerrarla con `end_enter`.
    pub fn begin_enter(&mut self, start: Instant, delay_ms: u64, ms: u64, dx: f32, dy: f32) {
        let t = if self.anim() {
            let el = self.now.saturating_duration_since(start).as_secs_f32() * 1000.0 - delay_ms as f32;
            Curve::Out.apply((el / ms as f32).clamp(0.0, 1.0))
        } else {
            1.0
        };
        self.push_fade(t);
        self.push_xf(Matrix3x2::translation(dx * (1.0 - t), dy * (1.0 - t)));
    }

    pub fn end_enter(&mut self) {
        self.pop_xf();
        self.pop_fade();
    }

    // --- Valores animados --------------------------------------------------------

    pub fn tween(&mut self, key: TK, target: f32, ms: u64, curve: Curve) -> f32 {
        self.tw.to(key, target, ms, curve, self.now)
    }

    pub fn hover_t(&mut self, hit: Hit, ms: u64) -> f32 {
        let on = self.hover == hit && hit != Hit::None;
        self.tw.to(TK::Hover(hit), on as u8 as f32, ms, Curve::Out, self.now)
    }

    pub fn hover_spring(&mut self, hit: Hit, ms: u64) -> f32 {
        let on = self.hover == hit && hit != Hit::None;
        self.tw.to(TK::Hover(hit), on as u8 as f32, ms, Curve::Spring, self.now)
    }

    pub fn press_t(&mut self, hit: Hit, ms: u64) -> f32 {
        let on = self.pressed == hit && self.hover == hit && hit != Hit::None;
        self.tw.to(TK::Press(hit), on as u8 as f32, ms, Curve::Out, self.now)
    }

    pub fn on_t(&mut self, hit: Hit, on: bool, ms: u64, curve: Curve) -> f32 {
        self.tw.to(TK::On(hit), on as u8 as f32, ms, curve, self.now)
    }

    // --- Zonas de clic y texto -----------------------------------------------------

    pub fn hit(&mut self, r: Rect, hit: Hit) {
        let c = Rect::new(r.left.max(self.clip.left), r.top.max(self.clip.top), r.right.min(self.clip.right), r.bottom.min(self.clip.bottom));
        if !c.is_empty() {
            self.hits.push((c, hit));
        }
    }

    pub fn font(&self, size: f32, semibold: bool) -> IDWriteTextFormat {
        self.fmts.get(self.r, None, size, semibold)
    }

    pub fn mono(&self, family: &str, size: f32) -> IDWriteTextFormat {
        self.fmts.get(self.r, Some(family), size, false)
    }

    pub fn text(&self, s: &str, size: f32, semibold: bool, r: Rect, c: Rgba) {
        self.r.text(s, &self.font(size, semibold), r, c);
    }

    pub fn measure(&self, s: &str, size: f32, semibold: bool) -> f32 {
        self.r.measure(s, &self.font(size, semibold))
    }

    pub fn icon(&self, d: &str, cx: f32, cy: f32, size: f32, width: f32, c: Rgba) {
        self.r.stroke_svg(d, cx, cy, size, width, c, None);
    }

    // --- Colores de la maqueta en términos de la paleta ------------------------------

    /// `#232527`: fondo de las filas.
    pub fn row_c(&self, hover: f32) -> Rgba {
        self.pal.surface_2.mix(self.pal.text, 0.025 * hover)
    }

    /// `#3a3c3f`: pista apagada, pastilla del segmentado.
    pub fn track_c(&self) -> Rgba {
        self.pal.line.mix(self.pal.text_2, 0.09)
    }

    /// `#c9cbcd`: texto secundario claro (atajos, notas).
    pub fn text_soft(&self) -> Rgba {
        self.pal.text.mix(self.pal.text_2, 0.3)
    }

    // --- Controles ---------------------------------------------------------------

    /// `.row` / `.lnk`: fondo de fila con su hover.
    pub fn row_bg(&mut self, r: Rect, hit: Hit, radius: f32) -> f32 {
        let h = self.hover_t(hit, 200);
        self.r.fill_round(r, radius, self.row_c(h));
        h
    }

    /// `.tg`: 40×22, bolita que se estira al pulsar y rebota al cambiar.
    pub fn toggle(&mut self, rect: Rect, hit: Hit, on: bool) {
        let t = self.on_t(hit, on, 320, Curve::Spring);
        let bg_t = self.tw.to(TK::Bg(hit), on as u8 as f32, 250, Curve::Out, self.now);
        let press = self.press_t(hit, 150);
        let track = Rect::new(rect.left, rect.top + (rect.height() - 22.0) / 2.0, rect.left + 40.0, rect.top + (rect.height() + 22.0) / 2.0);
        self.r.fill_round(track, 11.0, self.track_c().mix(self.pal.accent, bg_t));
        let w = 14.0 + 5.0 * press;
        // Encendida y pulsada, la bolita crece hacia la izquierda (`translateX(13px)`).
        let x = track.left + 4.0 + 18.0 * t - 5.0 * press * bg_t;
        let knob = Rect::new(x, track.top + 4.0, x + w, track.top + 18.0);
        self.r.fill_round(knob, 7.0, self.pal.text_2.mix(self.pal.on_accent, bg_t));
        self.hit(track, hit);
    }

    /// `.seg`: opciones del mismo ancho y una pastilla que se desliza con rebote.
    pub fn seg(&mut self, rect: Rect, key: SettingKey, labels: &[&str], sel: Option<usize>) {
        let n = labels.len().max(1) as f32;
        self.r.fill_round(rect, 6.0, self.pal.surface);
        let inner = Rect::new(rect.left + 3.0, rect.top + 3.0, rect.right - 3.0, rect.bottom - 3.0);
        let cw = inner.width() / n;
        if let Some(s) = sel {
            let pos = self.tween(TK::Pill(key), s as f32, 380, Curve::Spring);
            let x = inner.left + cw * pos;
            self.r.fill_round(Rect::new(x, inner.top, x + cw, inner.bottom), 4.0, self.track_c());
        }
        for (j, label) in labels.iter().enumerate() {
            let hit = Hit::Choice(key, j as u8);
            let cell = Rect::new(inner.left + cw * j as f32, inner.top, inner.left + cw * (j + 1) as f32, inner.bottom);
            let h = self.hover_t(hit, 200);
            let c = if sel == Some(j) { self.pal.text } else { self.pal.text_2.mix(self.pal.text, h) };
            self.r.text_center(label, &self.font(12.0, false), cell, c);
            self.hit(cell, hit);
        }
    }

    /// Ancho natural de un `.seg` (para decidir si cabe al lado del título).
    pub fn seg_natural_w(&self, labels: &[&str]) -> f32 {
        let widest = labels.iter().map(|l| self.measure(l, 12.0, false)).fold(0.0f32, f32::max);
        (widest + 20.0) * labels.len() as f32 + 6.0
    }

    /// `.big`: selector grande de 3, pastilla con borde de acento.
    pub fn big_seg(&mut self, rect: Rect, key: SettingKey, labels: &[&str], sel: Option<usize>) {
        let n = labels.len().max(1) as f32;
        let gap = 8.0;
        let cw = (rect.width() - gap * (n - 1.0)) / n;
        for j in 0..labels.len() {
            let hit = Hit::Choice(key, j as u8);
            let x = rect.left + (cw + gap) * j as f32;
            let cell = Rect::new(x, rect.top, x + cw, rect.bottom);
            let on = self.on_t(hit, sel == Some(j), 200, Curve::Out);
            self.r.fill_round(cell, 8.0, self.pal.surface.faded(1.0 - on));
            self.hit(cell, hit);
        }
        if let Some(s) = sel {
            let pos = self.tween(TK::Pill(key), s as f32, 420, Curve::Spring);
            let x = rect.left + (cw + gap) * pos;
            let pill = Rect::new(x, rect.top, x + cw, rect.bottom);
            self.r.fill_round(pill, 8.0, self.pal.accent.faded(0.1));
            self.r.stroke_round_rect(Rect::new(pill.left + 0.75, pill.top + 0.75, pill.right - 0.75, pill.bottom - 0.75), 7.5, 1.5, self.pal.accent);
        }
        for (j, label) in labels.iter().enumerate() {
            let hit = Hit::Choice(key, j as u8);
            let x = rect.left + (cw + gap) * j as f32;
            let cell = Rect::new(x, rect.top, x + cw, rect.bottom);
            let h = self.hover_t(hit, 200);
            let c = if sel == Some(j) { self.pal.text } else { self.pal.text_2.mix(self.pal.text, h) };
            self.r.text_center(label, &self.font(12.0, false), cell, c);
        }
    }

    /// `.card`: sube 3 px al pasar el ratón, anillo y ✓ al elegirla. Lo de dentro se
    /// dibuja entre `card_begin` y `card_end` (ya desplazado).
    pub fn card_begin(&mut self, rect: Rect, hit: Hit, selected: bool) {
        let lift = self.hover_spring(hit, 250);
        let press = self.press_t(hit, 120);
        let sel = self.on_t(hit, selected, 250, Curve::Out);
        let c = Vector2 { X: rect.left + rect.width() / 2.0, Y: rect.top + rect.height() / 2.0 };
        let s = 1.0 - 0.02 * press;
        self.push_xf(Matrix3x2::scale_around(s, s, c) * Matrix3x2::translation(0.0, -3.0 * lift * (1.0 - press)));
        let bg = self.row_c(0.0).mix(self.pal.accent, 0.07 * sel);
        self.r.fill_round(rect, 8.0, bg);
        if sel > 0.01 {
            let ring = Rect::new(rect.left + 0.75, rect.top + 0.75, rect.right - 0.75, rect.bottom - 0.75);
            self.r.stroke_round_rect(ring, 7.25, 1.5, self.pal.accent.faded(sel));
        }
        self.hit(rect, hit);
    }

    pub fn card_end(&mut self, rect: Rect, hit: Hit, selected: bool) {
        let t = self.tw.to(TK::Badge(hit), selected as u8 as f32, 350, Curve::Spring, self.now);
        if t > 0.01 {
            let (cx, cy) = (rect.right + 3.0, rect.top + 3.0);
            let s = 0.3 + 0.7 * t;
            self.push_xf(Matrix3x2::scale_around(s, s, Vector2 { X: cx, Y: cy }));
            self.push_fade(t.min(1.0));
            self.r.fill_circle(cx, cy, 9.0, self.pal.accent);
            self.icon(icon::CHECK, cx, cy, 11.0, 3.0, self.pal.on_accent);
            self.pop_fade();
            self.pop_xf();
        }
        self.pop_xf();
    }

    /// `.ic`: caja de 34×34 con el icono que se agranda y ladea con el hover de la fila.
    /// Devuelve el centro; el icono se dibuja entre `ic_begin` y `pop_xf`.
    pub fn ic_begin(&mut self, rect: Rect, row_hover: f32, bg: Rgba) -> (f32, f32) {
        self.r.fill_round(rect, 6.0, bg);
        let c = Vector2 { X: rect.left + rect.width() / 2.0, Y: rect.top + rect.height() / 2.0 };
        let s = 1.0 + 0.08 * row_hover;
        self.push_xf(Matrix3x2::rotation_around(-3.0 * row_hover, c) * Matrix3x2::scale_around(s, s, c));
        (c.X, c.Y)
    }

    /// `.chev`: se adelanta 4 px con el hover de la fila.
    pub fn chevron(&mut self, cx: f32, cy: f32, row_hit: Hit, d: &str) {
        let t = self.tw.to(TK::Nudge(row_hit), (self.hover == row_hit) as u8 as f32, 250, Curve::Spring, self.now);
        let c = self.pal.text_3.mix(self.pal.text, t.clamp(0.0, 1.0));
        self.icon(d, cx + 4.0 * t, cy, 16.0, 1.8, c);
    }

    /// `.btn` (principal) o `.btn.ghost`. Se encoge al pulsar.
    pub fn button(&mut self, rect: Rect, label: &str, primary: bool, enabled: bool, hit: Hit) {
        let h = self.hover_t(hit, 150);
        let p = self.press_t(hit, 150);
        let c = Vector2 { X: rect.left + rect.width() / 2.0, Y: rect.top + rect.height() / 2.0 };
        let s = 1.0 - 0.05 * p;
        self.push_xf(Matrix3x2::scale_around(s, s, c));
        self.push_fade(if enabled { 1.0 } else { 0.4 });
        let (bg, fg) = if primary {
            (self.pal.accent.mix(self.pal.text, 0.15 * h), self.pal.on_accent)
        } else {
            (self.pal.chrome.mix(self.pal.line, h), self.pal.text)
        };
        self.r.fill_round(rect, 6.0, bg);
        self.r.text_center(label, &self.font(12.0, primary), rect, fg);
        self.pop_fade();
        self.pop_xf();
        if enabled {
            self.hit(rect, hit);
        }
    }

    pub fn button_w(&self, label: &str, primary: bool) -> f32 {
        self.measure(label, 12.0, primary) + 28.0
    }

    /// `.more`: enlace de acento con subrayado que crece desde la izquierda.
    pub fn more(&mut self, x: f32, cy: f32, label: &str, hit: Hit) -> Rect {
        let label = self.tr(label);
        let w = self.measure(label, 12.0, false);
        let r = Rect::new(x, cy - 9.0, x + w, cy + 9.0);
        let t = self.hover_t(hit, 250);
        self.text(label, 12.0, false, r, self.pal.accent);
        if t > 0.01 {
            self.r.fill(Rect::new(r.left, r.bottom - 1.5, r.left + w * t, r.bottom - 0.5), self.pal.accent);
        }
        self.hit(Rect::new(r.left - 2.0, r.top, r.right + 2.0, r.bottom), hit);
        r
    }

    /// `.kbd`: tecla con borde inferior de 2 px. `hot` = 0..1 hacia el acento.
    /// Devuelve su rectángulo.
    pub fn kbd(&mut self, x: f32, cy: f32, keys: &str, hot: f32, press: f32, size: f32) -> Rect {
        let f = self.mono(self.r.mono_family(), size);
        let w = self.r.measure(keys, &f) + size * 1.27;
        let h = size + 10.0;
        let dy = press * 1.5;
        let r = Rect::new(x, cy - h / 2.0 + dy, x + w, cy + h / 2.0 + dy);
        let border = self.pal.line.mix(self.pal.accent, hot);
        self.r.fill_round(r, 4.0, border);
        let bottom = 2.0 - press;
        self.r.fill_round(Rect::new(r.left + 1.0, r.top + 1.0, r.right - 1.0, r.bottom - bottom), 3.0, self.pal.surface);
        self.r.text_center(keys, &f, Rect::new(r.left, r.top, r.right, r.bottom - bottom + 1.0), self.text_soft().mix(self.pal.accent, hot));
        r
    }

    pub fn kbd_w(&self, keys: &str, size: f32) -> f32 {
        let f = self.mono(self.r.mono_family(), size);
        self.r.measure(keys, &f) + size * 1.27
    }

    /// `.inp`: caja de texto (fondo `cmd`, borde que se vuelve acento con halo al
    /// enfocarla) con cursor y selección.
    #[allow(clippy::too_many_arguments)]
    pub fn input(
        &mut self,
        rect: Rect,
        id: InputId,
        input: &TextInput,
        focused: bool,
        caret_on: bool,
        placeholder: &str,
        fmt: &IDWriteTextFormat,
        centered: bool,
    ) {
        let f = self.tw.to(TK::Focus(id), focused as u8 as f32, 200, Curve::Out, self.now);
        if f > 0.01 {
            let g = Rect::new(rect.left - 3.0, rect.top - 3.0, rect.right + 3.0, rect.bottom + 3.0);
            self.r.fill_round(g, 8.0, self.pal.accent.faded(0.15 * f));
        }
        self.r.fill_round(rect, 5.0, self.pal.line.mix(self.pal.accent, f));
        let inner = Rect::new(rect.left + 1.0, rect.top + 1.0, rect.right - 1.0, rect.bottom - 1.0);
        self.r.fill_round(inner, 4.0, self.pal.cmd);
        let pad = 10.0;
        let text_w = self.r.measure(&input.text, fmt);
        let avail = inner.width() - pad * 2.0;
        let x0 = if centered { inner.left + (inner.width() - text_w) / 2.0 } else { inner.left + pad };
        // Si no cabe, se desplaza para que el cursor quede a la vista.
        let caret_px = self.r.measure(input.prefix(input.caret), fmt);
        let shift = if !centered && caret_px > avail { caret_px - avail } else { 0.0 };
        self.r.push_clip(inner);
        if input.text.is_empty() {
            let pw = self.r.measure(placeholder, fmt);
            let px = if centered { inner.left + (inner.width() - pw) / 2.0 } else { x0 };
            self.r.text(placeholder, fmt, Rect::new(px, inner.top, inner.right + 200.0, inner.bottom), self.pal.text_3);
        } else {
            if let (true, Some((a, b))) = (focused, input.selection()) {
                let sa = x0 - shift + self.r.measure(input.prefix(a), fmt);
                let sb = x0 - shift + self.r.measure(input.prefix(b), fmt);
                self.r.fill(Rect::new(sa, inner.top + 5.0, sb, inner.bottom - 5.0), self.pal.accent.faded(0.3));
            }
            self.r.text(&input.text, fmt, Rect::new(x0 - shift, inner.top, x0 - shift + text_w + 200.0, inner.bottom), self.pal.text);
        }
        if focused && caret_on {
            let cx = x0 - shift + caret_px;
            self.r.fill(Rect::new(cx, inner.top + 6.0, cx + 1.2, inner.bottom - 6.0), self.pal.text);
        }
        self.r.pop_clip();
        self.hit(rect, Hit::Input(id));
    }

    /// Índice de carácter bajo `x` en un campo dibujado con `input` (para colocar el
    /// cursor al hacer clic).
    pub fn char_at(r: &Renderer, text: &TextInput, fmt: &IDWriteTextFormat, rel_x: f32) -> usize {
        let n = text.text.chars().count();
        let mut prev = 0.0f32;
        for i in 1..=n {
            let w = r.measure(text.prefix(i), fmt);
            if rel_x < (prev + w) / 2.0 {
                return i - 1;
            }
            prev = w;
        }
        n
    }
}

/// Hash estable (FNV-1a) de una cadena, para usarla como clave de animación.
pub(crate) fn key_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
