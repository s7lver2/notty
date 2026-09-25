//! Renderer Direct2D/DirectWrite: dibuja con una sola brocha (a la que se le cambia
//! el color antes de cada uso), varios formatos de texto fijos, y va apuntando en
//! `hits` qué rectángulo es qué para el ratón (ver `Hit`/`hit()`).

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_RECT_F, D2D_SIZE_U};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BRUSH_PROPERTIES, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_PROPERTIES, D2D1_ROUNDED_RECT, D2D1CreateFactory,
    ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory,
    IDWriteFactory, IDWriteFontCollection, IDWriteTextFormat,
};
use windows::core::Result;
use windows_numerics::{Matrix3x2, Vector2};

use notty_config::UiConfig;

use crate::layout::{self, Rect};
use crate::theme::{self, Rgba};
use crate::{EditorState, Workspace};

/// Convierte un desplazamiento en chars (relativo al inicio de `text`) a un
/// desplazamiento en unidades UTF-16, que es lo que espera `IDWriteTextLayout`.
fn char_offset_to_utf16(text: &str, char_offset: usize) -> u32 {
    text.chars().take(char_offset).map(|c| c.len_utf16()).sum::<usize>() as u32
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn color(c: Rgba) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.0, g: c.1, b: c.2, a: c.3 }
}

fn rect_of(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

/// Zonas del ratón que `paint` va registrando mientras dibuja; lo último pintado
/// queda encima al recorrer la lista en `hit()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hit {
    #[default]
    None,
    /// Zona libre de la barra de título: arrastrar mueve la ventana.
    Caption,
    Min,
    Max,
    Close,
    Tab(usize),
    TabClose(usize),
    NewTab,
    Menu(usize),
    MenuItem(usize),
    Clickme,
    Pencil,
    Suggestion(usize),
    /// 0 = Aa, 1 = ab, 2 = .*
    SearchOpt(u8),
    /// En el prompt de reemplazar: 0 = campo buscar, 1 = campo «por».
    SearchField(u8),
    Body,
}

/// Contexto que no vive en `Workspace`/`UiConfig` pero que `paint` necesita para
/// saber cómo dibujar: tema, estado del ratón, si la ventana está maximizada/activa
/// y si hay un menú de la barra desplegado.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ViewState {
    pub dark: bool,
    pub hover: Hit,
    pub pressed: Hit,
    pub maximized: bool,
    pub active_window: bool,
    /// Menú `Alt` desplegado, o barra de menús visible.
    pub menu_bar_visible: bool,
    pub open_menu: Option<usize>,
}

/// Los formatos de texto fijos que usa la maqueta, creados una vez en `Renderer::new`.
/// Varios campos y helpers de dibujo (`text`, `stroke_*`) no se usan todavía: entran
/// en juego a partir de la Task 7, cuando se dibujan pestañas/menú/atajos/estado.
#[allow(dead_code)]
struct Fonts {
    ui_12: IDWriteTextFormat,
    ui_12_5: IDWriteTextFormat,
    ui_11: IDWriteTextFormat,
    ui_11_5: IDWriteTextFormat,
    ui_13: IDWriteTextFormat,
    ui_20_semibold: IDWriteTextFormat,
    ui_11_5_semibold: IDWriteTextFormat,
    ui_9: IDWriteTextFormat,
    mono_13: IDWriteTextFormat,
    mono_13_bold: IDWriteTextFormat,
    mono_11: IDWriteTextFormat,
    mono_11_bold: IDWriteTextFormat,
    mono_11_5_semibold: IDWriteTextFormat,
    mono_12: IDWriteTextFormat,
    mono_12_5: IDWriteTextFormat,
}

/// Busca `primary` en la colección de fuentes del sistema; si no existe, usa `fallback`.
fn resolve_family(collection: &IDWriteFontCollection, primary: &str, fallback: &str) -> String {
    unsafe {
        let mut index = 0u32;
        let mut exists = windows::core::BOOL(0);
        let name_wide: Vec<u16> = primary.encode_utf16().chain(std::iter::once(0)).collect();
        if collection
            .FindFamilyName(windows::core::PCWSTR(name_wide.as_ptr()), &mut index, &mut exists)
            .is_ok()
            && exists.as_bool()
        {
            primary.to_string()
        } else {
            fallback.to_string()
        }
    }
}

