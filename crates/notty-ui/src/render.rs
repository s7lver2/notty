use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_RECT_F, D2D_SIZE_U};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BRUSH_PROPERTIES, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_PROPERTIES, D2D1CreateFactory,
    ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_TRAILING, DWriteCreateFactory, IDWriteFactory,
    IDWriteTextFormat,
};
use windows::core::Result;
use windows_numerics::{Matrix3x2, Vector2};

use notty_config::{Files, MenuBar, UiConfig};

use crate::{EditorState, Workspace};

const FONT_SIZE: f32 = 16.0;
const BG: D2D1_COLOR_F = D2D1_COLOR_F { r: 0.09, g: 0.09, b: 0.10, a: 1.0 };
const FG: D2D1_COLOR_F = D2D1_COLOR_F { r: 0.92, g: 0.92, b: 0.93, a: 1.0 };
const SEL: D2D1_COLOR_F = D2D1_COLOR_F { r: 0.30, g: 0.45, b: 0.85, a: 0.35 };
const PADDING_X: f32 = 8.0;
const PADDING_TOP: f32 = 8.0;
const CARET_WIDTH: f32 = 2.0;
const TAB_WIDTH: f32 = 160.0;

/// Convierte un desplazamiento en chars (relativo al inicio de `text`) a un
/// desplazamiento en unidades UTF-16, que es lo que espera `IDWriteTextLayout`.
fn char_offset_to_utf16(text: &str, char_offset: usize) -> u32 {
    text.chars().take(char_offset).map(|c| c.len_utf16()).sum::<usize>() as u32
}

pub struct Renderer {
    _d2d: ID2D1Factory,
    target: ID2D1HwndRenderTarget,
    _dwrite: IDWriteFactory,
    text_format: IDWriteTextFormat,
    fg_brush: ID2D1SolidColorBrush,
    caret_brush: ID2D1SolidColorBrush,
    sel_brush: ID2D1SolidColorBrush,
    line_height: f32,
    tab_rects: Vec<(f32, f32, f32, f32)>,
}

