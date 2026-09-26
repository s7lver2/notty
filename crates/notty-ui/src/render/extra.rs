//! Piezas de dibujo que solo usa (por ahora) la ventana de Ajustes: iconos de trazo
//! a partir de un `d` de SVG, formatos de texto a demanda, la lista de fuentes
//! monoespaciadas instaladas y el logo de notty.

use windows::Win32::Graphics::Direct2D::Common::{
    D2D_SIZE_F, D2D1_BEZIER_SEGMENT, D2D1_FIGURE_BEGIN_HOLLOW,
    D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL, D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_CUSTOM, D2D1_DASH_STYLE_SOLID, D2D1_LINE_JOIN_ROUND,
    D2D1_STROKE_STYLE_PROPERTIES, D2D1_SWEEP_DIRECTION_CLOCKWISE, D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    IDWriteFont1, IDWriteFontCollection, IDWriteTextFormat,
};
use windows::core::Interface;
use windows_numerics::Vector2;

use super::{Renderer, color, make_format};
use crate::layout::Rect;
use crate::svg_path::{self, Seg};
use crate::theme::Rgba;

/// El logo de notty ("n_"), en el mismo lienzo de 24×24 que los iconos. Misma
/// geometría que `tools/make-icon.ps1` (assets/notty.ico).
pub const LOGO_N: &str = "M5.6 8.4V17M5.6 12.3a3.9 3.9 0 0 1 7.8 0V17";
pub const LOGO_CARET: &str = "M16.4 17h2.6";
const LOGO_STROKE: f32 = 2.4;

impl Renderer {
    /// Logo sobre su cuadrado redondeado. `caret` es la opacidad del "_" (para que
    /// parpadee en Acerca de; 1.0 fijo en las barras de título).
    pub fn draw_logo(&self, r: Rect, bg: Rgba, fg: Rgba, caret: f32) {
        let s = r.width();
        let (cx, cy) = (r.left + s / 2.0, r.top + s / 2.0);
        self.fill_round(r, s * 0.22, bg);
        self.stroke_svg(LOGO_N, cx, cy, s, LOGO_STROKE, fg, None);
        if caret > 0.0 {
            self.stroke_svg(LOGO_CARET, cx, cy, s, LOGO_STROKE, fg.faded(caret), None);
        }
    }

    /// Traza el icono `d` (lienzo SVG de 24×24) centrado en `(cx, cy)` y escalado a
    /// `size` DIPs, con extremos/uniones redondeados como `stroke-linecap:round`.
    /// `dash` = `stroke-dasharray` en unidades del lienzo.
    pub fn stroke_svg(&self, d: &str, cx: f32, cy: f32, size: f32, width: f32, c: Rgba, dash: Option<(f32, f32)>) {
        let k = size / 24.0;
        let map = |x: f32, y: f32| Vector2 { X: cx + (x - 12.0) * k, Y: cy + (y - 12.0) * k };
        let segs = svg_path::parse(d);
        unsafe {
            let Ok(path) = self._d2d.CreatePathGeometry() else { return };
            let Ok(sink) = path.Open() else { return };
            let mut open = false;
            for seg in &segs {
                match *seg {
                    Seg::Move(x, y) => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        }
                        sink.BeginFigure(map(x, y), D2D1_FIGURE_BEGIN_HOLLOW);
                        open = true;
                    }
                    Seg::Line(x, y) if open => sink.AddLine(map(x, y)),
                    Seg::Cubic { c1, c2, to } if open => sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                        point1: map(c1.0, c1.1),
                        point2: map(c2.0, c2.1),
                        point3: map(to.0, to.1),
                    }),
                    Seg::Arc { to, rx, ry, rot, large, sweep } if open => sink.AddArc(&D2D1_ARC_SEGMENT {
                        point: map(to.0, to.1),
                        size: D2D_SIZE_F { width: rx * k, height: ry * k },
                        rotationAngle: rot,
                        sweepDirection: if sweep { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE },
                        arcSize: if large { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
                    }),
                    Seg::Close if open => {
                        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                        open = false;
                    }
                    _ => {}
                }
            }
            if open {
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
            }
            if sink.Close().is_err() {
                return;
            }
            let w = width * k;
            let dashes = dash.map(|(on, off)| [on * k / w.max(0.01), off * k / w.max(0.01)]);
            let props = D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 10.0,
                dashStyle: if dashes.is_some() { D2D1_DASH_STYLE_CUSTOM } else { D2D1_DASH_STYLE_SOLID },
                dashOffset: 0.0,
            };
            let style = self._d2d.CreateStrokeStyle(&props, dashes.as_ref().map(|d| &d[..])).ok();
            self.brush.SetColor(&color(c.faded(self.fade.get())));
            self.target.DrawGeometry(&path, &self.brush, w, style.as_ref());
        }
    }

    /// Formato de texto nuevo de `family` (o la de la interfaz si es `None`), para
    /// tamaños que no están entre los fijos de `Fonts`. Quien lo pida lo cachea.
    pub fn create_format(&self, family: Option<&str>, size: f32, semibold: bool) -> Option<IDWriteTextFormat> {
        let weight = if semibold { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL };
        make_format(&self.dwrite, family.unwrap_or(&self.ui_family), size, weight).ok()
    }

    /// Familia monoespaciada vigente en el editor (tras el *fallback*).
    pub fn mono_family(&self) -> &str {
        &self.mono_family
    }

    /// Familias monoespaciadas instaladas, ordenadas por nombre. Deja fuera las de
    /// símbolos y las verticales ("@MS Gothic").
    pub fn monospace_families(&self) -> Vec<String> {
        let mut out = Vec::new();
        unsafe {
            let mut collection: Option<IDWriteFontCollection> = None;
            if self.dwrite.GetSystemFontCollection(&mut collection, false).is_err() {
                return out;
            }
            let Some(collection) = collection else { return out };
            for i in 0..collection.GetFontFamilyCount() {
                let Ok(family) = collection.GetFontFamily(i) else { continue };
                let Ok(font) = family.GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL)
                else {
                    continue;
                };
                if font.IsSymbolFont().as_bool() {
                    continue;
                }
                let Ok(font1) = font.cast::<IDWriteFont1>() else { continue };
                if !font1.IsMonospacedFont().as_bool() {
                    continue;
                }
                let Ok(names) = family.GetFamilyNames() else { continue };
                let mut idx = 0u32;
                let mut exists = windows::core::BOOL(0);
                let _ = names.FindLocaleName(windows::core::w!("en-us"), &mut idx, &mut exists);
                if !exists.as_bool() {
                    idx = 0;
                }
                let Ok(len) = names.GetStringLength(idx) else { continue };
                let mut buf = vec![0u16; len as usize + 1];
                if names.GetString(idx, &mut buf).is_err() {
                    continue;
                }
                let name = String::from_utf16_lossy(&buf[..len as usize]);
                if !name.starts_with('@') && !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        out.sort_by_key(|n| n.to_lowercase());
        out
    }
}
