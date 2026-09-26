//! Piezas de dibujo que solo usa (por ahora) la ventana de Ajustes: iconos de trazo
//! a partir de un `d` de SVG, formatos de texto a demanda, la lista de fuentes
//! monoespaciadas instaladas y el icono de la app como mapa de bits.

use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_BEZIER_SEGMENT, D2D1_FIGURE_BEGIN_HOLLOW,
    D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
    D2D1_BITMAP_PROPERTIES, D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_CUSTOM, D2D1_DASH_STYLE_SOLID, D2D1_LINE_JOIN_ROUND,
    D2D1_STROKE_STYLE_PROPERTIES, D2D1_SWEEP_DIRECTION_CLOCKWISE, D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE, ID2D1Bitmap,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    IDWriteFont1, IDWriteFontCollection, IDWriteTextFormat,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::Interface;
use windows_numerics::Vector2;

use super::{Renderer, color, make_format};
use crate::layout::Rect;
use crate::svg_path::{self, Seg};
use crate::theme::Rgba;

impl Renderer {
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

    /// El icono de la app (recurso 1 del ejecutable) a `px` píxeles, como mapa de
    /// bits de Direct2D. `None` si no hay icono (p.ej. en los tests).
    pub fn app_icon_bitmap(&self, px: i32) -> Option<ID2D1Bitmap> {
        use windows::Win32::Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, ReleaseDC,
        };
        use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW};
        unsafe {
            let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).ok()?;
            let handle = LoadImageW(Some(instance.into()), windows::core::PCWSTR(1usize as *const u16), IMAGE_ICON, px, px, LR_DEFAULTCOLOR).ok()?;
            let icon = HICON(handle.0);
            let mut info = ICONINFO::default();
            let ok = GetIconInfo(icon, &mut info).is_ok();
            let mut pixels = vec![0u8; (px * px * 4) as usize];
            let mut got = false;
            if ok && !info.hbmColor.is_invalid() {
                let hdc = GetDC(None);
                let mut bmi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: px,
                        biHeight: -px,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                got = GetDIBits(hdc, info.hbmColor, 0, px as u32, Some(pixels.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS) > 0;
                ReleaseDC(None, hdc);
            }
            if !info.hbmColor.is_invalid() {
                let _ = DeleteObject(info.hbmColor.into());
            }
            if !info.hbmMask.is_invalid() {
                let _ = DeleteObject(info.hbmMask.into());
            }
            let _ = DestroyIcon(icon);
            if !got {
                return None;
            }
            // Direct2D quiere alfa premultiplicado.
            for p in pixels.chunks_exact_mut(4) {
                let a = p[3] as u32;
                p[0] = (p[0] as u32 * a / 255) as u8;
                p[1] = (p[1] as u32 * a / 255) as u8;
                p[2] = (p[2] as u32 * a / 255) as u8;
            }
            let props = D2D1_BITMAP_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
            };
            self.target
                .CreateBitmap(D2D_SIZE_U { width: px as u32, height: px as u32 }, Some(pixels.as_ptr() as *const _), (px * 4) as u32, &props)
                .ok()
        }
    }

    pub fn draw_bitmap(&self, bmp: &ID2D1Bitmap, r: Rect, opacity: f32) {
        unsafe {
            let dst = D2D_RECT_F { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
            self.target.DrawBitmap(bmp, Some(&dst), opacity * self.fade.get(), D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, None);
        }
    }
}