impl Renderer {
    pub fn new(hwnd: HWND) -> Result<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let target = d2d.CreateHwndRenderTarget(
                &D2D1_RENDER_TARGET_PROPERTIES::default(),
                &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                    hwnd,
                    pixelSize: D2D_SIZE_U { width: 900, height: 600 },
                    ..Default::default()
                },
            )?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let text_format = dwrite.CreateTextFormat(
                windows::core::w!("Cascadia Mono"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                FONT_SIZE,
                windows::core::w!(""),
            )?;
            let fg_brush = target.CreateSolidColorBrush(
                &FG,
                Some(&D2D1_BRUSH_PROPERTIES { opacity: 1.0, transform: Matrix3x2::identity() }),
            )?;
            let caret_brush = target.CreateSolidColorBrush(
                &FG,
                Some(&D2D1_BRUSH_PROPERTIES { opacity: 1.0, transform: Matrix3x2::identity() }),
            )?;
            let sel_brush = target.CreateSolidColorBrush(
                &SEL,
                Some(&D2D1_BRUSH_PROPERTIES { opacity: 1.0, transform: Matrix3x2::identity() }),
            )?;
            let line_height = FONT_SIZE * 1.35;
            Ok(Self {
                _d2d: d2d,
                target,
                _dwrite: dwrite,
                text_format,
                fg_brush,
                caret_brush,
                sel_brush,
                line_height,
                tab_rects: Vec::new(),
            })
        }
    }

    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    /// Ancho aproximado de un dígito con la fuente actual, usado para el hueco de
    /// los números de línea. Mide "0" con `IDWriteTextLayout`; si falla, aproxima.
    fn digit_width(&self) -> f32 {
        let wide: Vec<u16> = "0".encode_utf16().collect();
        unsafe {
            if let Ok(layout) = self._dwrite.CreateTextLayout(&wide, &self.text_format, f32::MAX, self.line_height)
            {
                let mut metrics = Default::default();
                if layout.GetMetrics(&mut metrics).is_ok() && metrics.width > 0.0 {
                    return metrics.width;
                }
            }
        }
        FONT_SIZE * 0.6
    }

    /// Rectángulos de las pestañas dibujadas la última vez, para hit-testing de clic.
    pub fn tab_rects(&self) -> &[(f32, f32, f32, f32)] {
        &self.tab_rects
    }

    fn draw_text_line(&self, text: &str, x: f32, y: f32, max_width: f32) {
        let wide: Vec<u16> = text.encode_utf16().collect();
        if wide.is_empty() {
            return;
        }
        unsafe {
            if let Ok(layout) =
                self._dwrite.CreateTextLayout(&wide, &self.text_format, max_width, self.line_height)
            {
                self.target.DrawTextLayout(
                    Vector2 { X: x, Y: y },
                    &layout,
                    &self.fg_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                );
            }
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        unsafe {
            let _ = self.target.Resize(&D2D_SIZE_U { width, height });
        }
    }

    /// Traduce un punto del cliente (en píxeles, origen arriba-izquierda) al índice de
    /// char del documento más cercano, usando `state.viewport` para saber qué línea es cada fila.
    pub fn char_index_at(&self, state: &EditorState, x: f32, y: f32) -> usize {
        let buf = state.doc.buffer();
        let total = buf.len_lines();
        let range = state.viewport.range(total);
        let row = ((y - PADDING_TOP) / self.line_height).floor().max(0.0) as usize;
        let line = (state.viewport.first_line + row).min(total.saturating_sub(1)).max(range.start);

        let start = buf.line_start(line);
        let end = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
        let text: String = buf.slice(start..end).trim_end_matches(['\r', '\n']).to_string();
        if text.is_empty() {
            return start;
        }
        let wide: Vec<u16> = text.encode_utf16().collect();
        let Ok(layout) =
            (unsafe { self._dwrite.CreateTextLayout(&wide, &self.text_format, f32::MAX, self.line_height) })
        else {
            return start;
        };
        let mut trailing = windows::core::BOOL(0);
        let mut inside = windows::core::BOOL(0);
        let mut metrics = Default::default();
        if unsafe { layout.HitTestPoint(x - PADDING_X, 0.0, &mut trailing, &mut inside, &mut metrics) }.is_ok() {
            let utf16_offset = metrics.textPosition as usize + if trailing.as_bool() { 1 } else { 0 };
            let prefix = String::from_utf16_lossy(&wide[..utf16_offset.min(wide.len())]);
            start + prefix.chars().count()
        } else {
            start
        }
    }

    /// Dibuja fondo + las líneas visibles del documento activo de `ws`, con selección
    /// y caret, además de las franjas de pestañas/menú/atajos/gutter según `ui`.
    pub fn paint(&mut self, ws: &Workspace, ui: &UiConfig) {
        self.tab_rects.clear();
        let state = ws.active();
        let buf = state.doc.buffer();
        let total_lines = buf.len_lines();
        let range = state.viewport.range(total_lines);
        let sel = state.doc.selection();
        let sel_range = sel.range();
        let head = sel.head;

        let show_tabs = ui.files == Files::Tabs
            && ui.tabs_position != notty_config::TabsPosition::Hidden
            && (ui.tabs_position != notty_config::TabsPosition::Auto || ws.len() > 1);
        let show_menubar = ui.menubar == MenuBar::Visible;
        let show_hints = ui.hints_bar;

        unsafe {
            self.target.BeginDraw();
            self.target.Clear(Some(&BG));

            let mut top = 0.0f32;

            if show_menubar {
                self.draw_text_line("Archivo   Editar   Buscar   Ver   Ayuda", PADDING_X, top, f32::MAX);
                top += self.line_height;
            }

            if show_tabs {
                let mut x = 0.0f32;
                for (i, doc) in ws.iter().enumerate() {
                    let name = match &doc.path {
                        Some(p) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                        None => "sin título".to_string(),
                    };
                    let dirty = if doc.doc.is_dirty() { " •" } else { "" };
                    let rect =
                        D2D_RECT_F { left: x, top, right: x + TAB_WIDTH, bottom: top + self.line_height };
                    if i == ws.active_index() {
                        self.target.FillRectangle(&rect, &self.sel_brush);
                    }
                    self.draw_text_line(&format!("{name}{dirty}"), x + PADDING_X, top, TAB_WIDTH - PADDING_X);
                    self.tab_rects.push((rect.left, rect.top, rect.right, rect.bottom));
                    x += TAB_WIDTH;
                }
                top += self.line_height;
            }

            let gutter_w = if ui.line_numbers {
                crate::gutter_width(total_lines, self.digit_width())
            } else {
                0.0
            };

            let size = self.target.GetSize();
            let hints_h = if show_hints { self.line_height } else { 0.0 };
            let status_h = self.line_height;
            let bottom_reserved = hints_h + status_h;

            let text_pad = PADDING_X + gutter_w;
            let mut y = top + PADDING_TOP;
            for line in range {
                let start = buf.line_start(line);
                let full_end = if line + 1 < total_lines { buf.line_start(line + 1) } else { buf.len_chars() };
                let text: String = buf.slice(start..full_end).trim_end_matches(['\r', '\n']).to_string();
                let text_end = start + text.chars().count();
                let wide: Vec<u16> = text.encode_utf16().collect();

                let layout = if wide.is_empty() {
                    None
                } else {
                    self._dwrite.CreateTextLayout(&wide, &self.text_format, f32::MAX, self.line_height).ok()
                };

                if ui.line_numbers {
                    let num = (line + 1).to_string();
                    let num_wide: Vec<u16> = num.encode_utf16().collect();
                    if let Ok(num_layout) =
                        self._dwrite.CreateTextLayout(&num_wide, &self.text_format, (gutter_w - 12.0).max(0.0), self.line_height)
                    {
                        let _ = num_layout.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING);
                        self.target.DrawTextLayout(
                            Vector2 { X: PADDING_X, Y: y },
                            &num_layout,
                            &self.fg_brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                        );
                    }
                }

                // Fondo de selección para el tramo de esta línea dentro de `sel_range`.
                if !sel.is_empty() && sel_range.start < full_end && sel_range.end > start {
                    let clamp_start = sel_range.start.max(start);
                    let clamp_end = sel_range.end.min(text_end);
                    let x0 = if let (true, Some(layout)) = (clamp_end > clamp_start, layout.as_ref()) {
                        hit_test_x(layout, &text, clamp_start - start, text_pad)
                    } else {
                        text_pad
                    };
                    let mut x1 = if let (true, Some(layout)) = (clamp_end > clamp_start, layout.as_ref()) {
                        hit_test_x(layout, &text, clamp_end - start, text_pad)
                    } else {
                        text_pad
                    };
                    // La selección sigue más allá del texto visible (incluye el salto de línea):
                    // ensancha un poco el rectángulo para que se note.
                    if sel_range.end > text_end {
                        x1 = (x1).max(x0) + 6.0;
                    }
                    x1 = x1.max(x0 + 2.0);
                    let rect = D2D_RECT_F { left: x0, top: y, right: x1, bottom: y + self.line_height };
                    self.target.FillRectangle(&rect, &self.sel_brush);
                }

                if let Some(layout) = &layout {
                    self.target.DrawTextLayout(
                        Vector2 { X: text_pad, Y: y },
                        layout,
                        &self.fg_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                    );
                }

                // Caret.
                if head >= start && head <= text_end {
                    let x = if let Some(layout) = layout.as_ref() {
                        hit_test_x(layout, &text, head - start, text_pad)
                    } else {
                        text_pad
                    };
                    let rect = D2D_RECT_F { left: x, top: y, right: x + CARET_WIDTH, bottom: y + self.line_height };
                    self.target.FillRectangle(&rect, &self.caret_brush);
                }

                y += self.line_height;
                if y > size.height - bottom_reserved {
                    break;
                }
            }

            if show_hints {
                let hints_top = size.height - bottom_reserved;
                self.draw_text_line(crate::hints_text(false, false), PADDING_X, hints_top, size.width - PADDING_X);
            }

            // Barra de estado: franja inferior con la posición del caret, codificación y EOL,
            // alineada a la esquina inferior derecha.
            let status = crate::status_line(&state.doc, state.encoding, state.eol);
            let status_top = size.height - self.line_height;
            let status_wide: Vec<u16> = status.encode_utf16().collect();
            if !status_wide.is_empty() {
                if let Ok(layout) = self._dwrite.CreateTextLayout(
                    &status_wide,
                    &self.text_format,
                    (size.width - PADDING_X).max(0.0),
                    self.line_height,
                ) {
                    let _ = layout.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING);
                    self.target.DrawTextLayout(
                        Vector2 { X: 0.0, Y: status_top },
                        &layout,
                        &self.fg_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                    );
                }
            }

            let _ = self.target.EndDraw(None, None);
        }
    }
}

/// Coordenada X del punto de inserción para el char `local_char_offset` (relativo al
/// inicio de `text`) dentro de `layout`.
unsafe fn hit_test_x(
    layout: &windows::Win32::Graphics::DirectWrite::IDWriteTextLayout,
    text: &str,
    local_char_offset: usize,
    pad: f32,
) -> f32 {
    unsafe {
        let utf16_offset = char_offset_to_utf16(text, local_char_offset);
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        let mut metrics = Default::default();
        if layout.HitTestTextPosition(utf16_offset, false, &mut x, &mut y, &mut metrics).is_ok() {
            pad + x
        } else {
            pad
        }
    }
}
