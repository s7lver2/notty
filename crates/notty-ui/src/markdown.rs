//! Analizador de Markdown para Previsualización (Ajustes → Archivos), al estilo de
//! GitHub (GFM): encabezados (# y subrayados ===/---), párrafos, listas anidadas y de
//! tareas, citas (anidadas y avisos `> [!NOTE]`), reglas, bloques de código (``` ~~~ e
//! indentados, con resaltado por lenguaje), tablas, y en línea negrita/cursiva/tachado/
//! código/enlaces/imágenes/autoenlaces/escapes/etiquetas HTML.
//!
//! La idea central: el texto que se dibuja nunca cambia respecto al markdown de verdad.
//! Todo son rangos (en chars de la línea) de estilo u ocultación que el renderizador
//! aplica sobre el mismo `IDWriteTextLayout`, así que cursor, selección y búsqueda (que
//! calculan posiciones sobre ese texto) siguen funcionando igual.

use crate::syntax::{self, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMarker {
    Bullet,
    Ordered,
    /// Casilla `- [ ]` / `- [x]`.
    Task(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Blank,
    Paragraph,
    /// 1..=6.
    Heading(u8),
    ListItem(ListMarker),
    Rule,
    /// Línea ``` que abre un bloque de código (se oculta).
    FenceOpen,
    /// Línea ``` que lo cierra (se oculta).
    FenceClose,
    Code,
    /// Fila de tabla (`row` 0 = cabecera).
    TableRow { table: usize, row: usize },
    /// No se dibuja (fila `|---|` de una tabla, subrayado `===` de un encabezado,
    /// comentario HTML, línea en blanco repetida…): alto 0.
    Hidden,
}

/// Aviso de GitHub (`> [!NOTE]`…).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alert {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    /// Código en línea (`así`): letra monoespaciada con fondo.
    pub code: bool,
    pub link: bool,
    /// Texto alternativo de una imagen (no se cargan imágenes: se ve el alt atenuado).
    pub dim: bool,
    /// Marca de sintaxis ("**", "# ", "](url)"…): se reduce a ancho ~0.
    pub hidden: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyledRange {
    /// Desplazamientos en chars dentro de la línea.
    pub start: usize,
    pub end: usize,
    pub style: InlineStyle,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub block: BlockKind,
    /// Profundidad de cita (`>`), 0 fuera de citas.
    pub quote: u8,
    pub alert: Option<Alert>,
    /// Sangría de lista en niveles: un `ListItem` de nivel `n` tiene `indent = n + 1`
    /// (su viñeta se dibuja en el nivel `n`); una línea de continuación dentro de la
    /// lista, el número de listas en las que está metida.
    pub indent: u8,
    /// Rango en chars del número de una lista ordenada ("12."), que se dibuja aparte.
    pub number: Option<(usize, usize)>,
    pub spans: Vec<StyledRange>,
    /// Filas de tabla: rango en chars del contenido de cada celda.
    pub cells: Vec<(usize, usize)>,
    /// Líneas de un bloque ```lang: índice en `MdDoc::blocks` (colores con `MdDoc::code_spans`).
    pub code_block: Option<u32>,
    /// Primera/última fila de la caja de un bloque de código.
    pub box_first: bool,
    pub box_last: bool,
}

impl Line {
    fn new(block: BlockKind) -> Self {
        Line {
            block,
            quote: 0,
            alert: None,
            indent: 0,
            number: None,
            spans: Vec::new(),
            cells: Vec::new(),
            code_block: None,
            box_first: false,
            box_last: false,
        }
    }

    /// Parte de la caja gris de un bloque de código.
    pub fn is_code_box(&self) -> bool {
        matches!(self.block, BlockKind::FenceOpen | BlockKind::FenceClose | BlockKind::Code)
    }
}

pub struct Table {
    pub aligns: Vec<Align>,
}

/// Un bloque ```lang: se colorea la primera vez que se pinta (con muchos bloques,
/// colorearlos todos al abrir el archivo era lo que más tardaba).
struct CodeBlock {
    first: usize,
    lang: syntax::Lang,
    /// Sus líneas sin la sangría de la valla, unidas con saltos de línea.
    text: String,
    /// Cuánto se quitó (en UTF-16) al principio de cada línea.
    shifts: Vec<u32>,
    spans: std::cell::OnceCell<Vec<Vec<Span>>>,
}

pub struct MdDoc {
    pub lines: Vec<Line>,
    pub tables: Vec<Table>,
    blocks: Vec<CodeBlock>,
    /// Anchos de columna ya medidos por el renderizador, por (tabla, zoom, fuente):
    /// medirlos recorre todas las filas de la tabla, no solo las que se ven.
    pub table_widths: std::cell::RefCell<std::collections::HashMap<(usize, u32, String), Vec<f32>>>,
}

impl MdDoc {
    /// Colores de sintaxis de la línea `line` (en UTF-16 desde su inicio), si es código.
    pub fn code_spans(&self, line: usize) -> &[Span] {
        let Some(b) = self.lines.get(line).and_then(|l| l.code_block).and_then(|id| self.blocks.get(id as usize)) else {
            return &[];
        };
        let spans = b.spans.get_or_init(|| {
            syntax::highlight_snippet(b.lang, &b.text)
                .into_iter()
                .zip(&b.shifts)
                .map(|(s, &shift)| s.into_iter().map(|s| Span { start: s.start + shift, ..s }).collect())
                .collect()
        });
        spans.get(line - b.first).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Cuánto más grande se dibuja un encabezado respecto al texto normal (como GitHub:
/// 2em, 1.5em, 1.25em, 1em, .875em, .85em).
pub fn heading_scale(level: u8) -> f32 {
    match level {
        1 => 2.0,
        2 => 1.5,
        3 => 1.25,
        4 => 1.0,
        5 => 0.875,
        _ => 0.85,
    }
}

fn hide(out: &mut Vec<StyledRange>, start: usize, end: usize) {
    if end > start {
        out.push(StyledRange { start, end, style: InlineStyle { hidden: true, ..Default::default() } });
    }
}

fn styled(out: &mut Vec<StyledRange>, start: usize, end: usize, style: InlineStyle) {
    if end > start {
        out.push(StyledRange { start, end, style });
    }
}

fn leading_ws(chars: &[char], from: usize) -> usize {
    chars[from.min(chars.len())..].iter().take_while(|c| **c == ' ' || **c == '\t').count()
}

fn is_blank(chars: &[char], from: usize) -> bool {
    chars[from.min(chars.len())..].iter().all(|c| c.is_whitespace())
}

/// `---`, `***`, `___` (con espacios entre medias permitidos).
fn is_rule(chars: &[char], from: usize) -> bool {
    let ws = leading_ws(chars, from);
    if ws > 3 {
        return false;
    }
    let rest = &chars[from + ws..];
    let Some(&c) = rest.first() else { return false };
    matches!(c, '-' | '*' | '_')
        && rest.iter().filter(|&&x| x == c).count() >= 3
        && rest.iter().all(|&x| x == c || x == ' ' || x == '\t')
}

/// Subrayado de encabezado: `===` (nivel 1) o `---` (nivel 2).
fn setext_level(chars: &[char]) -> Option<u8> {
    let ws = leading_ws(chars, 0);
    if ws > 3 {
        return None;
    }
    let rest: Vec<char> = chars[ws..].iter().copied().collect();
    let body: Vec<char> = {
        let mut r = rest.clone();
        while r.last().is_some_and(|c| c.is_whitespace()) {
            r.pop();
        }
        r
    };
    let c = *body.first()?;
    if !body.iter().all(|&x| x == c) {
        return None;
    }
    match c {
        '=' => Some(1),
        '-' => Some(2),
        _ => None,
    }
}

/// Apertura de bloque de código: (carácter, longitud, etiqueta de lenguaje).
fn fence_open(chars: &[char], from: usize) -> Option<(char, usize, String)> {
    let ws = leading_ws(chars, from);
    let rest = &chars[from + ws..];
    let c = *rest.first()?;
    if c != '`' && c != '~' {
        return None;
    }
    let run = rest.iter().take_while(|&&x| x == c).count();
    if run < 3 {
        return None;
    }
    let info: String = rest[run..].iter().collect();
    if c == '`' && info.contains('`') {
        return None;
    }
    Some((c, run, info.split_whitespace().next().unwrap_or("").to_string()))
}

fn fence_close(chars: &[char], from: usize, c: char, len: usize) -> bool {
    let ws = leading_ws(chars, from);
    let rest = &chars[from + ws..];
    let run = rest.iter().take_while(|&&x| x == c).count();
    run >= len && rest[run..].iter().all(|x| x.is_whitespace())
}

/// Profundidad de cita y char donde empieza el contenido.
fn quote_prefix(chars: &[char]) -> (u8, usize) {
    let mut depth = 0u8;
    let mut i = 0;
    loop {
        let ws = leading_ws(chars, i);
        if ws <= 3 && chars.get(i + ws) == Some(&'>') {
            depth += 1;
            i += ws + 1;
            if chars.get(i) == Some(&' ') {
                i += 1;
            }
        } else {
            return (depth, i);
        }
    }
}

/// Marca de lista en `from` (tras la sangría): (tipo, fin de la marca, inicio del contenido).
fn list_marker(chars: &[char], from: usize) -> Option<(ListMarker, usize, usize)> {
    let c = *chars.get(from)?;
    let marker_end = if matches!(c, '-' | '*' | '+') {
        from + 1
    } else if c.is_ascii_digit() {
        let d = chars[from..].iter().take_while(|c| c.is_ascii_digit()).count();
        if d > 9 || !matches!(chars.get(from + d), Some('.') | Some(')')) {
            return None;
        }
        from + d + 1
    } else {
        return None;
    };
    // Detrás de la marca: espacio o fin de línea.
    let after = chars.get(marker_end);
    if after.is_some_and(|c| *c != ' ' && *c != '\t') {
        return None;
    }
    let content = (marker_end + leading_ws(chars, marker_end).clamp(1, 4)).min(chars.len());
    let mut kind = if c.is_ascii_digit() { ListMarker::Ordered } else { ListMarker::Bullet };
    let mut content_start = content;
    if kind == ListMarker::Bullet && chars.len() >= content + 3 && chars[content] == '[' && chars[content + 2] == ']' {
        let mark = chars[content + 1];
        let after_box = chars.get(content + 3);
        if matches!(mark, ' ' | 'x' | 'X') && after_box.is_none_or(|c| *c == ' ') {
            kind = ListMarker::Task(mark != ' ');
            content_start = (content + 4).min(chars.len());
        }
    }
    Some((kind, marker_end, content_start))
}

/// Separa una fila de tabla en celdas (rangos en chars, ya sin espacios alrededor).
fn split_cells(chars: &[char], from: usize) -> Vec<(usize, usize)> {
    let mut start = from + leading_ws(chars, from);
    let mut end = chars.len();
    while end > start && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    if chars.get(start) == Some(&'|') {
        start += 1;
    }
    if end > start && chars[end - 1] == '|' && (end < 2 || chars[end - 2] != '\\') {
        end -= 1;
    }
    let mut cells = Vec::new();
    let mut cell_start = start;
    let mut in_code = false;
    let mut i = start;
    while i < end {
        match chars[i] {
            '\\' => i += 1,
            '`' => in_code = !in_code,
            '|' if !in_code => {
                cells.push((cell_start, i));
                cell_start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    cells.push((cell_start, end));
    cells
        .into_iter()
        .map(|(mut s, mut e)| {
            while s < e && chars[s].is_whitespace() {
                s += 1;
            }
            while e > s && chars[e - 1].is_whitespace() {
                e -= 1;
            }
            (s, e)
        })
        .collect()
}

/// `|---|:--:|--:|` → alineaciones.
fn delim_row(chars: &[char], from: usize) -> Option<Vec<Align>> {
    if !chars[from.min(chars.len())..].contains(&'-') {
        return None;
    }
    split_cells(chars, from)
        .into_iter()
        .map(|(s, e)| {
            let cell = &chars[s..e];
            let left = cell.first() == Some(&':');
            let right = cell.last() == Some(&':');
            let inner = &cell[left as usize..cell.len() - (right && cell.len() > 1) as usize];
            (!inner.is_empty() && inner.iter().all(|&c| c == '-')).then_some(match (left, right) {
                (true, true) => Align::Center,
                (false, true) => Align::Right,
                _ => Align::Left,
            })
        })
        .collect()
}

/// Analiza el documento entero (una entrada por línea, ya sin `\r`/`\n`).
pub fn analyze(texts: &[String]) -> MdDoc {
    let docs: Vec<Vec<char>> = texts.iter().map(|t| t.chars().collect()).collect();
    let n = docs.len();
    let mut lines: Vec<Line> = Vec::with_capacity(n);
    let mut tables: Vec<Table> = Vec::new();
    let mut blocks: Vec<CodeBlock> = Vec::new();
    // (carácter, longitud, lenguaje, primera línea de código, sangría a quitar, niveles de lista)
    let mut fence: Option<(char, usize, String, usize, usize, u8)> = None;
    // (sangría de la marca, columna del contenido) por cada lista abierta.
    let mut list: Vec<(usize, usize)> = Vec::new();
    let mut in_comment = false;
    // (tabla, siguiente fila)
    let mut table: Option<(usize, usize)> = None;
    let mut alert: Option<Alert> = None;

    let mut i = 0;
    while i < n {
        let chars = &docs[i];

        // --- Dentro de un bloque de código ---
        if let Some((fc, fl, ref lang, first, strip, indent)) = fence {
            if fence_close(chars, 0, fc, fl) {
                let mut l = Line::new(BlockKind::FenceClose);
                l.indent = indent;
                hide(&mut l.spans, 0, chars.len());
                lines.push(l);
                highlight_block(&docs, &mut lines, &mut blocks, first, i, lang, strip);
                fence = None;
            } else {
                let mut l = Line::new(BlockKind::Code);
                l.indent = indent;
                hide(&mut l.spans, 0, leading_ws(chars, 0).min(strip));
                lines.push(l);
            }
            i += 1;
            continue;
        }

        // --- Comentario HTML de varias líneas ---
        let text: String = chars.iter().collect();
        if in_comment || text.trim_start().starts_with("<!--") {
            in_comment = !text.contains("-->");
            let mut l = Line::new(BlockKind::Hidden);
            hide(&mut l.spans, 0, chars.len());
            lines.push(l);
            i += 1;
            continue;
        }

        let (quote, off) = quote_prefix(chars);
        if quote == 0 {
            alert = None;
        }

        // --- Tabla en curso ---
        if let Some((t, row)) = table {
            if !is_blank(chars, off) && chars[off..].contains(&'|') {
                let mut l = Line::new(BlockKind::TableRow { table: t, row });
                l.quote = quote;
                l.cells = split_cells(chars, off);
                for &(s, e) in &l.cells {
                    inline(chars, s, e, &mut l.spans);
                }
                lines.push(l);
                table = Some((t, row + 1));
                i += 1;
                continue;
            }
            table = None;
        }

        // --- Línea en blanco ---
        if is_blank(chars, off) {
            let repeated = lines.last().is_some_and(|l| matches!(l.block, BlockKind::Blank | BlockKind::Hidden) && l.quote == quote);
            let mut l = Line::new(if repeated && quote == 0 { BlockKind::Hidden } else { BlockKind::Blank });
            l.quote = quote;
            l.alert = alert;
            lines.push(l);
            i += 1;
            continue;
        }

        let ws = leading_ws(chars, off);
        let prev_blank = lines.last().is_none_or(|l| matches!(l.block, BlockKind::Blank | BlockKind::Hidden));
        let prev_in_list = lines.last().is_some_and(|l| l.indent > 0 && !prev_blank);

        // --- Bloque de código indentado (4 espacios, fuera de listas) ---
        if quote == 0 && list.is_empty() && ws >= 4 {
            let prev_code = lines.last().is_some_and(|l| l.block == BlockKind::Code);
            if prev_blank || prev_code {
                let mut l = Line::new(BlockKind::Code);
                hide(&mut l.spans, 0, 4);
                lines.push(l);
                i += 1;
                continue;
            }
        }

        // Niveles de lista en los que cae esta línea (se ajusta más abajo si es un ítem).
        let marker = if is_rule(chars, off) { None } else { list_marker(chars, off + ws) };
        if marker.is_none() && !(prev_in_list && lines.last().is_some_and(|l| matches!(l.block, BlockKind::Paragraph | BlockKind::ListItem(_)))) {
            while list.last().is_some_and(|&(_, col)| ws < col) {
                list.pop();
            }
        }

        // --- Apertura de bloque de código ---
        if let Some((c, len, lang)) = fence_open(chars, off) {
            let indent = list.len() as u8;
            let mut l = Line::new(BlockKind::FenceOpen);
            l.quote = quote;
            l.indent = indent;
            hide(&mut l.spans, 0, chars.len());
            lines.push(l);
            fence = Some((c, len, lang, i + 1, off + ws, indent));
            i += 1;
            continue;
        }

        // --- Tabla (cabecera + fila |---|) ---
        if chars[off..].contains(&'|') {
            if let Some(aligns) = docs.get(i + 1).and_then(|next| delim_row(next, quote_prefix(next).1)) {
                let cells = split_cells(chars, off);
                if aligns.len() == cells.len() {
                    let t = tables.len();
                    tables.push(Table { aligns });
                    let mut l = Line::new(BlockKind::TableRow { table: t, row: 0 });
                    l.quote = quote;
                    for &(s, e) in &cells {
                        inline(chars, s, e, &mut l.spans);
                    }
                    l.cells = cells;
                    lines.push(l);
                    let mut d = Line::new(BlockKind::Hidden);
                    d.quote = quote;
                    hide(&mut d.spans, 0, docs[i + 1].len());
                    lines.push(d);
                    table = Some((t, 1));
                    i += 2;
                    continue;
                }
            }
        }

        let mut l = Line::new(BlockKind::Paragraph);
        l.quote = quote;
        hide(&mut l.spans, 0, off + ws);

        // Aviso de GitHub en la primera línea de una cita.
        if quote > 0 && lines.last().is_none_or(|p| p.quote == 0) {
            let rest: String = chars[off + ws..].iter().collect::<String>().trim_end().to_ascii_uppercase();
            alert = match rest.as_str() {
                "[!NOTE]" => Some(Alert::Note),
                "[!TIP]" => Some(Alert::Tip),
                "[!IMPORTANT]" => Some(Alert::Important),
                "[!WARNING]" => Some(Alert::Warning),
                "[!CAUTION]" => Some(Alert::Caution),
                _ => None,
            };
            if alert.is_some() {
                let s = off + ws;
                let e = s + rest.chars().count();
                l.alert = alert;
                hide(&mut l.spans, s, s + 2);
                styled(&mut l.spans, s + 2, e - 1, InlineStyle { bold: true, ..Default::default() });
                hide(&mut l.spans, e - 1, chars.len());
                lines.push(l);
                i += 1;
                continue;
            }
        }
        l.alert = alert;

        if is_rule(chars, off) {
            // "texto\n---" es un encabezado, no una regla.
            let setext = quote == 0 && setext_level(chars) == Some(2) && lines.last().is_some_and(|p| p.block == BlockKind::Paragraph && p.quote == 0 && p.indent == 0);
            if !setext {
                list.clear();
                l.block = BlockKind::Rule;
                l.spans.clear();
                hide(&mut l.spans, 0, chars.len());
                lines.push(l);
                i += 1;
                continue;
            }
        }

        // --- Encabezado subrayado (el párrafo de arriba pasa a encabezado) ---
        if quote == 0 && ws <= 3 && list.is_empty() {
            if let Some(level) = setext_level(chars) {
                if let Some(prev) = lines.last_mut().filter(|p| p.block == BlockKind::Paragraph && p.quote == 0 && p.indent == 0) {
                    prev.block = BlockKind::Heading(level);
                    l.block = BlockKind::Hidden;
                    l.spans.clear();
                    hide(&mut l.spans, 0, chars.len());
                    lines.push(l);
                    i += 1;
                    continue;
                }
            }
        }

        // --- Encabezado # ---
        let hashes = chars[off + ws..].iter().take_while(|&&c| c == '#').count();
        if ws <= 3 && (1..=6).contains(&hashes) && chars.get(off + ws + hashes).is_none_or(|c| *c == ' ' || *c == '\t') {
            list.clear();
            let content = (off + ws + hashes + 1).min(chars.len());
            hide(&mut l.spans, off + ws, content);
            // Cierre opcional: "## Título ##".
            let mut end = chars.len();
            while end > content && chars[end - 1].is_whitespace() {
                end -= 1;
            }
            let closing = chars[content..end].iter().rev().take_while(|&&c| c == '#').count();
            if closing > 0 && (end - closing == content || chars[end - closing - 1] == ' ') {
                let e = end - closing;
                hide(&mut l.spans, e.saturating_sub(1).max(content), chars.len());
                end = e;
            }
            inline(chars, content, end, &mut l.spans);
            l.block = BlockKind::Heading(hashes as u8);
            lines.push(l);
            i += 1;
            continue;
        }

        // --- Ítem de lista ---
        if let Some((kind, marker_end, content)) = marker {
            let marker_col = ws;
            let content_col = content - off;
            // Metido hasta el contenido del ítem de arriba → sublista; si no, se cierran listas.
            while list.last().is_some_and(|&(_, col)| marker_col < col) {
                list.pop();
            }
            let level = list.len() as u8;
            list.push((marker_col, content_col));
            l.block = BlockKind::ListItem(kind);
            l.indent = level + 1;
            if kind == ListMarker::Ordered {
                l.number = Some((off + ws, marker_end));
            }
            hide(&mut l.spans, off + ws, content);
            inline(chars, content, chars.len(), &mut l.spans);
            lines.push(l);
            i += 1;
            continue;
        }

        // --- Párrafo (o continuación de un ítem de lista) ---
        l.indent = list.len() as u8;
        inline(chars, off + ws, chars.len(), &mut l.spans);
        // Solo etiquetas HTML (`<p align="center">`, `<br>`, `<img …>`): no se ve nada.
        let visible = (off + ws..chars.len()).any(|c| !chars[c].is_whitespace() && !l.spans.iter().any(|s| s.style.hidden && s.start <= c && c < s.end));
        if !visible {
            l.block = BlockKind::Hidden;
        }
        lines.push(l);
        i += 1;
    }

    if let Some((_, _, ref lang, first, strip, _)) = fence {
        highlight_block(&docs, &mut lines, &mut blocks, first, n, lang, strip);
    }

    // Esquinas redondeadas de las cajas de código.
    let boxy: Vec<bool> = lines.iter().map(Line::is_code_box).collect();
    for k in 0..lines.len() {
        if !boxy[k] {
            continue;
        }
        let b = lines[k].block;
        let prev = k.checked_sub(1).map(|p| (boxy[p], lines[p].block));
        let next = lines.get(k + 1).map(|l| (boxy[k + 1], l.block));
        lines[k].box_first = b == BlockKind::FenceOpen || prev.is_none_or(|(bx, pb)| !bx || pb == BlockKind::FenceClose);
        lines[k].box_last = b == BlockKind::FenceClose || next.is_none_or(|(bx, nb)| !bx || nb == BlockKind::FenceOpen);
    }

    MdDoc { lines, tables, blocks, table_widths: Default::default() }
}

/// Apunta las líneas `first..end` (un bloque ```lang) para colorearlas cuando se vean.
fn highlight_block(docs: &[Vec<char>], lines: &mut [Line], blocks: &mut Vec<CodeBlock>, first: usize, end: usize, lang: &str, strip: usize) {
    let Some(lang) = syntax::lang_for_fence(lang) else { return };
    if first >= end {
        return;
    }
    let stripped: Vec<(usize, String)> = (first..end)
        .map(|k| {
            let cut = leading_ws(&docs[k], 0).min(strip);
            (docs[k][..cut].iter().map(|c| c.len_utf16()).sum::<usize>(), docs[k][cut..].iter().collect())
        })
        .collect();
    let text = stripped.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join("\n");
    let id = blocks.len() as u32;
    for line in &mut lines[first..end] {
        line.code_block = Some(id);
    }
    let shifts = stripped.iter().map(|(s, _)| *s as u32).collect();
    blocks.push(CodeBlock { first, lang, text, shifts, spans: Default::default() });
}

/// Busca el cierre de un delimitador de énfasis de longitud exacta `len`.
fn find_closer(chars: &[char], from: usize, to: usize, d: char, len: usize) -> Option<usize> {
    let mut j = from;
    while j < to {
        let c = chars[j];
        if c == '\\' {
            j += 2;
            continue;
        }
        if c == '`' {
            let run = chars[j..to].iter().take_while(|&&x| x == '`').count();
            j = code_close(chars, j + run, to, run).map(|e| e + run).unwrap_or(j + run);
            continue;
        }
        if c == d {
            let run = chars[j..to].iter().take_while(|&&x| x == d).count();
            let after_ok = d != '_' || chars.get(j + run).is_none_or(|x| !x.is_alphanumeric());
            if run == len && j > from && !chars[j - 1].is_whitespace() && after_ok {
                return Some(j);
            }
            j += run;
            continue;
        }
        j += 1;
    }
    None
}

/// Cierre de un código en línea de `run` backticks.
fn code_close(chars: &[char], from: usize, to: usize, run: usize) -> Option<usize> {
    let mut j = from;
    while j < to {
        if chars[j] == '`' {
            let r = chars[j..to].iter().take_while(|&&x| x == '`').count();
            if r == run {
                return Some(j);
            }
            j += r;
        } else {
            j += 1;
        }
    }
    None
}

/// Busca el `]` que cierra el `[` en `open`, y el `(...)` que va justo detrás:
/// (fin del texto, cierre del paréntesis).
fn link_end(chars: &[char], open: usize, to: usize) -> Option<(usize, usize)> {
    let mut depth = 0;
    let mut j = open;
    let text_end = loop {
        if j >= to {
            return None;
        }
        match chars[j] {
            '\\' => j += 1,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break j;
                }
            }
            _ => {}
        }
        j += 1;
    };
    if chars.get(text_end + 1) != Some(&'(') {
        return None;
    }
    let mut depth = 0;
    let mut k = text_end + 1;
    while k < to {
        match chars[k] {
            '\\' => k += 1,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((text_end, k));
                }
            }
            _ => {}
        }
        k += 1;
    }
    None
}

/// Etiqueta HTML en `from` (`<b>`, `</div>`, `<img src="…">`, `<br/>`): su fin.
fn html_tag_end(chars: &[char], from: usize, to: usize) -> Option<usize> {
    let mut j = from + 1;
    if chars.get(j) == Some(&'/') {
        j += 1;
    }
    if !chars.get(j).is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut quote: Option<char> = None;
    while j < to {
        let c = chars[j];
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '>' => return Some(j + 1),
            None if c == '<' => return None,
            None => {}
        }
        j += 1;
    }
    None
}

fn starts_with(chars: &[char], at: usize, s: &str) -> bool {
    let mut k = at;
    for c in s.chars() {
        if chars.get(k) != Some(&c) {
            return false;
        }
        k += 1;
    }
    true
}

/// Marcas en línea de `chars[from..to]`, en `out`.
fn inline(chars: &[char], from: usize, to: usize, out: &mut Vec<StyledRange>) {
    let mut i = from;
    while i < to {
        let c = chars[i];
        match c {
            '\\' if i + 1 < to && chars[i + 1].is_ascii_punctuation() => {
                hide(out, i, i + 1);
                i += 2;
                continue;
            }
            '`' => {
                let run = chars[i..to].iter().take_while(|&&x| x == '`').count();
                if let Some(close) = code_close(chars, i + run, to, run) {
                    hide(out, i, i + run);
                    styled(out, i + run, close, InlineStyle { code: true, ..Default::default() });
                    hide(out, close, close + run);
                    i = close + run;
                } else {
                    i += run;
                }
                continue;
            }
            '!' if chars.get(i + 1) == Some(&'[') => {
                if let Some((text_end, close)) = link_end(chars, i + 1, to) {
                    hide(out, i, i + 2);
                    styled(out, i + 2, text_end, InlineStyle { dim: true, italic: true, ..Default::default() });
                    hide(out, text_end, close + 1);
                    i = close + 1;
                    continue;
                }
            }
            '[' => {
                if let Some((text_end, close)) = link_end(chars, i, to) {
                    hide(out, i, i + 1);
                    styled(out, i + 1, text_end, InlineStyle { link: true, ..Default::default() });
                    inline(chars, i + 1, text_end, out);
                    hide(out, text_end, close + 1);
                    i = close + 1;
                    continue;
                }
            }
            '<' => {
                if starts_with(chars, i, "<!--") {
                    let end = (i + 4..to.saturating_sub(2)).find(|&k| starts_with(chars, k, "-->")).map(|k| k + 3).unwrap_or(to);
                    hide(out, i, end);
                    i = end;
                    continue;
                }
                if starts_with(chars, i, "<http://") || starts_with(chars, i, "<https://") || starts_with(chars, i, "<mailto:") {
                    if let Some(close) = (i + 1..to).find(|&k| chars[k] == '>') {
                        hide(out, i, i + 1);
                        styled(out, i + 1, close, InlineStyle { link: true, ..Default::default() });
                        hide(out, close, close + 1);
                        i = close + 1;
                        continue;
                    }
                }
                if let Some(end) = html_tag_end(chars, i, to) {
                    hide(out, i, end);
                    i = end;
                    continue;
                }
            }
            'h' if (starts_with(chars, i, "http://") || starts_with(chars, i, "https://"))
                && (i == from || chars[i - 1].is_whitespace() || chars[i - 1] == '(') =>
            {
                let mut end = (i..to).find(|&k| chars[k].is_whitespace() || chars[k] == '<').unwrap_or(to);
                while end > i && matches!(chars[end - 1], '.' | ',' | ';' | ':' | '!' | '?' | ')' | '\'' | '"') {
                    end -= 1;
                }
                styled(out, i, end, InlineStyle { link: true, ..Default::default() });
                i = end;
                continue;
            }
            '~' if chars.get(i + 1) == Some(&'~') => {
                if let Some(close) = find_closer(chars, i + 2, to, '~', 2) {
                    hide(out, i, i + 2);
                    styled(out, i + 2, close, InlineStyle { strike: true, ..Default::default() });
                    inline(chars, i + 2, close, out);
                    hide(out, close, close + 2);
                    i = close + 2;
                    continue;
                }
                i += 2;
                continue;
            }
            '*' | '_' => {
                let run = chars[i..to].iter().take_while(|&&x| x == c).count();
                let len = run.min(3);
                let opens = chars.get(i + run).is_some_and(|x| !x.is_whitespace())
                    && (c != '_' || i == 0 || !chars[i - 1].is_alphanumeric());
                if opens {
                    // Con 3 marcas, si no hay cierre de 3 prueba negrita (2) o cursiva (1).
                    let found = (1..=len).rev().find_map(|l| find_closer(chars, i + run, to, c, l).map(|close| (l, close)));
                    if let Some((l, close)) = found {
                        let open_start = i + run - l;
                        hide(out, open_start, i + run);
                        let style = InlineStyle { bold: l >= 2, italic: l != 2, ..Default::default() };
                        styled(out, i + run, close, style);
                        inline(chars, i + run, close, out);
                        hide(out, close, close + l);
                        i = close + l;
                        continue;
                    }
                }
                i += run;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(src: &str) -> MdDoc {
        let lines: Vec<String> = src.split('\n').map(str::to_string).collect();
        analyze(&lines)
    }

    fn visible(text: &str, l: &Line) -> String {
        text.chars()
            .enumerate()
            .filter(|(k, _)| !l.spans.iter().any(|s| s.style.hidden && s.start <= *k && *k < s.end))
            .map(|(_, c)| c)
            .collect()
    }

    #[test]
    fn headings_and_setext() {
        let d = doc("# Uno\nDos\n===\nTres\n---");
        assert_eq!(d.lines[0].block, BlockKind::Heading(1));
        assert_eq!(visible("# Uno", &d.lines[0]), "Uno");
        assert_eq!(d.lines[1].block, BlockKind::Heading(1));
        assert_eq!(d.lines[2].block, BlockKind::Hidden);
        assert_eq!(d.lines[3].block, BlockKind::Heading(2));
    }

    #[test]
    fn rule_after_blank_is_a_rule() {
        let d = doc("texto\n\n---");
        assert_eq!(d.lines[2].block, BlockKind::Rule);
    }

    #[test]
    fn fenced_code_hides_fences() {
        let d = doc("```rust\nfn main() {}\n```");
        assert_eq!(d.lines[0].block, BlockKind::FenceOpen);
        assert_eq!(d.lines[1].block, BlockKind::Code);
        assert_eq!(d.lines[2].block, BlockKind::FenceClose);
        assert!(d.lines[0].box_first && d.lines[2].box_last);
        assert!(!d.code_spans(1).is_empty(), "resaltado de rust");
    }

    #[test]
    fn quotes_nest_and_hide_markers() {
        let d = doc("> uno\n> > dos");
        assert_eq!(d.lines[0].quote, 1);
        assert_eq!(d.lines[1].quote, 2);
        assert_eq!(visible("> > dos", &d.lines[1]), "dos");
    }

    #[test]
    fn github_alert() {
        let d = doc("> [!WARNING]\n> cuidado");
        assert_eq!(d.lines[0].alert, Some(Alert::Warning));
        assert_eq!(d.lines[1].alert, Some(Alert::Warning));
        assert_eq!(visible("> [!WARNING]", &d.lines[0]), "WARNING");
    }

    #[test]
    fn nested_lists_and_tasks() {
        let d = doc("- a\n  - b\n    - [x] c\n- d\n1. e");
        assert_eq!(d.lines[0].indent, 1);
        assert_eq!(d.lines[1].indent, 2);
        assert_eq!(d.lines[2].block, BlockKind::ListItem(ListMarker::Task(true)));
        assert_eq!(d.lines[2].indent, 3);
        assert_eq!(visible("    - [x] c", &d.lines[2]), "c");
        assert_eq!(d.lines[3].indent, 1);
        assert_eq!(d.lines[4].block, BlockKind::ListItem(ListMarker::Ordered));
    }

    #[test]
    fn table_rows_and_alignment() {
        let d = doc("| a | b |\n|:--|--:|\n| 1 | 2 |");
        assert_eq!(d.lines[0].block, BlockKind::TableRow { table: 0, row: 0 });
        assert_eq!(d.lines[1].block, BlockKind::Hidden);
        assert_eq!(d.lines[2].block, BlockKind::TableRow { table: 0, row: 1 });
        assert_eq!(d.tables[0].aligns, vec![Align::Left, Align::Right]);
        assert_eq!(d.lines[2].cells.len(), 2);
    }

    #[test]
    fn inline_styles() {
        let t = "**negrita** *cursiva* ~~no~~ `code` [link](http://x) ![alt](i.png) snake_case_name";
        let d = doc(t);
        assert_eq!(visible(t, &d.lines[0]), "negrita cursiva no code link alt snake_case_name");
        let s = &d.lines[0].spans;
        assert!(s.iter().any(|r| r.style.bold));
        assert!(s.iter().any(|r| r.style.italic));
        assert!(s.iter().any(|r| r.style.strike));
        assert!(s.iter().any(|r| r.style.code));
        assert!(s.iter().any(|r| r.style.link));
        assert!(s.iter().any(|r| r.style.dim));
    }

    #[test]
    fn html_only_lines_are_hidden() {
        let d = doc("<p align=\"center\">\ntexto <b>ya</b>");
        assert_eq!(d.lines[0].block, BlockKind::Hidden);
        assert_eq!(visible("texto <b>ya</b>", &d.lines[1]), "texto ya");
    }

    #[test]
    fn unclosed_marker_stays_literal() {
        let d = doc("esto **no cierra");
        assert!(d.lines[0].spans.iter().all(|r| !r.style.bold));
    }
}
