//! Previsualización de Markdown (Ajustes → Archivos), al estilo de GitHub: letra
//! proporcional, encabezados grandes con raya debajo, cajas de código con resaltado,
//! citas con barra, avisos `> [!NOTE]`, viñetas y casillas, tablas con bordes…
//!
//! Cada línea del documento sigue siendo un `IDWriteTextLayout` con su texto intacto
//! (ver `markdown.rs`): las marcas se encogen a ancho ~0 en vez de quitarse, así que
//! cursor, selección y búsqueda siguen calculando sobre las mismas posiciones.

use std::rc::Rc;

use windows::Win32::Graphics::Direct2D::{D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, ID2D1SolidColorBrush};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_LINE_SPACING_METHOD_UNIFORM, DWRITE_TEXT_METRICS,
    DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_WRAP, IDWriteTextLayout,
};
use windows_numerics::Vector2;

use super::{Renderer, char_offset_to_utf16, color, hit_test_range_rects, hit_test_xy, wide};
use crate::EditorState;
use crate::layout::{self, Rect};
use crate::markdown::{Align, Alert, BlockKind, Line, ListMarker, MdDoc, StyledRange};
use crate::theme::{Palette, Rgba};

/// Texto normal respecto al tamaño de la letra del editor.
const PROSE: f32 = 1.08;
/// Sangría por nivel de lista y por nivel de cita.
const LIST_W: f32 = 24.0;
const QUOTE_W: f32 = 18.0;
/// Margen interior de las cajas de código.
const CODE_PAD: f32 = 16.0;
const CELL_PAD_X: f32 = 13.0;
const CELL_PAD_Y: f32 = 6.0;
/// Margen a cada lado del documento (GitHub no pega el texto al borde).
const MARGIN: f32 = 20.0;

struct Geom {
    /// Donde empieza el texto.
    x: f32,
    /// Izquierda del bloque (caja de código, raya del encabezado…).
    block_x: f32,
    pad_top: f32,
    pad_bottom: f32,
    /// Escala del interlineado (encabezados grandes).
    scale: f32,
}

struct Brushes {
    text_2: Option<ID2D1SolidColorBrush>,
    dim: Option<ID2D1SolidColorBrush>,
    link: Option<ID2D1SolidColorBrush>,
    alert: [Option<ID2D1SolidColorBrush>; 5],
    syntax: Vec<Option<ID2D1SolidColorBrush>>,
}

fn alert_color(pal: &Palette, a: Alert) -> Rgba {
    match a {
        Alert::Note => pal.accent,
        Alert::Tip => pal.ok,
        Alert::Important => pal.syn_function,
        Alert::Warning => pal.warn,
        Alert::Caution => pal.danger,
    }
}

fn border(pal: &Palette) -> Rgba {
    pal.text_3.faded(0.45)
}

