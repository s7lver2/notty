use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_SIZE_U};
use windows_numerics::{Matrix3x2, Vector2};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BRUSH_PROPERTIES, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_PROPERTIES, D2D1CreateFactory,
    ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
};
use windows::core::Result;

use crate::EditorState;

const FONT_SIZE: f32 = 16.0;
const BG: D2D1_COLOR_F = D2D1_COLOR_F { r: 0.09, g: 0.09, b: 0.10, a: 1.0 };
const FG: D2D1_COLOR_F = D2D1_COLOR_F { r: 0.92, g: 0.92, b: 0.93, a: 1.0 };
const PADDING_X: f32 = 8.0;
const PADDING_TOP: f32 = 8.0;

pub struct Renderer {
    _d2d: ID2D1Factory,
    target: ID2D1HwndRenderTarget,
    _dwrite: IDWriteFactory,
    text_format: IDWriteTextFormat,
    fg_brush: ID2D1SolidColorBrush,
    line_height: f32,
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
            let line_height = FONT_SIZE * 1.35;
            Ok(Self { _d2d: d2d, target, _dwrite: dwrite, text_format, fg_brush, line_height })
        }
    }

    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        unsafe {
            let _ = self.target.Resize(&D2D_SIZE_U { width, height });
        }
    }

    /// Dibuja fondo + las líneas visibles de `state`. No dibuja caret ni selección (Task 6).
    pub fn paint(&mut self, state: &EditorState) {
        let buf = state.doc.buffer();
        let total_lines = buf.len_lines();
        let range = state.viewport.range(total_lines);

        unsafe {
            self.target.BeginDraw();
            self.target.Clear(Some(&BG));

            let mut y = PADDING_TOP;
            for line in range {
                let start = buf.line_start(line);
                let end = if line + 1 < total_lines { buf.line_start(line + 1) } else { buf.len_chars() };
                let text: String = buf.slice(start..end).trim_end_matches(['\r', '\n']).to_string();
                if !text.is_empty() {
                    let wide: Vec<u16> = text.encode_utf16().collect();
                    if let Ok(layout) =
                        self._dwrite.CreateTextLayout(&wide, &self.text_format, f32::MAX, self.line_height)
                    {
                        self.target.DrawTextLayout(
                            Vector2 { X: PADDING_X, Y: y },
                            &layout,
                            &self.fg_brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                        );
                    }
                }
                y += self.line_height;
            }

            let _ = self.target.EndDraw(None, None);
        }
    }
}