fn make_format(
    dwrite: &IDWriteFactory,
    family: &str,
    size: f32,
    weight: windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT,
) -> Result<IDWriteTextFormat> {
    let family_wide: Vec<u16> = family.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let fmt = dwrite.CreateTextFormat(
            windows::core::PCWSTR(family_wide.as_ptr()),
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            windows::core::w!(""),
        )?;
        let _ = fmt.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        Ok(fmt)
    }
}

/// Añade recorte con «…» (usado en pestañas, nombres largos) al formato dado.
fn with_ellipsis_trimming(dwrite: &IDWriteFactory, fmt: &IDWriteTextFormat) -> Result<()> {
    unsafe {
        let sign = dwrite.CreateEllipsisTrimmingSign(fmt)?;
        let trimming =
            DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
        fmt.SetTrimming(&trimming, &sign)?;
    }
    Ok(())
}

pub struct Renderer {
    _d2d: ID2D1Factory,
    target: ID2D1HwndRenderTarget,
    dwrite: IDWriteFactory,
    brush: ID2D1SolidColorBrush,
    fonts: Fonts,
    dpi: u32,
    hits: Vec<(Rect, Hit)>,
}

impl Renderer {
    pub fn new(hwnd: HWND, dpi: u32) -> Result<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let mut client = windows::Win32::Foundation::RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client);
            let width = (client.right - client.left).max(1) as u32;
            let height = (client.bottom - client.top).max(1) as u32;
            let target = d2d.CreateHwndRenderTarget(
                &D2D1_RENDER_TARGET_PROPERTIES::default(),
                &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                    hwnd,
                    pixelSize: D2D_SIZE_U { width, height },
                    ..Default::default()
                },
            )?;
            target.SetDpi(dpi as f32, dpi as f32);

            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let sys_fonts: IDWriteFontCollection = {
                let mut collection: Option<IDWriteFontCollection> = None;
                dwrite.GetSystemFontCollection(&mut collection, false)?;
                collection.expect("GetSystemFontCollection debe devolver una colección")
            };
            let ui_family = resolve_family(&sys_fonts, "Segoe UI Variable Text", "Segoe UI");
            let mono_family = resolve_family(&sys_fonts, "Cascadia Mono", "Consolas");

            let ui_12 = make_format(&dwrite, &ui_family, layout::FONT_UI, DWRITE_FONT_WEIGHT_NORMAL)?;
            with_ellipsis_trimming(&dwrite, &ui_12)?;
            let ui_12_5 = make_format(&dwrite, &ui_family, layout::FONT_MENU, DWRITE_FONT_WEIGHT_NORMAL)?;
            let ui_11 = make_format(&dwrite, &ui_family, layout::FONT_STATUS, DWRITE_FONT_WEIGHT_NORMAL)?;
            let ui_11_5 = make_format(&dwrite, &ui_family, layout::FONT_PROMPT_LABEL, DWRITE_FONT_WEIGHT_NORMAL)?;
            let ui_13 = make_format(&dwrite, &ui_family, 13.0, DWRITE_FONT_WEIGHT_NORMAL)?;
            let ui_20_semibold = make_format(&dwrite, &ui_family, 20.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let ui_11_5_semibold =
                make_format(&dwrite, &ui_family, layout::FONT_PROMPT_LABEL, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let ui_9 = make_format(&dwrite, &ui_family, 9.0, DWRITE_FONT_WEIGHT_NORMAL)?;
            let mono_13 = make_format(&dwrite, &mono_family, layout::FONT_MONO, DWRITE_FONT_WEIGHT_NORMAL)?;
            let mono_13_bold = make_format(&dwrite, &mono_family, layout::FONT_MONO, DWRITE_FONT_WEIGHT_BOLD)?;
            let mono_11 = make_format(&dwrite, &mono_family, layout::FONT_HINTS, DWRITE_FONT_WEIGHT_NORMAL)?;
            let mono_11_bold = make_format(&dwrite, &mono_family, layout::FONT_HINTS, DWRITE_FONT_WEIGHT_BOLD)?;
            let mono_11_5_semibold = make_format(&dwrite, &mono_family, 11.5, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let mono_12 = make_format(&dwrite, &mono_family, layout::FONT_SUGGEST, DWRITE_FONT_WEIGHT_NORMAL)?;
            with_ellipsis_trimming(&dwrite, &mono_12)?;
            let mono_12_5 = make_format(&dwrite, &mono_family, layout::FONT_PROMPT, DWRITE_FONT_WEIGHT_NORMAL)?;

            let brush = target.CreateSolidColorBrush(
                &color(Rgba(1.0, 1.0, 1.0, 1.0)),
                Some(&D2D1_BRUSH_PROPERTIES { opacity: 1.0, transform: Matrix3x2::identity() }),
            )?;

            Ok(Self {
                _d2d: d2d,
                target,
                dwrite,
                brush,
                fonts: Fonts {
                    ui_12,
                    ui_12_5,
                    ui_11,
                    ui_11_5,
                    ui_13,
                    ui_20_semibold,
                    ui_11_5_semibold,
                    ui_9,
                    mono_13,
                    mono_13_bold,
                    mono_11,
                    mono_11_bold,
                    mono_11_5_semibold,
                    mono_12,
                    mono_12_5,
                },
                dpi,
                hits: Vec::new(),
            })
        }
    }

    /// Alto de línea del documento en DIPs (`--lh` de la maqueta).
    pub fn line_height(&self) -> f32 {
        layout::LINE_H
    }

    pub fn set_dpi(&mut self, dpi: u32) {
        self.dpi = dpi;
        unsafe {
            self.target.SetDpi(dpi as f32, dpi as f32);
        }
    }

    /// `dpi / 96`: factor para convertir píxeles físicos del ratón a DIPs.
    pub fn scale(&self) -> f32 {
        self.dpi as f32 / 96.0
    }

    /// Tamaño del render target en DIPs (ya escalado por Direct2D con `SetDpi`).
    pub fn size_dips(&self) -> (f32, f32) {
        let s = unsafe { self.target.GetSize() };
        (s.width, s.height)
    }

    /// Ancho de un dígito con `mono_13`, usado para el hueco de los números de línea.
    fn digit_width(&self) -> f32 {
        self.measure("0", &self.fonts.mono_13).max(1.0)
    }

    /// Recorre las zonas registradas en el último `paint`, de la última a la primera
    /// (lo último dibujado queda encima). Si nada coincide: `Caption` en la franja de
    /// título, `None` en cualquier otro sitio.
    pub fn hit(&self, x_dip: f32, y_dip: f32) -> Hit {
        for &(r, h) in self.hits.iter().rev() {
            if r.contains(x_dip, y_dip) {
                return h;
            }
        }
        if y_dip < layout::TITLEBAR_H { Hit::Caption } else { Hit::None }
    }

    // --- Helpers de dibujo con la brocha única --------------------------------------

    fn fill(&self, r: Rect, c: Rgba) {
        if r.is_empty() {
            return;
        }
        unsafe {
            self.brush.SetColor(&color(c));
            self.target.FillRectangle(&rect_of(r), &self.brush);
        }
    }

    fn fill_round(&self, r: Rect, radius: f32, c: Rgba) {
        if r.is_empty() {
            return;
        }
        unsafe {
            self.brush.SetColor(&color(c));
            self.target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT { rect: rect_of(r), radiusX: radius, radiusY: radius },
                &self.brush,
            );
        }
    }

    #[allow(dead_code)]
    fn stroke_line(&self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, c: Rgba) {
        unsafe {
            self.brush.SetColor(&color(c));
            self.target.DrawLine(Vector2 { X: x0, Y: y0 }, Vector2 { X: x1, Y: y1 }, &self.brush, width, None);
        }
    }

    #[allow(dead_code)]
    fn stroke_rect(&self, r: Rect, width: f32, c: Rgba) {
        if r.is_empty() {
            return;
        }
        unsafe {
            self.brush.SetColor(&color(c));
            self.target.DrawRectangle(&rect_of(r), &self.brush, width, None);
        }
    }

    #[allow(dead_code)]
    fn stroke_round_rect(&self, r: Rect, radius: f32, width: f32, c: Rgba) {
        if r.is_empty() {
            return;
        }
        unsafe {
            self.brush.SetColor(&color(c));
            self.target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT { rect: rect_of(r), radiusX: radius, radiusY: radius },
                &self.brush,
                width,
                None,
            );
        }
    }

    /// Dibuja `s` con `fmt` en `r`, centrado verticalmente en su alto (recorte con
    /// «…» si el formato lo tiene configurado).
    #[allow(dead_code)]
    fn text(&self, s: &str, fmt: &IDWriteTextFormat, r: Rect, c: Rgba) {
        self.text_aligned(s, fmt, r, c, false);
    }

    /// Igual que `text`, pero alineado a la derecha de `r`.
    #[allow(dead_code)]
    fn text_right(&self, s: &str, fmt: &IDWriteTextFormat, r: Rect, c: Rgba) {
        self.text_aligned(s, fmt, r, c, true);
    }

    fn text_aligned(&self, s: &str, fmt: &IDWriteTextFormat, r: Rect, c: Rgba, right: bool) {
        let w = wide(s);
        if w.is_empty() || r.width() <= 0.0 {
            return;
        }
        unsafe {
            if let Ok(layout) = self.dwrite.CreateTextLayout(&w, fmt, r.width().max(0.0), r.height().max(0.0)) {
                let _ = layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                if right {
                    let _ = layout.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING);
                }
                self.brush.SetColor(&color(c));
                self.target.DrawTextLayout(
                    Vector2 { X: r.left, Y: r.top },
                    &layout,
                    &self.brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                );
            }
        }
    }

    /// Ancho de `s` con `fmt`, sin límite (para medir nombres de pestaña, etc.).
    fn measure(&self, s: &str, fmt: &IDWriteTextFormat) -> f32 {
        let w = wide(s);
        if w.is_empty() {
            return 0.0;
        }
        unsafe {
            if let Ok(layout) = self.dwrite.CreateTextLayout(&w, fmt, f32::MAX, f32::MAX) {
                let mut metrics = Default::default();
                if layout.GetMetrics(&mut metrics).is_ok() {
                    return metrics.widthIncludingTrailingWhitespace;
                }
            }
        }
        0.0
    }

    /// Rectángulo del cuerpo y ancho del canal de números para el tamaño actual de la
    /// ventana, `total_lines` líneas y `doc_count`/`menu_bar_visible` (mismas bandas
    /// que resuelve `paint`). Lo usa `window.rs` para el hit-testing del ratón sobre
    /// el documento y para `WM_SIZE`.
    pub fn body_and_gutter(&self, ui: &UiConfig, doc_count: usize, menu_bar_visible: bool, total_lines: usize, is_raw: bool) -> (Rect, f32) {
        let (w, h) = self.size_dips();
        let bands = Self::resolve_bands(ui, doc_count, menu_bar_visible);
        let frame = layout::frame(w, h, bands);
        let gutter_w =
            if ui.line_numbers && !is_raw { layout::gutter_width(total_lines, self.digit_width()) } else { 0.0 };
        (frame.body, gutter_w)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        unsafe {
            let _ = self.target.Resize(&D2D_SIZE_U { width, height });
        }
    }

    /// Traduce un punto del cliente (en DIPs, origen arriba-izquierda) al índice de
    /// char del documento más cercano, usando `state.viewport` para saber qué línea es cada fila.
    pub fn char_index_at(&self, state: &EditorState, body: Rect, gutter_w: f32, x: f32, y: f32) -> usize {
        let buf = state.doc.buffer();
        let total = buf.len_lines();
        let range = state.viewport.range(total);
        let row = ((y - body.top - layout::TEXT_PAD_T) / layout::LINE_H).floor().max(0.0) as usize;
        let line = (state.viewport.first_line + row).min(total.saturating_sub(1)).max(range.start);

        let start = buf.line_start(line);
        let end = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
        let text: String = buf.slice(start..end).trim_end_matches(['\r', '\n']).to_string();
        let text_pad = body.left + gutter_w + layout::TEXT_PAD_L;
        if text.is_empty() {
            return start;
        }
        let w = wide(&text);
        let Ok(text_layout) = (unsafe { self.dwrite.CreateTextLayout(&w, &self.fonts.mono_13, f32::MAX, layout::LINE_H) })
        else {
            return start;
        };
        let mut trailing = windows::core::BOOL(0);
        let mut inside = windows::core::BOOL(0);
        let mut metrics = Default::default();
        if unsafe { text_layout.HitTestPoint(x - text_pad, 0.0, &mut trailing, &mut inside, &mut metrics) }.is_ok() {
            let utf16_offset = metrics.textPosition as usize + if trailing.as_bool() { 1 } else { 0 };
            let prefix = String::from_utf16_lossy(&w[..utf16_offset.min(w.len())]);
            start + prefix.chars().count()
        } else {
            start
        }
    }

    /// Qué franjas se muestran, resuelto a partir de `ui`, el número de documentos y
    /// si el menú `Alt` está desplegado (maqueta, `tabsVisible()` línea 462).
    fn resolve_bands(ui: &UiConfig, doc_count: usize, menu_bar_visible: bool) -> layout::Bands {
        use notty_config::{Files, TabsPosition};
        let tabs_in_title = ui.files == Files::Tabs
            && (ui.tabs_position == TabsPosition::Title
                || (ui.tabs_position == TabsPosition::Auto && doc_count > 1));
        let tabs_below = ui.files == Files::Tabs && ui.tabs_position == TabsPosition::Below;
        layout::Bands {
            tabs_in_title,
            menubar: menu_bar_visible,
            tabs_below,
            hints: ui.hints_bar && !ui.merged_command_line,
            merged_status: ui.merged_command_line,
        }
    }

    /// Dibuja fondo + documento del `Workspace` activo, con las medidas y colores de
    /// la maqueta: barra de título propia (icono, pestañas o título, botones), barra
    /// de menús, pestañas debajo, canal de números, documento y barra de atajos. La
    /// barra de estado / prompts y la vista raw llegan en las Tasks 8-9.
    pub fn paint(&mut self, ws: &Workspace, ui: &UiConfig, view: &ViewState) {
        self.hits.clear();
        let pal = theme::palette(view.dark);
        let state = ws.active();
        let buf = state.doc.buffer();
        let is_raw = state.raw.is_some();
        let total_lines = buf.len_lines();
        let range = state.viewport.range(total_lines);
        let sel = state.doc.selection();
        let sel_range = sel.range();
        let head = sel.head;
        let cursor_line = state.doc.line_col().0;

        let (search_matches, search_current): (Vec<std::ops::Range<usize>>, Option<usize>) = match &ws.prompt {
            crate::Prompt::Find(s) | crate::Prompt::Replace(s) => {
                (s.matches(&state.doc).unwrap_or_default(), Some(s.current))
            }
            _ => (Vec::new(), None),
        };

        let (w, h) = self.size_dips();
        let bands = Self::resolve_bands(ui, ws.len(), view.menu_bar_visible);
        let frame = layout::frame(w, h, bands);
        // El botón de la barra de título queda cubierto por las pestañas/botones que
        // se registran después (hit() prioriza lo último registrado).
        self.hits.push((frame.titlebar, Hit::Caption));
        self.hits.push((frame.body, Hit::Body));

        let gutter_w = if ui.line_numbers && !is_raw { layout::gutter_width(total_lines, self.digit_width()) } else { 0.0 };
        let text_pad = frame.body.left + gutter_w + layout::TEXT_PAD_L;

        unsafe {
            self.target.BeginDraw();
            self.target.Clear(Some(&color(pal.surface)));

            self.draw_titlebar(ws, ui, view, pal, frame);
            if bands.menubar {
                self.draw_menubar(view, pal, frame);
            }
            if bands.tabs_below {
                self.draw_tabs_row(ws, view, pal, layout::TABS_BELOW_PAD_X, w - layout::TABS_BELOW_PAD_X, frame.tabs_below.bottom);
                self.fill(Rect::new(0.0, frame.tabs_below.top, w, frame.tabs_below.top), pal.chrome);
            }

            let mut y = frame.body.top + layout::TEXT_PAD_T;
            for line in range {
                let start = buf.line_start(line);
                let full_end = if line + 1 < total_lines { buf.line_start(line + 1) } else { buf.len_chars() };
                let text: String = buf.slice(start..full_end).trim_end_matches(['\r', '\n']).to_string();
                let text_end = start + text.chars().count();
                let w16 = wide(&text);

                let text_layout = if w16.is_empty() {
                    None
                } else {
                    self.dwrite.CreateTextLayout(&w16, &self.fonts.mono_13, f32::MAX, layout::LINE_H).ok()
                };

                if ui.line_numbers && !is_raw {
                    let num = (line + 1).to_string();
                    let num_color = if line == cursor_line { pal.text } else { pal.text_3 };
                    self.text_right(
                        &num,
                        &self.fonts.mono_13,
                        Rect::new(frame.body.left, y, frame.body.left + gutter_w - layout::GUTTER_PAD_R, y + layout::LINE_H),
                        num_color,
                    );
                }

                if !sel.is_empty() && sel_range.start < full_end && sel_range.end > start {
                    let clamp_start = sel_range.start.max(start);
                    let clamp_end = sel_range.end.min(text_end);
                    let x0 = if let (true, Some(l)) = (clamp_end > clamp_start, text_layout.as_ref()) {
                        hit_test_x(l, &text, clamp_start - start, text_pad)
                    } else {
                        text_pad
                    };
                    let mut x1 = if let (true, Some(l)) = (clamp_end > clamp_start, text_layout.as_ref()) {
                        hit_test_x(l, &text, clamp_end - start, text_pad)
                    } else {
                        text_pad
                    };
                    if sel_range.end > text_end {
                        x1 = x1.max(x0) + 6.0;
                    }
                    x1 = x1.max(x0 + 2.0);
                    self.fill(Rect::new(x0, y, x1, y + layout::LINE_H), pal.accent_soft);
                }

                for (mi, m) in search_matches.iter().enumerate() {
                    if m.start >= full_end || m.end <= start {
                        continue;
                    }
                    let clamp_start = m.start.max(start);
                    let clamp_end = m.end.min(text_end);
                    if clamp_end <= clamp_start {
                        continue;
                    }
                    let x0 =
                        text_layout.as_ref().map(|l| hit_test_x(l, &text, clamp_start - start, text_pad)).unwrap_or(text_pad);
                    let x1 =
                        text_layout.as_ref().map(|l| hit_test_x(l, &text, clamp_end - start, text_pad)).unwrap_or(text_pad);
                    let c = if Some(mi) == search_current { pal.mark_cur } else { pal.mark };
                    self.fill_round(Rect::new(x0, y, x1.max(x0 + 2.0), y + layout::LINE_H), 2.0, c);
                }

                if let Some(l) = &text_layout {
                    self.brush.SetColor(&color(pal.text));
                    self.target.DrawTextLayout(
                        Vector2 { X: text_pad, Y: y },
                        l,
                        &self.brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                    );
                }

                if head >= start && head <= text_end {
                    let x = if let Some(l) = text_layout.as_ref() { hit_test_x(l, &text, head - start, text_pad) } else { text_pad };
                    let caret_color = if state.vim.as_ref().is_some_and(|v| v.mode == crate::VimMode::Normal) {
                        pal.accent
                    } else {
                        pal.text
                    };
                    self.fill(Rect::new(x, y, x + 1.0, y + layout::LINE_H), caret_color);
                }

                y += layout::LINE_H;
                if y > frame.body.bottom {
                    break;
                }
            }

            if bands.hints {
                self.draw_hints(state, ws, pal, frame);
            }

            let _ = self.target.EndDraw(None, None);
        }
    }

    /// Icono, pestañas o título, y botones de min/max/cerrar (`.titlebar`).
    #[allow(unused_unsafe)]
    unsafe fn draw_titlebar(&mut self, ws: &Workspace, ui: &UiConfig, view: &ViewState, pal: &theme::Palette, frame: layout::Frame) {
        unsafe {
            self.fill(frame.titlebar, pal.chrome);

            // Icono de la app: rectángulo redondeado + tres líneas, en los primeros 36px.
            let icon_x = (layout::APPICON_W - layout::APPICON_SIZE) / 2.0;
            let icon_y = (layout::TITLEBAR_H - layout::APPICON_SIZE) / 2.0;
            self.stroke_round_rect(
                Rect::new(icon_x + 2.5, icon_y + 1.5, icon_x + 2.5 + 11.0, icon_y + 1.5 + 13.0),
                2.0,
                1.4,
                pal.accent,
            );
            self.stroke_line(icon_x + 5.0, icon_y + 5.5, icon_x + 11.0, icon_y + 5.5, 1.3, pal.text_2);
            self.stroke_line(icon_x + 5.0, icon_y + 8.0, icon_x + 11.0, icon_y + 8.0, 1.3, pal.text_2);
            self.stroke_line(icon_x + 5.0, icon_y + 10.5, icon_x + 8.5, icon_y + 10.5, 1.3, pal.text_2);

            let bands = Self::resolve_bands(ui, ws.len(), view.menu_bar_visible);
            if bands.tabs_in_title {
                self.draw_tabs_row(ws, view, pal, layout::APPICON_W, frame.caption_min.left, frame.titlebar.bottom);
            } else {
                let name = crate::doc_name(ws.active().path.as_deref());
                let title = crate::window_title(&name, ws.active().doc.is_dirty(), ui.preset == notty_config::Preset::Zen);
                self.text(
                    &title,
                    &self.fonts.ui_12,
                    Rect::new(layout::APPICON_W, 0.0, frame.caption_min.left, layout::TITLEBAR_H),
                    pal.text_2,
                );
            }

            self.draw_caption_button(view, pal, frame.caption_min, Hit::Min, false);
            self.draw_caption_button(view, pal, frame.caption_max, Hit::Max, view.maximized);
            self.draw_caption_button(view, pal, frame.caption_close, Hit::Close, false);
        }
    }

    #[allow(unused_unsafe)]
    unsafe fn draw_caption_button(&mut self, view: &ViewState, pal: &theme::Palette, r: Rect, hit: Hit, maximized: bool) {
        unsafe {
            self.hits.push((r, hit));
            let hovered = view.hover == hit;
            let is_close = hit == Hit::Close;
            if hovered {
                self.fill(r, if is_close { pal.close_hover } else { pal.hover });
            }
            let glyph_color = if hovered && is_close { pal.close_hover_fg } else { pal.text_2 };
            let cx = r.left + (r.width() - 10.0) / 2.0;
            let cy = r.top + (r.height() - 10.0) / 2.0;
            match hit {
                Hit::Min => self.stroke_line(cx, cy + 5.0, cx + 10.0, cy + 5.0, 1.0, glyph_color),
                Hit::Max if maximized => {
                    self.stroke_rect(Rect::new(cx + 2.0, cy, cx + 10.0, cy + 8.0), 1.0, glyph_color);
                    self.stroke_rect(Rect::new(cx, cy + 2.0, cx + 8.0, cy + 10.0), 1.0, glyph_color);
                }
                Hit::Max => self.stroke_rect(Rect::new(cx + 0.5, cy + 0.5, cx + 9.0, cy + 9.0), 1.0, glyph_color),
                Hit::Close => {
                    self.stroke_line(cx, cy, cx + 10.0, cy + 10.0, 1.0, glyph_color);
                    self.stroke_line(cx + 10.0, cy, cx, cy + 10.0, 1.0, glyph_color);
                }
                _ => {}
            }
        }
    }

    /// Barra de menús (`.menubar`): "Archivo Editar Buscar Ver Ayuda".
    #[allow(unused_unsafe)]
    unsafe fn draw_menubar(&mut self, view: &ViewState, pal: &theme::Palette, frame: layout::Frame) {
        unsafe {
            self.fill(frame.menubar, pal.chrome);
            let labels = ["Archivo", "Editar", "Buscar", "Ver", "Ayuda"];
            let mut x = layout::MENUBAR_PAD_X;
            let top = frame.menubar.top + (layout::MENUBAR_H - 22.0) / 2.0;
            for (i, label) in labels.iter().enumerate() {
                let w = self.measure(label, &self.fonts.ui_12_5) + layout::MENU_BTN_PAD_X * 2.0;
                let r = Rect::new(x, top, x + w, top + 22.0);
                let hovered = view.hover == Hit::Menu(i) || view.open_menu == Some(i);
                if hovered {
                    self.fill_round(r, 4.0, pal.hover);
                }
                self.text(label, &self.fonts.ui_12_5, r, pal.text);
                self.hits.push((r, Hit::Menu(i)));
                x += w + layout::MENU_BTN_GAP;
            }
        }
    }

    /// Fila de pestañas compartida entre la barra de título y `.tabs.below`.
    #[allow(unused_unsafe)]
    unsafe fn draw_tabs_row(&mut self, ws: &Workspace, view: &ViewState, pal: &theme::Palette, x0: f32, max_right: f32, bottom: f32) {
        unsafe {
            let names: Vec<String> = ws.iter().map(|d| crate::doc_name(d.path.as_deref())).collect();
            let dirty: Vec<bool> = ws.iter().map(|d| d.doc.is_dirty()).collect();
            let name_w: Vec<f32> = names.iter().map(|n| self.measure(n, &self.fonts.ui_12)).collect();
            let dot_w = self.measure("●", &self.fonts.ui_9);
            let (tabs, plus) = layout::tabs(x0, bottom, max_right, &name_w, &dirty, dot_w);

            for (i, t) in tabs.iter().enumerate() {
                let active = i == ws.active_index();
                let hovered_tab = view.hover == Hit::Tab(i) || view.hover == Hit::TabClose(i);
                if active {
                    self.fill_round(t.rect, layout::TAB_RADIUS, pal.surface);
                    self.fill(Rect::new(t.rect.left, t.rect.bottom - layout::TAB_RADIUS, t.rect.right, t.rect.bottom), pal.surface);
                } else if hovered_tab {
                    self.fill_round(t.rect, layout::TAB_RADIUS, pal.hover);
                    self.fill(Rect::new(t.rect.left, t.rect.bottom - layout::TAB_RADIUS, t.rect.right, t.rect.bottom), pal.hover);
                }
                let name_color = if active { pal.text } else { pal.text_2 };
                self.text(&names[i], &self.fonts.ui_12, Rect::new(t.name_x, t.rect.top, t.name_x + t.name_w, t.rect.bottom), name_color);
                if let Some(dot_x) = t.dot_x {
                    self.text(
                        "●",
                        &self.fonts.ui_9,
                        Rect::new(dot_x, t.rect.top, dot_x + dot_w, t.rect.bottom),
                        pal.text_2,
                    );
                }
                self.hits.push((t.rect, Hit::Tab(i)));
                if active || hovered_tab {
                    let close_hovered = view.hover == Hit::TabClose(i);
                    if close_hovered {
                        self.fill_round(t.close, 4.0, pal.hover);
                    }
                    let cc = if close_hovered { pal.text } else { pal.text_3 };
                    self.stroke_line(t.close.left + 1.0, t.close.top + 1.0, t.close.right - 1.0, t.close.bottom - 1.0, 1.1, cc);
                    self.stroke_line(t.close.right - 1.0, t.close.top + 1.0, t.close.left + 1.0, t.close.bottom - 1.0, 1.1, cc);
                    self.hits.push((t.close, Hit::TabClose(i)));
                }
            }

            let plus_hovered = view.hover == Hit::NewTab;
            if plus_hovered {
                self.fill_round(plus, 5.0, pal.hover);
            }
            let pc = pal.text_3;
            let pcx = plus.left + plus.width() / 2.0;
            let pcy = plus.top + plus.height() / 2.0;
            self.stroke_line(pcx - 5.0, pcy, pcx + 5.0, pcy, 1.1, pc);
            self.stroke_line(pcx, pcy - 5.0, pcx, pcy + 5.0, 1.1, pc);
            self.hits.push((plus, Hit::NewTab));
        }
    }

    /// Barra de atajos (`.hints`): fondo `surface_2`, línea superior, pares tecla/acción.
    #[allow(unused_unsafe)]
    unsafe fn draw_hints(&self, state: &EditorState, ws: &Workspace, pal: &theme::Palette, frame: layout::Frame) {
        unsafe {
            self.fill(frame.hints, pal.surface_2);
            self.stroke_line(0.0, frame.hints.top, frame.hints.width(), frame.hints.top, 1.0, pal.line);

            let ctx = if !matches!(ws.prompt, crate::Prompt::None) {
                match &ws.prompt {
                    crate::Prompt::Path(p) if p.purpose == crate::Purpose::Save => crate::HintsCtx::PathSave,
                    crate::Prompt::Path(_) => crate::HintsCtx::PathOpen,
                    crate::Prompt::Find(_) => crate::HintsCtx::Find,
                    crate::Prompt::Replace(_) => crate::HintsCtx::Replace,
                    _ => crate::HintsCtx::Normal,
                }
            } else if state.raw.is_some() {
                crate::HintsCtx::Raw
            } else if let Some(vim) = &state.vim {
                if vim.mode == crate::VimMode::Insert { crate::HintsCtx::VimInsert } else { crate::HintsCtx::VimNormal }
            } else {
                crate::HintsCtx::Normal
            };

            let mut x = layout::HINTS_PAD_X;
            let items = crate::hints_items(ctx);
            for (key, action) in items {
                let key_w = self.measure(key, &self.fonts.mono_11_bold);
                self.text(key, &self.fonts.mono_11_bold, Rect::new(x, frame.hints.top, x + key_w, frame.hints.bottom), pal.text_hint);
                x += key_w;
                let space_w = self.measure(" ", &self.fonts.mono_11);
                x += space_w;
                let action_w = self.measure(action, &self.fonts.mono_11);
                self.text(action, &self.fonts.mono_11, Rect::new(x, frame.hints.top, x + action_w, frame.hints.bottom), pal.text_2);
                x += action_w + layout::HINTS_GAP;
            }
        }
    }
}

/// Coordenada X del punto de inserción para el char `local_char_offset` (relativo al
/// inicio de `text`) dentro de `layout`.
unsafe fn hit_test_x(
    text_layout: &windows::Win32::Graphics::DirectWrite::IDWriteTextLayout,
    text: &str,
    local_char_offset: usize,
    pad: f32,
) -> f32 {
    unsafe {
        let utf16_offset = char_offset_to_utf16(text, local_char_offset);
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        let mut metrics = Default::default();
        if text_layout.HitTestTextPosition(utf16_offset, false, &mut x, &mut y, &mut metrics).is_ok() {
            pad + x
        } else {
            pad
        }
    }
}