fn wide0(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn range16(text: &str, s: usize, e: usize) -> DWRITE_TEXT_RANGE {
    let a = char_offset_to_utf16(text, s);
    DWRITE_TEXT_RANGE { startPosition: a, length: char_offset_to_utf16(text, e).saturating_sub(a) }
}

fn line_text(buf: &notty_core::Buffer, line: usize, total: usize) -> String {
    let s = buf.line_start(line);
    let e = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
    buf.slice(s..e).trim_end_matches(['\r', '\n']).to_string()
}

impl Renderer {
    /// El análisis del documento, recalculado solo cuando cambia el texto.
    pub(super) fn md_doc(&self, state: &EditorState) -> Rc<MdDoc> {
        let rev = state.doc.revision();
        if let Some((r, d)) = state.md_cache.borrow().as_ref() {
            if *r == rev {
                return d.clone();
            }
        }
        let texts = state.doc.buffer().line_strings();
        let d = Rc::new(crate::markdown::analyze(&texts));
        *state.md_cache.borrow_mut() = Some((rev, d.clone()));
        d
    }

    fn md_geom(&self, info: Option<&Line>, rendered: bool, base_x: f32) -> Geom {
        let Some(info) = info else {
            return Geom { x: base_x, block_x: base_x, pad_top: 0.0, pad_bottom: 0.0, scale: 1.0 };
        };
        let block_x = base_x + info.quote as f32 * QUOTE_W + info.indent.saturating_sub(matches!(info.block, BlockKind::ListItem(_)) as u8) as f32 * LIST_W;
        let mut x = base_x + info.quote as f32 * QUOTE_W + info.indent as f32 * LIST_W;
        if info.is_code_box() {
            x += CODE_PAD;
        }
        let (pad_top, pad_bottom, scale) = match (rendered, info.block) {
            (true, BlockKind::Heading(n @ (1 | 2))) => (8.0, 12.0, crate::markdown::heading_scale(n)),
            (true, BlockKind::Heading(n)) => (6.0, 4.0, crate::markdown::heading_scale(n).max(1.0)),
            _ => (0.0, 0.0, 1.0),
        };
        Geom { x, block_x, pad_top, pad_bottom, scale }
    }

    /// Aplica los rangos de `spans` que caen en `[from, to)` de `text` a `l`, cuyo texto
    /// empieza en el char `from` de `text`. Con `b`, también los colores.
    unsafe fn md_apply_spans(&self, l: &IDWriteTextLayout, text: &str, spans: &[StyledRange], from: usize, to: usize, b: Option<&Brushes>, alert: Option<Alert>) {
        unsafe {
            let sub: String = text.chars().skip(from).take(to - from).collect();
            let base = layout::FONT_MONO * self.font_scale;
            let mono = wide0(&self.mono_family);
            let clip = |s: &StyledRange| {
                let (a, e) = (s.start.max(from), s.end.min(to));
                (e > a).then(|| range16(&sub, a - from, e - from))
            };
            for s in spans.iter().filter(|s| !s.style.hidden) {
                let Some(r) = clip(s) else { continue };
                if s.style.bold {
                    let _ = l.SetFontWeight(DWRITE_FONT_WEIGHT_SEMI_BOLD, r);
                }
                if s.style.italic {
                    let _ = l.SetFontStyle(DWRITE_FONT_STYLE_ITALIC, r);
                }
                if s.style.strike {
                    let _ = l.SetStrikethrough(true, r);
                }
                if s.style.code {
                    let _ = l.SetFontFamilyName(windows::core::PCWSTR(mono.as_ptr()), r);
                    let _ = l.SetFontSize(base * 0.92, r);
                }
                if let Some(b) = b {
                    let brush = if s.style.link {
                        b.link.as_ref()
                    } else if s.style.dim {
                        b.dim.as_ref()
                    } else if s.style.bold && alert.is_some() {
                        alert.and_then(|a| b.alert[a as usize].as_ref())
                    } else {
                        None
                    };
                    if let Some(brush) = brush {
                        let _ = l.SetDrawingEffect(brush, r);
                    }
                }
            }
            // Las marcas, al final: encogidas hasta no ocupar nada.
            for s in spans.iter().filter(|s| s.style.hidden) {
                if let Some(r) = clip(s) {
                    let _ = l.SetFontSize(0.01, r);
                }
            }
        }
    }

    /// Layout de una línea en Previsualización (sin `b`, solo con las medidas: para el
    /// hit-test del ratón). `rendered = false` es la línea en crudo del modo "En línea".
    unsafe fn md_line_layout(&self, text: &str, info: Option<&Line>, rendered: bool, g: &Geom, width: f32, b: Option<&Brushes>, code: &[crate::syntax::Span]) -> Option<IDWriteTextLayout> {
        if text.is_empty() {
            return None;
        }
        unsafe {
            let w16 = wide(text);
            let l = self.dwrite.CreateTextLayout(&w16, &self.fonts.mono_13, width.max(40.0), 20_000.0).ok()?;
            let _ = l.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
            let whole = DWRITE_TEXT_RANGE { startPosition: 0, length: w16.len() as u32 };
            if let (true, Some(info)) = (rendered, info) {
                let base = layout::FONT_MONO * self.font_scale;
                if !info.is_code_box() {
                    let ui = wide0(&self.ui_family);
                    let _ = l.SetFontFamilyName(windows::core::PCWSTR(ui.as_ptr()), whole);
                    let _ = l.SetFontSize(base * PROSE, whole);
                    if let BlockKind::Heading(n) = info.block {
                        let _ = l.SetFontWeight(DWRITE_FONT_WEIGHT_SEMI_BOLD, whole);
                        let _ = l.SetFontSize(base * PROSE * crate::markdown::heading_scale(n), whole);
                    }
                    if let Some(b) = b {
                        if info.quote > 0 && info.alert.is_none() {
                            if let Some(br) = &b.text_2 {
                                let _ = l.SetDrawingEffect(br, whole);
                            }
                        }
                    }
                }
                if let Some(b) = b {
                    for s in code {
                        let Some(br) = b.syntax.get(s.highlight).and_then(Option::as_ref) else { continue };
                        let _ = l.SetDrawingEffect(br, DWRITE_TEXT_RANGE { startPosition: s.start, length: s.len });
                    }
                }
                self.md_apply_spans(&l, text, &info.spans, 0, text.chars().count(), b, info.alert);
            }
            let lh = self.line_height() * g.scale;
            let _ = l.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, lh, lh * 0.8);
            Some(l)
        }
    }

    fn md_row_h(&self, l: Option<&IDWriteTextLayout>, info: Option<&Line>, rendered: bool, g: &Geom) -> f32 {
        if rendered && info.is_some_and(|i| i.block == BlockKind::Hidden) {
            return 0.0;
        }
        let rows = l
            .and_then(|l| {
                let mut m: DWRITE_TEXT_METRICS = Default::default();
                unsafe { l.GetMetrics(&mut m) }.ok().map(|_| m.lineCount.max(1))
            })
            .unwrap_or(1);
        rows as f32 * self.line_height() * g.scale + g.pad_top + g.pad_bottom
    }

    fn md_table_row_h(&self) -> f32 {
        self.line_height() + 2.0 * CELL_PAD_Y
    }

    /// Layout de una celda de tabla (`cell` en chars de `text`).
    unsafe fn md_cell_layout(&self, text: &str, info: &Line, cell: (usize, usize), header: bool, b: Option<&Brushes>) -> Option<IDWriteTextLayout> {
        let sub: String = text.chars().skip(cell.0).take(cell.1 - cell.0).collect();
        if sub.is_empty() {
            return None;
        }
        unsafe {
            let w16 = wide(&sub);
            let l = self.dwrite.CreateTextLayout(&w16, &self.fonts.mono_13, f32::MAX, 20_000.0).ok()?;
            let whole = DWRITE_TEXT_RANGE { startPosition: 0, length: w16.len() as u32 };
            let ui = wide0(&self.ui_family);
            let _ = l.SetFontFamilyName(windows::core::PCWSTR(ui.as_ptr()), whole);
            let _ = l.SetFontSize(layout::FONT_MONO * self.font_scale * PROSE, whole);
            if header {
                let _ = l.SetFontWeight(DWRITE_FONT_WEIGHT_SEMI_BOLD, whole);
            }
            self.md_apply_spans(&l, text, &info.spans, cell.0, cell.1, b, None);
            let lh = self.line_height();
            let _ = l.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, lh, lh * 0.8);
            Some(l)
        }
    }

    fn layout_width(l: &IDWriteTextLayout) -> f32 {
        let mut m: DWRITE_TEXT_METRICS = Default::default();
        unsafe { l.GetMetrics(&mut m) }.map(|_| m.widthIncludingTrailingWhitespace).unwrap_or(0.0)
    }

    /// Ancho de cada columna de la tabla `table` (lo que ocupa su celda más ancha).
    fn md_col_widths(&self, md: &MdDoc, buf: &notty_core::Buffer, table: usize) -> Vec<f32> {
        let total = buf.len_lines();
        let mut widths = vec![0.0f32; md.tables.get(table).map(|t| t.aligns.len()).unwrap_or(0)];
        for (li, info) in md.lines.iter().enumerate() {
            let BlockKind::TableRow { table: t, row } = info.block else { continue };
            if t != table {
                continue;
            }
            let text = line_text(buf, li, total);
            for (k, &cell) in info.cells.iter().enumerate().take(widths.len()) {
                if let Some(l) = unsafe { self.md_cell_layout(&text, info, cell, row == 0, None) } {
                    widths[k] = widths[k].max(Self::layout_width(&l));
                }
            }
        }
        widths.into_iter().map(|w| w + 2.0 * CELL_PAD_X).collect()
    }

    /// Dibuja el documento en Previsualización. `head`: posición del cursor (o
    /// `usize::MAX` sin foco). En modo "En línea", la línea del cursor sale en crudo.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_md(
        &self,
        state: &EditorState,
        pal: &Palette,
        body: Rect,
        base_x: f32,
        head: usize,
        inline_mode: bool,
        search_matches: &[std::ops::Range<usize>],
        search_current: Option<usize>,
    ) {
        let md = self.md_doc(state);
        let buf = state.doc.buffer();
        let total = buf.len_lines();
        let cursor_line = state.doc.line_col().0;
        let sel = state.doc.selection();
        let sel_range = sel.range();
        let lh = self.line_height();
        let base_x = base_x + MARGIN;
        let right = body.right - MARGIN;
        let brush = |c: Rgba| unsafe { self.target.CreateSolidColorBrush(&color(c), None).ok() };
        let b = Brushes {
            text_2: brush(pal.text_2),
            dim: brush(pal.text_3),
            link: brush(pal.accent),
            alert: [Alert::Note, Alert::Tip, Alert::Important, Alert::Warning, Alert::Caution].map(|a| brush(alert_color(pal, a))),
            syntax: (0..crate::syntax::NAMES.len()).map(|h| crate::syntax::color(pal, h).and_then(brush)).collect(),
        };
        let mut y = body.top + layout::TEXT_PAD_T;

        for line in state.viewport.first_line..total {
            if y > body.bottom {
                break;
            }
            let start = buf.line_start(line);
            let full_end = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
            let text = line_text(&buf, line, total);
            let text_end = start + text.chars().count();
            let info = md.lines.get(line);
            let rendered = !(inline_mode && line == cursor_line);
            let g = self.md_geom(info, rendered, base_x);

            unsafe {
                // --- Fila de tabla: una celda por layout ---
                if let (true, Some(info @ Line { block: BlockKind::TableRow { table, row }, .. })) = (rendered, info) {
                    let key = (*table, self.font_scale.to_bits(), self.mono_family.clone());
                    let widths = md.table_widths.borrow_mut().entry(key).or_insert_with(|| self.md_col_widths(&md, &buf, *table)).clone();
                    let aligns = &md.tables[*table].aligns;
                    let row_h = self.md_table_row_h();
                    let mut x = g.x;
                    for (k, w) in widths.iter().enumerate() {
                        let cell_r = Rect::new(x, y, x + w, y + row_h);
                        if *row % 2 == 0 && *row > 0 {
                            self.fill(cell_r, pal.text_3.faded(0.08));
                        }
                        self.stroke_rect(cell_r, 1.0, border(pal));
                        if let Some(&cell) = info.cells.get(k) {
                            if let Some(l) = self.md_cell_layout(&text, info, cell, *row == 0, Some(&b)) {
                                let free = w - 2.0 * CELL_PAD_X - Self::layout_width(&l);
                                let dx = match aligns.get(k) {
                                    Some(Align::Center) => free / 2.0,
                                    Some(Align::Right) => free,
                                    _ => 0.0,
                                };
                                let (cx, cy) = (x + CELL_PAD_X + dx, y + CELL_PAD_Y);
                                let sub: String = text.chars().skip(cell.0).take(cell.1 - cell.0).collect();
                                for s in info.spans.iter().filter(|s| s.style.code && s.start >= cell.0 && s.end <= cell.1) {
                                    for (rx, ry, rw, rh) in hit_test_range_rects(&l, &sub, s.start - cell.0, s.end - cell.0, cx, cy) {
                                        self.fill_round(Rect::new(rx - 3.0, ry + 1.0, rx + rw + 3.0, ry + rh - 1.0), 4.0, pal.text_3.faded(0.18));
                                    }
                                }
                                self.draw_md_layout(&l, cx, cy, pal);
                            }
                        }
                        x += w;
                    }
                    self.md_quote_bars(info, pal, base_x, y, row_h);
                    y += row_h;
                    continue;
                }

                let l = self.md_line_layout(&text, info, rendered, &g, right - g.x, Some(&b), md.code_spans(line));
                let row_h = self.md_row_h(l.as_ref(), info, rendered, &g);
                if row_h <= 0.0 {
                    continue;
                }
                let ty = y + g.pad_top;

                // --- Fondos y adornos del bloque ---
                if let Some(info) = info {
                    if info.is_code_box() {
                        let r = Rect::new(g.block_x, y, right, y + row_h);
                        let radius = 6.0;
                        match (info.box_first, info.box_last) {
                            (true, true) => self.fill_round(r, radius, pal.chrome),
                            (true, false) => {
                                self.fill_round(r, radius, pal.chrome);
                                self.fill(Rect::new(r.left, y + row_h / 2.0, r.right, r.bottom), pal.chrome);
                            }
                            (false, true) => {
                                self.fill_round(r, radius, pal.chrome);
                                self.fill(Rect::new(r.left, r.top, r.right, y + row_h / 2.0), pal.chrome);
                            }
                            (false, false) => self.fill(r, pal.chrome),
                        }
                    }
                    self.md_quote_bars(info, pal, base_x, y, row_h);
                    if rendered {
                        match info.block {
                            BlockKind::Heading(1 | 2) => {
                                self.fill(Rect::new(g.block_x, y + row_h - 5.0, right, y + row_h - 4.0), border(pal));
                            }
                            BlockKind::Rule => {
                                let cy = y + row_h / 2.0;
                                self.fill_round(Rect::new(g.block_x, cy - 2.0, right, cy + 2.0), 1.0, border(pal));
                            }
                            BlockKind::ListItem(kind) => self.md_bullet(&text, info, kind, &g, ty, pal),
                            _ => {}
                        }
                        // Fondo del código en línea.
                        if let Some(l) = &l {
                            for s in info.spans.iter().filter(|s| s.style.code) {
                                for (rx, ry, rw, rh) in hit_test_range_rects(l, &text, s.start, s.end, g.x, ty) {
                                    self.fill_round(Rect::new(rx - 3.0, ry + 1.0, rx + rw + 3.0, ry + rh - 1.0), 4.0, pal.text_3.faded(0.18));
                                }
                            }
                        }
                    }
                }

                // --- Selección y búsqueda ---
                let highlight = |s: usize, e: usize, c: Rgba, round: bool| {
                    let (cs, ce) = (s.max(start), e.min(text_end));
                    match &l {
                        Some(l) if ce > cs => {
                            for (rx, ry, rw, rh) in hit_test_range_rects(l, &text, cs - start, ce - start, g.x, ty) {
                                let r = Rect::new(rx, ry, (rx + rw).max(rx + 2.0), ry + rh);
                                if round { self.fill_round(r, 2.0, c) } else { self.fill(r, c) }
                            }
                        }
                        _ if e > text_end => self.fill(Rect::new(g.x, ty, g.x + 6.0, ty + lh), c),
                        _ => {}
                    }
                };
                if !sel.is_empty() && sel_range.start < full_end && sel_range.end > start {
                    highlight(sel_range.start, sel_range.end, pal.accent_soft, false);
                }
                for (mi, m) in search_matches.iter().enumerate() {
                    if m.start < full_end && m.end > start {
                        highlight(m.start, m.end, if Some(mi) == search_current { pal.mark_cur } else { pal.mark }, true);
                    }
                }

                if let Some(l) = &l {
                    self.draw_md_layout(l, g.x, ty, pal);
                }

                // Cursor: solo en la línea en crudo (en "Solo lectura" no hay).
                if !rendered && head >= start && head <= text_end {
                    let (x, y_off) = l.as_ref().map(|l| hit_test_xy(l, &text, head - start, g.x)).unwrap_or((g.x, 0.0));
                    self.fill(Rect::new(x, ty + y_off, x + 1.0, ty + y_off + lh), pal.text);
                }
                y += row_h;
            }
        }
    }

    unsafe fn draw_md_layout(&self, l: &IDWriteTextLayout, x: f32, y: f32, pal: &Palette) {
        unsafe {
            self.brush.SetColor(&color(pal.text));
            self.target.DrawTextLayout(Vector2 { X: x, Y: y }, l, &self.brush, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
        }
    }

    fn md_quote_bars(&self, info: &Line, pal: &Palette, base_x: f32, y: f32, row_h: f32) {
        for k in 0..info.quote {
            let c = match info.alert {
                Some(a) if k + 1 == info.quote => alert_color(pal, a),
                _ => border(pal),
            };
            let x = base_x + k as f32 * QUOTE_W;
            self.fill(Rect::new(x, y, x + 3.0, y + row_h), c);
        }
    }

    /// Viñeta, número o casilla de un ítem de lista, a la izquierda de su texto.
    unsafe fn md_bullet(&self, text: &str, info: &Line, kind: ListMarker, g: &Geom, ty: f32, pal: &Palette) {
        let cy = ty + self.line_height() / 2.0;
        let x = g.x - 13.0;
        match kind {
            ListMarker::Bullet => match (info.indent - 1) % 3 {
                0 => self.fill_circle(x, cy, 2.6, pal.text),
                1 => self.stroke_circle(x, cy, 2.6, 1.1, pal.text),
                _ => self.fill(Rect::new(x - 2.5, cy - 2.5, x + 2.5, cy + 2.5), pal.text),
            },
            ListMarker::Ordered => {
                if let Some((s, e)) = info.number {
                    let num: String = text.chars().skip(s).take(e - s).collect();
                    self.text_right(&num, &self.fonts.ui_13, Rect::new(g.x - 44.0, ty, g.x - 5.0, ty + self.line_height()), pal.text);
                }
            }
            ListMarker::Task(done) => {
                let r = Rect::new(g.x - 20.0, cy - 6.5, g.x - 7.0, cy + 6.5);
                if done {
                    self.fill_round(r, 3.0, pal.accent);
                    self.stroke_line(r.left + 3.0, cy, r.left + 5.5, cy + 3.0, 1.6, pal.on_accent);
                    self.stroke_line(r.left + 5.5, cy + 3.0, r.right - 3.0, cy - 3.0, 1.6, pal.on_accent);
                } else {
                    self.stroke_round_rect(r, 3.0, 1.2, pal.text_3);
                }
            }
        }
    }

    /// `char_index_at` en Previsualización: mismas medidas que `paint_md`.
    pub(super) fn md_char_index_at(&self, state: &EditorState, body: Rect, base_x: f32, inline_mode: bool, x: f32, y: f32) -> usize {
        let md = self.md_doc(state);
        let buf = state.doc.buffer();
        let total = buf.len_lines();
        let cursor_line = state.doc.line_col().0;
        let base_x = base_x + MARGIN;
        let right = body.right - MARGIN;
        let mut row_top = body.top + layout::TEXT_PAD_T;
        let first = state.viewport.first_line.min(total.saturating_sub(1));
        for line in first..total {
            let start = buf.line_start(line);
            let text = line_text(&buf, line, total);
            let info = md.lines.get(line);
            let rendered = !(inline_mode && line == cursor_line);
            let g = self.md_geom(info, rendered, base_x);
            if rendered && matches!(info.map(|i| i.block), Some(BlockKind::TableRow { .. })) {
                let h = self.md_table_row_h();
                if y < row_top + h || line + 1 >= total {
                    return start;
                }
                row_top += h;
                continue;
            }
            let l = unsafe { self.md_line_layout(&text, info, rendered, &g, right - g.x, None, &[]) };
            let h = self.md_row_h(l.as_ref(), info, rendered, &g);
            if (h > 0.0 && y < row_top + h) || line + 1 >= total {
                let Some(l) = l else { return start };
                let w16 = wide(&text);
                let mut trailing = windows::core::BOOL(0);
                let mut inside = windows::core::BOOL(0);
                let mut m = Default::default();
                return if unsafe { l.HitTestPoint(x - g.x, y - row_top - g.pad_top, &mut trailing, &mut inside, &mut m) }.is_ok() {
                    let off = m.textPosition as usize + trailing.as_bool() as usize;
                    start + String::from_utf16_lossy(&w16[..off.min(w16.len())]).chars().count()
                } else {
                    start
                };
            }
            row_top += h;
        }
        buf.len_chars()
    }
}
