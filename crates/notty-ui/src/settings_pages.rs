//! Contenido de cada página de Ajustes (maqueta `Prototipo.dc.html`, dirección A):
//! vistas previas vivas arriba, filas debajo. Todo se dibuja con los controles de
//! `settings_ui` y devuelve hasta dónde llegó (para el scroll).

use std::time::Instant;

use notty_config::{Config, Files, HotkeyMechanism, MenuBar, Preset, TabsPosition};
use notty_input::Command;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows_numerics::{Matrix3x2, Vector2};

use crate::anim::Curve;
use crate::layout::Rect;
use crate::settings_model::{self as model, LinkAction, Page, SettingKey};
use crate::settings_ui::{Hit, InputId, TK, Ui, icon, key_hash};
use crate::text_input::TextInput;
use crate::theme::Rgba;
use crate::UpdatePhase;

/// Lo que Ajustes sabe del actualizador (lo rellena la ventana principal).
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateInfo {
    pub phase: UpdatePhase,
    pub new_version: Option<String>,
    pub notes: String,
    pub release_url: Option<String>,
    /// La última comprobación de esta sesión dijo "al día".
    pub up_to_date: bool,
}

impl Default for UpdateInfo {
    fn default() -> Self {
        Self { phase: UpdatePhase::Idle, new_version: None, notes: String::new(), release_url: None, up_to_date: false }
    }
}

pub(crate) struct Inputs {
    pub sample: TextInput,
    pub search: TextInput,
    pub seq: TextInput,
    pub glyph: TextInput,
}

impl Default for Inputs {
    fn default() -> Self {
        Self {
            sample: TextInput::new("fn main() -> Ok(0O 1lI) != {}", 80),
            search: TextInput::new("", 40),
            seq: TextInput::new("", 6),
            glyph: TextInput::new("", 2),
        }
    }
}

impl Inputs {
    pub fn get_mut(&mut self, id: InputId) -> &mut TextInput {
        match id {
            InputId::Sample => &mut self.sample,
            InputId::Search => &mut self.search,
            InputId::LigSeq => &mut self.seq,
            InputId::LigGlyph => &mut self.glyph,
        }
    }
}

/// Código de la vista previa de Apariencia, resaltado con el mismo tree-sitter que
/// el editor.
pub(crate) struct Preview {
    pub doc: notty_core::Document,
    pub syntax: crate::syntax::SyntaxCache,
    /// La muestra de cada lenguaje de `syntax::LANGS` (subpágina Sintaxis), con su
    /// propio árbol para no reparsear al cambiar de uno a otro.
    pub langs: Vec<(notty_core::Document, crate::syntax::SyntaxCache)>,
}

pub(crate) const PREVIEW_CODE: &str = "fn main() -> Result<(), Error> {\n    let path = args().nth(1)?;\n    if path != \"\" { open(&path)?; }\n    // abre lo que te pasen por argumento\n}";

impl Default for Preview {
    fn default() -> Self {
        Self {
            doc: notty_core::Document::new(PREVIEW_CODE, "\n"),
            syntax: Default::default(),
            langs: crate::syntax::LANGS.iter().map(|l| (notty_core::Document::new(l.sample, "\n"), Default::default())).collect(),
        }
    }
}

pub(crate) struct PageData<'a> {
    pub cfg: &'a Config,
    pub page: Page,
    pub enter: Instant,
    pub fonts: &'a [String],
    pub inputs: &'a Inputs,
    pub focus: Option<InputId>,
    pub caret_on: bool,
    pub update: &'a UpdateInfo,
    pub preview: &'a Preview,
    pub logo: Option<&'a ID2D1Bitmap>,
    pub system_dark: bool,
    /// Lenguaje de la vista previa de Sintaxis (índice en `syntax::LANGS`) y cuándo se eligió.
    pub syn_pick: usize,
    pub syn_pick_at: Instant,
}

/// Fuentes que traen ligaduras de programación propias (etiqueta "Ligaduras").
const LIGATURE_FONTS: &[&str] =
    &["Cascadia Code", "Fira Code", "JetBrains Mono", "Iosevka", "Victor Mono", "Hasklig", "Monoid", "Recursive Mono"];
/// Monoespaciadas que vienen con Windows (etiqueta "De Windows").
const WINDOWS_FONTS: &[&str] = &[
    "Cascadia Code",
    "Cascadia Mono",
    "Consolas",
    "Courier New",
    "Lucida Console",
    "Lucida Sans Typewriter",
    "MS Gothic",
    "NSimSun",
    "SimSun-ExtB",
    "SimSun-ExtG",
];

/// Filtra `fonts` por el buscador.
pub(crate) fn filtered_fonts<'f>(fonts: &'f [String], query: &str) -> Vec<&'f String> {
    let q = query.trim().to_lowercase();
    fonts.iter().filter(|f| q.is_empty() || f.to_lowercase().contains(&q)).collect()
}

/// Si la página tiene algo que se mueve siempre (cursor que parpadea, demo en
/// bucle...): la ventana mantiene el temporizador mientras se ve.
pub(crate) fn has_continuous_anim(d: &PageData) -> bool {
    match d.page {
        Page::Apariencia | Page::Archivos | Page::AtajoGlobal | Page::AcercaDe | Page::Ayuda => true,
        Page::Sintaxis => d.syn_pick_at.elapsed().as_millis() < 300,
        Page::Actualizaciones => matches!(d.update.phase, UpdatePhase::Checking | UpdatePhase::Downloading(..) | UpdatePhase::Found),
        _ => false,
    }
}

const MAX_W: f32 = 720.0;
const PAD_X: f32 = 36.0;

/// Dibuja la página `d.page` dentro de `area` (ya desplazada por el scroll) y
/// devuelve el alto de su contenido.
pub(crate) fn draw_page(ui: &mut Ui, d: &PageData, area: Rect) -> f32 {
    let x = area.left + PAD_X;
    let w = (area.width() - PAD_X * 2.0).clamp(200.0, MAX_W);
    let top = area.top + 22.0;
    let sub = d.page.parent().is_some();
    if sub {
        ui.begin_enter(d.enter, 0, 300, 24.0, 0.0);
    }
    let bottom = match d.page {
        Page::Apariencia => apariencia(ui, d, x, top, w),
        Page::Fuentes => fuentes(ui, d, x, top, w),
        Page::Ligaduras => ligaduras(ui, d, x, top, w),
        Page::Sintaxis => sintaxis(ui, d, x, top, w),
        Page::Ventana => ventana(ui, d, x, top, w),
        Page::Teclado => teclado(ui, d, x, top, w),
        Page::Archivos => archivos(ui, d, x, top, w),
        Page::AtajoGlobal => atajo(ui, d, x, top, w),
        Page::Actualizaciones => actualizaciones(ui, d, x, top, w),
        Page::AcercaDe => acerca(ui, d, x, top, w),
        Page::Ayuda => ayuda(ui, d, x, top, w),
    };
    if sub {
        ui.end_enter();
    }
    bottom + 28.0 - area.top
}

// --- Piezas comunes ---------------------------------------------------------------

/// Bloque `i` de una página (`.stg>*`): entra escalonado 40 ms. En las subpáginas
/// ya entra la página entera desde la derecha, así que no se escalona.
fn blk(ui: &mut Ui, d: &PageData, i: usize) {
    if d.page.parent().is_some() {
        ui.begin_enter(d.enter, 0, 1, 0.0, 0.0);
    } else {
        ui.begin_enter(d.enter, i.min(10) as u64 * 40, 340, 0.0, 10.0);
    }
}

fn blk_end(ui: &mut Ui) {
    ui.end_enter();
}

/// Elemento `j` de una lista (`.lst>*`): 30 ms entre uno y otro.
fn item(ui: &mut Ui, d: &PageData, j: usize) {
    ui.begin_enter(d.enter, j.min(10) as u64 * 30, 300, 0.0, 10.0);
}

fn header(ui: &mut Ui, x: f32, y: f32, w: f32, title: &str, hint: &str) -> f32 {
    ui.text(title, 20.0, true, Rect::new(x, y, x + w, y + 28.0), ui.pal.text);
    if !hint.is_empty() {
        let hw = ui.measure(hint, 12.0, false);
        let tw = ui.measure(title, 20.0, true);
        if tw + hw + 24.0 < w {
            ui.text(hint, 12.0, false, Rect::new(x + w - hw, y + 6.0, x + w, y + 26.0), ui.pal.text_3);
        }
    }
    y + 28.0
}

/// Cabecera de subpágina: ‹ Apariencia › Fuentes.
fn crumb_header(ui: &mut Ui, d: &PageData, x: f32, y: f32, w: f32) -> f32 {
    let parent = d.page.parent().unwrap_or(Page::Apariencia);
    let back = Rect::new(x, y, x + 28.0, y + 28.0);
    let h = ui.hover_spring(Hit::Back, 250);
    ui.r.fill_round(back, 6.0, ui.pal.hover.faded(h.clamp(0.0, 1.0)));
    let c = ui.pal.text_2.mix(ui.pal.text, h.clamp(0.0, 1.0));
    ui.icon(icon::ATRAS, x + 14.0 - 3.0 * h, y + 14.0, 16.0, 1.8, c);
    ui.hit(back, Hit::Back);
    let cx = x + 34.0;
    let cw = ui.measure(parent.name(), 20.0, true);
    let ch = ui.hover_t(Hit::Crumb, 150);
    let crumb = Rect::new(cx, y, cx + cw, y + 28.0);
    ui.text(parent.name(), 20.0, true, crumb, ui.pal.text_2.mix(ui.pal.text, ch));
    ui.hit(crumb, Hit::Crumb);
    ui.icon(icon::CHEVRON, crumb.right + 13.0, y + 14.0, 14.0, 2.0, ui.pal.text_3);
    ui.text(d.page.name(), 20.0, true, Rect::new(crumb.right + 26.0, y, x + w, y + 28.0), ui.pal.text);
    y + 28.0
}

fn label(ui: &mut Ui, x: f32, y: f32, w: f32, s: &str) -> f32 {
    ui.text(s, 12.0, false, Rect::new(x, y, x + w, y + 16.0), ui.pal.text_2);
    y + 16.0
}

/// `.grp`: cabecera de grupo en versalitas.
fn group(ui: &mut Ui, x: f32, y: f32, w: f32, s: &str) -> f32 {
    let y = y + 12.0;
    ui.text(&s.to_uppercase(), 11.0, false, Rect::new(x + 2.0, y, x + w, y + 16.0), ui.pal.text_3);
    y + 18.0
}

const DESC_LH: f32 = 16.0;

fn desc_h(ui: &Ui, w: f32, desc: &str) -> f32 {
    if desc.is_empty() { 0.0 } else { 2.0 + ui.r.measure_wrapped(desc, &ui.font(12.0, false), w, Some(DESC_LH)).max(DESC_LH) }
}

fn title_desc(ui: &Ui, x: f32, y: f32, w: f32, title: &str, desc: &str) {
    ui.text(title, 13.0, false, Rect::new(x, y, x + w, y + 17.0), ui.pal.text);
    if !desc.is_empty() {
        let dh = desc_h(ui, w, desc) - 2.0;
        ui.r.text_wrapped(desc, &ui.font(12.0, false), Rect::new(x, y + 19.0, x + w, y + 19.0 + dh), ui.pal.text_2, Some(DESC_LH));
    }
}

struct RowGeo {
    rect: Rect,
    ic: Rect,
    text_x: f32,
    text_w: f32,
    ctrl: Rect,
    hover: f32,
}

/// `.row`/`.lnk`: fondo, icono opcional a la izquierda, texto, y hueco a la derecha
/// de `ctrl_w`×`ctrl_h` para el control (que dibuja quien llama).
#[allow(clippy::too_many_arguments)]
fn row(ui: &mut Ui, x: f32, y: f32, w: f32, hit: Hit, ic: f32, desc: &str, ctrl_w: f32, ctrl_h: f32) -> RowGeo {
    let text_x = x + 14.0 + if ic > 0.0 { ic + 12.0 } else { 0.0 };
    let text_w = (x + w - 14.0 - if ctrl_w > 0.0 { ctrl_w + 12.0 } else { 0.0 } - text_x).max(40.0);
    let text_h = 17.0 + desc_h(ui, text_w, desc);
    let h = (text_h + 24.0).max(ic + 24.0).max(ctrl_h + 24.0);
    let rect = Rect::new(x, y, x + w, y + h);
    let hover = ui.row_bg(rect, hit, 6.0);
    if hit != Hit::None {
        ui.hit(rect, hit);
    }
    let cy = y + h / 2.0;
    RowGeo {
        rect,
        ic: Rect::new(x + 14.0, cy - ic / 2.0, x + 14.0 + ic, cy + ic / 2.0),
        text_x,
        text_w,
        ctrl: Rect::new(x + w - 14.0 - ctrl_w, cy - ctrl_h / 2.0, x + w - 14.0, cy + ctrl_h / 2.0),
        hover,
    }
}

fn text_y(g: &RowGeo, desc: &str, ui: &Ui) -> f32 {
    let th = 17.0 + desc_h(ui, g.text_w, desc);
    g.rect.top + (g.rect.height() - th) / 2.0
}

fn toggle_row(ui: &mut Ui, x: f32, y: f32, w: f32, cfg: &Config, key: SettingKey, title: &str, desc: &str) -> f32 {
    let hit = Hit::Toggle(key);
    let g = row(ui, x, y, w, hit, 0.0, desc, 40.0, 22.0);
    title_desc(ui, g.text_x, text_y(&g, desc, ui), g.text_w, title, desc);
    ui.toggle(g.ctrl, hit, model::current_bool(cfg, key));
    g.rect.bottom
}

/// Fila con un `.seg` a la derecha; si no cabe junto al título, baja debajo.
fn seg_row(ui: &mut Ui, x: f32, y: f32, w: f32, cfg: &Config, key: SettingKey, title: &str) -> f32 {
    let opts = model::options_for(key);
    let labels: Vec<&str> = opts.iter().map(|(l, _)| *l).collect();
    let sel = model::selected_index(cfg, key, opts);
    let nat = ui.seg_natural_w(&labels);
    let title_w = ui.measure(title, 13.0, false);
    if title_w + nat + 40.0 <= w {
        let g = row(ui, x, y, w, Hit::None, 0.0, "", nat, 30.0);
        title_desc(ui, g.text_x, text_y(&g, "", ui), g.text_w, title, "");
        ui.seg(g.ctrl, key, &labels, sel);
        g.rect.bottom
    } else {
        let h = 12.0 + 17.0 + 8.0 + 30.0 + 12.0;
        let rect = Rect::new(x, y, x + w, y + h);
        ui.row_bg(rect, Hit::None, 6.0);
        title_desc(ui, x + 14.0, y + 12.0, w - 28.0, title, "");
        ui.seg(Rect::new(x + 14.0, y + 37.0, x + w - 14.0, y + 67.0), key, &labels, sel);
        rect.bottom
    }
}

/// Fila de enlace (`.lnk`) con chevron o icono externo.
#[allow(clippy::too_many_arguments)]
fn link_row(ui: &mut Ui, x: f32, y: f32, w: f32, hit: Hit, title: &str, desc: &str, mono_desc: bool, d_icon: &str) -> f32 {
    let press = ui.press_t(hit, 150);
    let c = Vector2 { X: x + w / 2.0, Y: y + 25.0 };
    ui.push_xf(Matrix3x2::scale_around(1.0 - 0.01 * press, 1.0 - 0.01 * press, c));
    let g = row(ui, x, y, w, hit, 0.0, if mono_desc { "" } else { desc }, 16.0, 16.0);
    let ty = if mono_desc { g.rect.top + (g.rect.height() - 35.0) / 2.0 } else { text_y(&g, desc, ui) };
    if mono_desc {
        ui.text(title, 13.0, false, Rect::new(g.text_x, ty, g.text_x + g.text_w, ty + 17.0), ui.pal.text);
        let f = ui.mono(ui.r.mono_family(), 12.0);
        ui.r.text(desc, &f, Rect::new(g.text_x, ty + 19.0, g.text_x + g.text_w, ty + 35.0), ui.pal.text_2);
    } else {
        title_desc(ui, g.text_x, ty, g.text_w, title, desc);
    }
    ui.chevron(g.ctrl.left + 8.0, g.ctrl.top + 8.0, hit, d_icon);
    ui.pop_xf();
    // El texto mono de una línea no cuenta en `row`: se garantiza el alto mínimo.
    if mono_desc { g.rect.bottom.max(y + 59.0) } else { g.rect.bottom }
}

fn bullets_dot(ui: &Ui, x: f32, y: f32, s: &str, w: f32, c: Rgba) -> f32 {
    ui.text("•", 12.0, false, Rect::new(x, y, x + 10.0, y + 18.0), c);
    let h = ui.r.measure_wrapped(s, &ui.font(12.0, false), w - 12.0, Some(18.0)).max(18.0);
    ui.r.text_wrapped(s, &ui.font(12.0, false), Rect::new(x + 12.0, y, x + w, y + h), c, Some(18.0));
    h
}

fn millis(d: &PageData, now: Instant) -> f32 {
    now.saturating_duration_since(d.enter).as_secs_f32() * 1000.0
}

// --- Apariencia ---------------------------------------------------------------------

fn apariencia(ui: &mut Ui, d: &PageData, x: f32, top: f32, w: f32) -> f32 {
    let cfg = d.cfg;
    blk(ui, d, 0);
    let custom = if cfg.ui.preset == Preset::Custom { "Personalizado · los cambios se ven al instante" } else { "Los cambios se ven al instante" };
    let mut y = header(ui, x, top, w, "Apariencia", custom) + 16.0;
    blk_end(ui);

    blk(ui, d, 1);
    y = code_preview(ui, d, x, y, w) + 16.0;
    blk_end(ui);

    blk(ui, d, 2);
    let lw = label(ui, x, y, w, "Preset");
    let note = "Tocar cualquier otra opción lo pasa a Personalizado";
    let nw = ui.measure(note, 11.0, false);
    if nw + 80.0 < w {
        ui.text(note, 11.0, false, Rect::new(x + w - nw, y, x + w, y + 16.0), ui.pal.text_3);
    }
    y = lw + 8.0;
    let sel = model::selected_index(cfg, SettingKey::Preset, model::PRESET_OPTS);
    y = cards(ui, x, y, w, SettingKey::Preset, sel, |ui, j, r, hover| preset_thumb(ui, j, r, hover)) + 16.0;
    blk_end(ui);

    blk(ui, d, 3);
    y = label(ui, x, y, w, "Tema") + 8.0;
    let sel = model::selected_index(cfg, SettingKey::Theme, model::THEME_OPTS);
    let sys = d.system_dark;
    y = cards(ui, x, y, w, SettingKey::Theme, sel, |ui, j, r, hover| theme_thumb(ui, j, r, hover, sys)) + 16.0;
    blk_end(ui);

    blk(ui, d, 4);
    let px = model::font_px(cfg);
    let family = cfg.ui.font_family.primary_name();
    // Fuente
    item(ui, d, 0);
    let hit = Hit::Go(Page::Fuentes);
    let desc = format!("{family} · {px} px");
    let g = row(ui, x, y, w, hit, 34.0, &desc, 16.0, 16.0);
    let (cx, cy) = ui.ic_begin(g.ic, g.hover, ui.pal.surface);
    let f = ui.mono(family, 15.0);
    ui.r.text_center("Aa", &f, Rect::new(cx - 17.0, cy - 17.0, cx + 17.0, cy + 17.0), ui.pal.text);
    ui.pop_xf();
    title_desc(ui, g.text_x, text_y(&g, &desc, ui), g.text_w, "Fuente", &desc);
    ui.chevron(g.ctrl.left + 8.0, g.ctrl.top + 8.0, hit, icon::CHEVRON);
    y = g.rect.bottom + 4.0;
    ui.end_enter();

    // Ligaduras
    item(ui, d, 1);
    let entries = crate::ligature::entries(&cfg.ligature_overrides, &cfg.ligature_disabled);
    let on_entries: Vec<&crate::ligature::Entry> = entries.iter().filter(|e| e.enabled).collect();
    let desc = if cfg.ui.ligatures {
        let glyphs: String = on_entries.iter().map(|e| e.glyph.to_string()).collect::<Vec<_>>().join(" ");
        format!("{} activas · {glyphs}", on_entries.len())
    } else {
        "Desactivadas".to_string()
    };
    let hit = Hit::Toggle(SettingKey::Ligatures);
    let editar_w = ui.measure("Editar", 12.0, false);
    let g = row(ui, x, y, w, hit, 34.0, &desc, 40.0 + 14.0 + editar_w, 22.0);
    let lig_on = cfg.ui.ligatures;
    let (cx, cy) = ui.ic_begin(g.ic, g.hover, ui.pal.surface);
    let t = ui.tween(TK::Preview(10), lig_on as u8 as f32, 350, Curve::Spring);
    let fm = ui.mono(ui.r.mono_family(), 15.0);
    let ic_c = ui.pal.text_2.mix(ui.pal.accent, t.clamp(0.0, 1.0));
    swap_text(ui, "->", "→", t, cx, cy, &fm, ic_c);
    ui.pop_xf();
    title_desc(ui, g.text_x, text_y(&g, &desc, ui), g.text_w, "Ligaduras", &desc);
    ui.more(g.ctrl.left, g.ctrl.top + 11.0, "Editar", Hit::Go(Page::Ligaduras));
    ui.toggle(Rect::new(g.ctrl.right - 40.0, g.ctrl.top, g.ctrl.right, g.ctrl.bottom), hit, lig_on);
    y = g.rect.bottom + 4.0;
    ui.end_enter();

    // Resaltado de sintaxis
    item(ui, d, 2);
    let key = SettingKey::SyntaxHighlight;
    let hit = Hit::Toggle(key);
    let total = crate::syntax::LANGS.len();
    let active = crate::syntax::LANGS.iter().filter(|l| !cfg.syntax_disabled.iter().any(|d| d == l.id)).count();
    let desc_s = if cfg.ui.syntax_highlight { format!("{active} de {total} lenguajes, según la extensión del archivo") } else { "Desactivado".to_string() };
    let desc = desc_s.as_str();
    let editar_w = ui.measure("Editar", 12.0, false);
    let g = row(ui, x, y, w, hit, 34.0, desc, 40.0 + 14.0 + editar_w, 22.0);
    let on = cfg.ui.syntax_highlight;
    let (cx, cy) = ui.ic_begin(g.ic, g.hover, ui.pal.surface);
    let bars = [(16.0, ui.pal.syn_keyword), (11.0, ui.pal.syn_string), (14.0, ui.pal.syn_function)];
    for (i, (bh, c)) in bars.iter().enumerate() {
        let target = if on { *bh } else { 4.0 };
        let hh = ui.tw.to_delayed(TK::Preview(20 + i as u8), target, 350, 50 * i as u64, Curve::Spring, ui.now);
        let bx = cx - 8.5 + i as f32 * 7.0;
        ui.r.fill_round(Rect::new(bx, cy - hh / 2.0, bx + 5.0, cy + hh / 2.0), 2.0, *c);
    }
    ui.pop_xf();
    title_desc(ui, g.text_x, text_y(&g, desc, ui), g.text_w, "Resaltado de sintaxis", desc);
    ui.more(g.ctrl.left, g.ctrl.top + 11.0, "Editar", Hit::Go(Page::Sintaxis));
    ui.toggle(Rect::new(g.ctrl.right - 40.0, g.ctrl.top, g.ctrl.right, g.ctrl.bottom), hit, on);
    y = g.rect.bottom + 4.0;
    ui.end_enter();

    // Números de línea
    item(ui, d, 3);
    let key = SettingKey::LineNumbers;
    let hit = Hit::Toggle(key);
    let g = row(ui, x, y, w, hit, 34.0, "", 40.0, 22.0);
    let on = cfg.ui.line_numbers;
    let (cx, cy) = ui.ic_begin(g.ic, g.hover, ui.pal.surface);
    let t = ui.tween(TK::Preview(30), on as u8 as f32, 300, Curve::Out);
    let fm = ui.mono(ui.r.mono_family(), 10.0);
    let c = ui.track_c().mix(ui.pal.text_2, t);
    for (i, n) in ["1", "2", "3"].iter().enumerate() {
        let ly = cy - 15.0 + i as f32 * 10.0;
        ui.r.text_right(n, &fm, Rect::new(cx - 10.0, ly, cx + 4.0, ly + 10.0), c);
    }
    ui.pop_xf();
    title_desc(ui, g.text_x, text_y(&g, "", ui), g.text_w, "Números de línea", "");
    ui.toggle(g.ctrl, hit, on);
    y = g.rect.bottom;
    ui.end_enter();
    blk_end(ui);
    y
}

/// `.swapI`/`.lg`: `a` se va encogiendo mientras `b` aparece (`t` 0→1).
#[allow(clippy::too_many_arguments)]
fn swap_text(
    ui: &mut Ui,
    a: &str,
    b: &str,
    t: f32,
    cx: f32,
    cy: f32,
    f: &windows::Win32::Graphics::DirectWrite::IDWriteTextFormat,
    c: Rgba,
) {
    let box_ = Rect::new(cx - 30.0, cy - 12.0, cx + 30.0, cy + 12.0);
    let center = Vector2 { X: cx, Y: cy };
    let ta = t.clamp(0.0, 1.0);
    if ta < 0.99 {
        let s = 1.0 - 0.4 * t;
        ui.push_xf(Matrix3x2::scale_around(s, s, center));
        ui.push_fade(1.0 - ta);
        ui.r.text_center(a, f, box_, c);
        ui.pop_fade();
        ui.pop_xf();
    }
    if ta > 0.01 {
        let s = 0.6 + 0.4 * t;
        ui.push_xf(Matrix3x2::scale_around(s, s, center));
        ui.push_fade(ta);
        ui.r.text_center(b, f, box_, c);
        ui.pop_fade();
        ui.pop_xf();
    }
}

/// Tarjetas de un selector (preset, tema...): el mismo ancho cada una, 10 de hueco.
fn cards(
    ui: &mut Ui,
    x: f32,
    y: f32,
    w: f32,
    key: SettingKey,
    sel: Option<usize>,
    mut thumb: impl FnMut(&mut Ui, usize, Rect, f32),
) -> f32 {
    let opts = model::options_for(key);
    let n = opts.len() as f32;
    let cw = (w - 10.0 * (n - 1.0)) / n;
    let h = 8.0 + 50.0 + 8.0 + 16.0 + 8.0;
    for (j, (name, _)) in opts.iter().enumerate() {
        let hit = Hit::Choice(key, j as u8);
        let cx = x + (cw + 10.0) * j as f32;
        let r = Rect::new(cx, y, cx + cw, y + h);
        let selected = sel == Some(j);
        ui.card_begin(r, hit, selected);
        let hover = ui.hover_t(hit, 350);
        let tr = Rect::new(r.left + 8.0, r.top + 8.0, r.right - 8.0, r.top + 58.0);
        ui.r.push_clip(tr);
        let c = Vector2 { X: tr.left + tr.width() / 2.0, Y: tr.top + tr.height() / 2.0 };
        let s = 1.0 + 0.04 * hover;
        ui.push_xf(Matrix3x2::scale_around(s, s, c));
        thumb(ui, j, tr, hover);
        ui.pop_xf();
        ui.r.pop_clip();
        ui.text(name, 12.0, false, Rect::new(r.left + 10.0, tr.bottom + 8.0, r.right - 8.0, tr.bottom + 24.0), ui.pal.text);
        ui.card_end(r, hit, selected);
    }
    y + h
}

fn bar(ui: &Ui, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
    ui.r.fill_round(Rect::new(x, y, x + w, y + h), h / 2.0, c);
}

fn preset_thumb(ui: &mut Ui, j: usize, r: Rect, _hover: f32) {
    let preset = [Preset::Moderna, Preset::Clasica, Preset::Zen][j.min(2)];
    let mut p = notty_config::UiConfig::default();
    notty_config::apply_preset(&mut p, preset);
    let pal = ui.pal;
    ui.r.fill_round(r, 5.0, pal.cmd);
    let mut y = r.top;
    // Barra de título, con pestañas si van ahí.
    ui.r.fill(Rect::new(r.left, y, r.right, y + 9.0), pal.chrome);
    if p.tabs_position == TabsPosition::Title {
        ui.r.fill(Rect::new(r.left + 4.0, y + 3.0, r.left + 26.0, y + 9.0), pal.accent);
        ui.r.fill(Rect::new(r.left + 28.0, y + 3.0, r.left + 50.0, y + 9.0), ui.track_c());
    }
    y += 9.0;
    if p.menubar == MenuBar::Visible {
        ui.r.fill(Rect::new(r.left, y, r.right, y + 8.0), pal.chrome);
        bar(ui, r.left + 5.0, y + 3.0, 12.0, 2.5, pal.text_3);
        bar(ui, r.left + 20.0, y + 3.0, 12.0, 2.5, pal.text_3);
        y += 8.0;
    }
    if p.tabs_position == TabsPosition::Below {
        ui.r.fill(Rect::new(r.left + 4.0, y + 2.0, r.left + 26.0, y + 8.0), pal.accent);
        ui.r.fill(Rect::new(r.left + 28.0, y + 2.0, r.left + 50.0, y + 8.0), ui.track_c());
        y += 8.0;
    }
    let lx = if p.line_numbers { r.left + 12.0 } else { r.left + 7.0 };
    if p.line_numbers {
        bar(ui, r.left + 4.0, y + 5.0, 4.0, 3.0, pal.text_3);
        bar(ui, r.left + 4.0, y + 11.0, 4.0, 3.0, pal.text_3);
    }
    bar(ui, lx, y + 5.0, (r.width() * 0.5).max(10.0), 3.0, ui.track_c());
    bar(ui, lx, y + 11.0, (r.width() * 0.65).max(10.0), 3.0, ui.track_c());
    let bottom_h = if p.hints_bar && !p.merged_command_line { 8.0 } else { 5.0 };
    let bc = if p.merged_command_line { pal.accent.faded(0.5) } else { pal.chrome };
    ui.r.fill(Rect::new(r.left, r.bottom - bottom_h, r.right, r.bottom), bc);
}

fn theme_thumb(ui: &mut Ui, j: usize, r: Rect, _hover: f32, _system_dark: bool) {
    let draw = |ui: &mut Ui, p: &crate::theme::Palette, r: Rect| {
        ui.r.fill(r, p.surface);
        ui.r.fill(Rect::new(r.left, r.top, r.right, r.top + 10.0), p.chrome);
        bar(ui, r.left + 8.0, r.top + 16.0, r.width() * 0.6, 4.0, p.line.mix(p.text_2, 0.2));
        bar(ui, r.left + 8.0, r.top + 24.0, r.width() * 0.4, 4.0, p.accent);
    };
    match j {
        0 => {
            // Sistema: mitad oscura, mitad clara.
            let mid = r.left + r.width() / 2.0;
            ui.r.push_clip(Rect::new(r.left, r.top, mid, r.bottom));
            draw(ui, &crate::theme::DARK, r);
            ui.r.pop_clip();
            ui.r.push_clip(Rect::new(mid, r.top, r.right, r.bottom));
            draw(ui, &crate::theme::LIGHT, r);
            ui.r.pop_clip();
        }
        1 => draw(ui, &crate::theme::LIGHT, r),
        _ => draw(ui, &crate::theme::DARK, r),
    }
}

/// Tarjeta de código: se redibuja con la fuente, tamaño, resaltado, ligaduras y
/// números de línea actuales, con transición en cada cambio.
fn code_preview(ui: &mut Ui, d: &PageData, x: f32, y: f32, w: f32) -> f32 {
    let cfg = d.cfg;
    let family = cfg.ui.font_family.primary_name();
    let px_target = model::font_px(cfg) as f32;
    let px = (ui.tween(TK::Preview(1), px_target, 200, Curve::Out) * 2.0).round() / 2.0;
    let lh = px * 1.6;
    let lines: Vec<&str> = PREVIEW_CODE.lines().collect();
    let h = 28.0 + lh * lines.len() as f32;
    let card = Rect::new(x, y, x + w, y + h);
    ui.r.fill_round(card, 8.0, ui.pal.chrome);
    ui.r.fill_round(Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0), 7.0, ui.pal.cmd);

    let f = ui.mono(family, px);
    let cw = ui.r.measure("0", &f).max(1.0);
    let gut = ui.tween(TK::Preview(2), cfg.ui.line_numbers as u8 as f32, 350, Curve::Out);
    let syn = ui.tween(TK::Preview(3), cfg.ui.syntax_highlight as u8 as f32, 350, Curve::Linear);
    let code_x = card.left + 16.0 + (38.0 + 14.0) * gut;
    let spans = d.preview.syntax.line_spans(&d.preview.doc, Some(std::path::Path::new("vista.rs")), 0..lines.len(), &cfg.syntax_disabled);
    let entries = crate::ligature::entries(&cfg.ligature_overrides, &cfg.ligature_disabled);
    let mut seqs: Vec<&crate::ligature::Entry> = entries.iter().collect();
    seqs.sort_by_key(|e| std::cmp::Reverse(e.seq.chars().count()));
    ui.r.push_clip(Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0));
    for (li, line) in lines.iter().enumerate() {
        let ly = card.top + 14.0 + lh * li as f32;
        if gut > 0.01 {
            ui.push_fade(gut);
            let gr = Rect::new(card.left, ly, card.left + 38.0 * gut, ly + lh);
            ui.r.text_right(&(li + 1).to_string(), &f, gr, ui.pal.text_3);
            ui.pop_fade();
        }
        let chars: Vec<char> = line.chars().collect();
        let mut colors: Vec<Rgba> = vec![ui.pal.text; chars.len()];
        if let Some(sp) = spans.get(li) {
            for s in sp {
                if let Some(c) = crate::syntax::color(ui.pal, s.highlight) {
                    for col in s.start as usize..(s.start + s.len) as usize {
                        if let Some(slot) = colors.get_mut(col) {
                            *slot = ui.pal.text.mix(c, syn);
                        }
                    }
                }
            }
        }
        // Ligaduras: se sacan del texto base y se dibujan aparte para morfarlas.
        let mut lig_at: Vec<(usize, &crate::ligature::Entry)> = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let found = seqs.iter().find(|e| {
                let n = e.seq.chars().count();
                n > 0 && i + n <= chars.len() && chars[i..i + n].iter().copied().eq(e.seq.chars())
            });
            match found {
                Some(e) => {
                    lig_at.push((i, e));
                    i += e.seq.chars().count();
                }
                None => i += 1,
            }
        }
        let mut hidden = vec![false; chars.len()];
        for (at, e) in &lig_at {
            for k in *at..*at + e.seq.chars().count() {
                hidden[k] = true;
            }
        }
        // Tramos del mismo color.
        let mut start = 0;
        while start < chars.len() {
            let mut end = start + 1;
            while end < chars.len() && colors[end] == colors[start] && hidden[end] == hidden[start] {
                end += 1;
            }
            if !hidden[start] {
                let s: String = chars[start..end].iter().collect();
                if !s.trim().is_empty() {
                    let rx = code_x + cw * start as f32;
                    ui.r.text(&s, &f, Rect::new(rx, ly, rx + cw * (end - start) as f32 + cw, ly + lh), colors[start]);
                }
            }
            start = end;
        }
        for (at, e) in &lig_at {
            let on = cfg.ui.ligatures && e.enabled;
            let t = ui.tween(TK::Morph(key_hash(&e.seq)), on as u8 as f32, 360, Curve::Spring);
            let a = t.clamp(0.0, 1.0);
            let n = e.seq.chars().count() as f32;
            let rx = code_x + cw * *at as f32;
            let cell = Rect::new(rx, ly, rx + cw * n, ly + lh);
            let center = Vector2 { X: rx + cw * n / 2.0, Y: ly + lh / 2.0 };
            if a < 0.99 {
                let s = 1.0 - 0.3 * t;
                ui.push_xf(Matrix3x2::scale_around(s, s, center) * Matrix3x2::translation(0.0, 7.0 * t));
                ui.push_fade(1.0 - a);
                ui.r.text(&e.seq, &f, Rect::new(cell.left, cell.top, cell.right + cw, cell.bottom), colors.get(*at).copied().unwrap_or(ui.pal.text));
                ui.pop_fade();
                ui.pop_xf();
            }
            if a > 0.01 {
                let s = 0.7 + 0.3 * t;
                let gc = Vector2 { X: rx + cw / 2.0, Y: center.Y };
                ui.push_xf(Matrix3x2::scale_around(s, s, gc) * Matrix3x2::translation(0.0, -7.0 * (1.0 - t)));
                ui.push_fade(a);
                ui.r.text(&e.glyph.to_string(), &f, Rect::new(rx, cell.top, rx + cw * 2.0, cell.bottom), ui.pal.accent);
                ui.pop_fade();
                ui.pop_xf();
            }
        }
        if li == 3 && (ui.now.saturating_duration_since(d.enter).as_millis() % 1000) < 500 {
            let cx = code_x + cw * chars.len() as f32;
            ui.r.text("▏", &f, Rect::new(cx, ly, cx + cw * 2.0, ly + lh), ui.pal.accent);
        }
    }
    ui.r.pop_clip();

    // Etiqueta "Fuente · tamaño" que lleva a Fuentes.
    let chip = format!("{family} · {} px", px_target as u32);
    let cwid = ui.measure(&chip, 11.0, false) + 16.0;
    let cr = Rect::new(card.right - 12.0 - cwid, card.top + 10.0, card.right - 12.0, card.top + 30.0);
    let hv = ui.hover_t(Hit::Static(1), 150);
    ui.r.fill_round(cr, 4.0, ui.row_c(hv * 2.0));
    ui.r.text_center(&chip, &ui.font(11.0, false), cr, ui.pal.text_2.mix(ui.pal.text, hv));
    ui.hit(cr, Hit::Static(1));
    y + h
}

// --- Fuentes ------------------------------------------------------------------------

fn fuentes(ui: &mut Ui, d: &PageData, x: f32, top: f32, w: f32) -> f32 {
    let cfg = d.cfg;
    let mut y = crumb_header(ui, d, x, top, w) + 14.0;
    let px = model::font_px(cfg);
    // Controles: muestra, buscador, tamaño.
    let f12 = ui.font(12.0, false);
    let size_w = 8.0 + ui.measure("Tamaño", 12.0, false) + 8.0 + 80.0 + 8.0 + 44.0;
    let search_w = 140.0;
    let sample_w = (w - size_w - search_w - 24.0).max(120.0);
    let row_h = 30.0;
    let sample_r = Rect::new(x, y, x + sample_w, y + row_h);
    ui.input(sample_r, InputId::Sample, &d.inputs.sample, d.focus == Some(InputId::Sample), d.caret_on, "Texto de muestra", &f12, false);
    let search_r = Rect::new(sample_r.right + 12.0, y, sample_r.right + 12.0 + search_w, y + row_h);
    ui.input(search_r, InputId::Search, &d.inputs.search, d.focus == Some(InputId::Search), d.caret_on, "Buscar fuente", &f12, false);
    let mut sx = search_r.right + 12.0;
    ui.text("Tamaño", 12.0, false, Rect::new(sx, y, sx + 60.0, y + row_h), ui.pal.text_2);
    sx += ui.measure("Tamaño", 12.0, false) + 8.0;
    let track = Rect::new(sx, y + row_h / 2.0 - 2.0, sx + 80.0, y + row_h / 2.0 + 2.0);
    slider(ui, track, px);
    let fm = ui.mono(ui.r.mono_family(), 12.0);
    ui.r.text(&format!("{px} px"), &fm, Rect::new(track.right + 10.0, y, track.right + 60.0, y + row_h), ui.pal.text);
    y += row_h + 14.0;

    let shown = filtered_fonts(d.fonts, &d.inputs.search.text);
    let current = cfg.ui.font_family.primary_name();
    if d.fonts.is_empty() {
        ui.text("Buscando fuentes…", 12.0, false, Rect::new(x, y, x + w, y + 20.0), ui.pal.text_3);
        return y + 20.0;
    }
    if shown.is_empty() {
        ui.text("Ninguna fuente monoespaciada coincide con la búsqueda.", 12.0, false, Rect::new(x, y, x + w, y + 20.0), ui.pal.text_3);
        return y + 20.0;
    }
    let sample = if d.inputs.sample.text.is_empty() { "fn main() -> Ok(0O 1lI) != {}" } else { d.inputs.sample.text.as_str() };
    let sample_h = (px as f32 * 1.45).ceil();
    let h = 12.0 + 16.0 + 6.0 + sample_h + 12.0;
    let clip = ui.clip;
    for (j, name) in shown.iter().enumerate() {
        let r = Rect::new(x, y, x + w, y + h);
        if r.bottom < clip.top - 20.0 || r.top > clip.bottom + 20.0 {
            y += h + 6.0;
            continue;
        }
        let idx = d.fonts.iter().position(|f| f == *name).unwrap_or(0) as u16;
        let hit = Hit::Font(idx);
        item(ui, d, j);
        let hv = ui.hover_spring(hit, 250);
        let sel = name.as_str() == current;
        let st = ui.on_t(hit, sel, 250, Curve::Out);
        ui.push_xf(Matrix3x2::translation(3.0 * hv, 0.0));
        ui.r.fill_round(r, 8.0, ui.row_c(hv.clamp(0.0, 1.0) * 2.0).mix(ui.pal.accent, 0.07 * st));
        if st > 0.01 {
            ui.r.stroke_round_rect(Rect::new(r.left + 0.75, r.top + 0.75, r.right - 0.75, r.bottom - 0.75), 7.25, 1.5, ui.pal.accent.faded(st));
        }
        let mut tx = x + 14.0;
        let nw = ui.measure(name, 12.0, false);
        ui.text(name, 12.0, false, Rect::new(tx, y + 12.0, tx + nw + 2.0, y + 28.0), ui.pal.text_2);
        tx += nw + 8.0;
        for (tag, on) in [("Ligaduras", LIGATURE_FONTS.contains(&name.as_str())), ("De Windows", WINDOWS_FONTS.contains(&name.as_str()))] {
            if on {
                let tw = ui.measure(tag, 11.0, false) + 12.0;
                let tr = Rect::new(tx, y + 12.0, tx + tw, y + 28.0);
                ui.r.fill_round(tr, 4.0, ui.pal.chrome);
                ui.r.text_center(tag, &ui.font(11.0, false), tr, ui.pal.text_2);
                tx += tw + 6.0;
            }
        }
        // ✓ que entra girando.
        let ck = ui.tw.to(TK::Badge(hit), sel as u8 as f32, 350, Curve::Spring, ui.now);
        if ck > 0.01 {
            let c = Vector2 { X: r.right - 22.0, Y: y + 20.0 };
            let s = 0.4 + 0.6 * ck;
            ui.push_xf(Matrix3x2::rotation_around(-20.0 * (1.0 - ck), c) * Matrix3x2::scale_around(s, s, c));
            ui.push_fade(ck.min(1.0));
            ui.icon(icon::CHECK, c.X, c.Y, 16.0, 2.2, ui.pal.accent);
            ui.pop_fade();
            ui.pop_xf();
        }
        let f = ui.mono(name, px as f32);
        ui.r.push_clip(Rect::new(r.left + 14.0, y + 34.0, r.right - 14.0, y + 34.0 + sample_h + 2.0));
        ui.r.text(sample, &f, Rect::new(x + 14.0, y + 34.0, x + w * 3.0, y + 34.0 + sample_h), ui.pal.text);
        ui.r.pop_clip();
        ui.pop_xf();
        ui.hit(r, hit);
        ui.end_enter();
        y += h + 6.0;
    }
    y - 6.0
}

/// Deslizador de tamaño: pista con la parte llena en acento y bolita.
fn slider(ui: &mut Ui, track: Rect, px: u32) {
    let span = (model::FONT_PX_MAX - model::FONT_PX_MIN) as f32;
    let target = (px - model::FONT_PX_MIN) as f32 / span;
    let t = ui.tween(TK::Pill(SettingKey::FontFamily), target, 120, Curve::Out);
    let hv = ui.hover_t(Hit::Slider, 150).max(ui.press_t(Hit::Slider, 100));
    ui.r.fill_round(track, 2.0, ui.track_c());
    let kx = track.left + track.width() * t;
    ui.r.fill_round(Rect::new(track.left, track.top, kx, track.bottom), 2.0, ui.pal.accent);
    let kr = 7.0 + 1.5 * hv;
    ui.r.fill_circle(kx, track.top + track.height() / 2.0, kr, ui.pal.accent);
    ui.r.fill_circle(kx, track.top + track.height() / 2.0, kr - 3.0, ui.pal.on_accent);
    ui.hit(Rect::new(track.left - 8.0, track.top - 12.0, track.right + 8.0, track.bottom + 12.0), Hit::Slider);
}

/// Tamaño en px bajo la `x` del ratón sobre el deslizador de `fuentes`.
pub(crate) fn slider_px(track_left: f32, track_w: f32, mx: f32) -> u32 {
    let t = ((mx - track_left) / track_w).clamp(0.0, 1.0);
    model::FONT_PX_MIN + (t * (model::FONT_PX_MAX - model::FONT_PX_MIN) as f32).round() as u32
}

// --- Ligaduras ----------------------------------------------------------------------

pub(crate) const LIG_SAMPLE: &str = "if a != b => f(x) -> y :: z <= w";

fn ligaduras(ui: &mut Ui, d: &PageData, x: f32, top: f32, w: f32) -> f32 {
    let cfg = d.cfg;
    let key = SettingKey::Ligatures;
    let on = cfg.ui.ligatures;
    let mut y = crumb_header(ui, d, x, top, w);
    // "Activadas" + interruptor a la derecha de la cabecera.
    let tr = Rect::new(x + w - 40.0, y - 25.0, x + w, y - 3.0);
    ui.toggle(tr, Hit::Toggle(key), on);
    let aw = ui.measure("Activadas", 12.0, false);
    ui.text("Activadas", 12.0, false, Rect::new(tr.left - 10.0 - aw, tr.top, tr.left - 8.0, tr.bottom), ui.pal.text_2);
    y += 14.0;

    // Tal como lo escribes / lo ves.
    let entries = crate::ligature::entries(&cfg.ligature_overrides, &cfg.ligature_disabled);
    let f14 = ui.mono(ui.r.mono_family(), 14.0);
    let cw = ui.r.measure("0", &f14).max(1.0);
    let card = Rect::new(x, y, x + w, y + 14.0 + 16.0 + 8.0 + 20.0 + 8.0 + 16.0 + 8.0 + 20.0 + 14.0);
    ui.r.fill_round(card, 8.0, ui.pal.chrome);
    ui.r.fill_round(Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0), 7.0, ui.pal.cmd);
    let mut cy = card.top + 14.0;
    let lx = x + 16.0;
    ui.text("Tal como lo escribes", 11.0, false, Rect::new(lx, cy, x + w, cy + 16.0), ui.pal.text_3);
    cy += 24.0;
    ui.r.text(LIG_SAMPLE, &f14, Rect::new(lx, cy, x + w, cy + 20.0), ui.pal.text_2);
    cy += 28.0;
    ui.text("Tal como lo ves", 11.0, false, Rect::new(lx, cy, x + w, cy + 16.0), ui.pal.text_3);
    cy += 24.0;
    let chars: Vec<char> = LIG_SAMPLE.chars().collect();
    let mut sorted: Vec<&crate::ligature::Entry> = entries.iter().collect();
    sorted.sort_by_key(|e| std::cmp::Reverse(e.seq.chars().count()));
    let mut i = 0;
    while i < chars.len() {
        let found = sorted.iter().find(|e| {
            let n = e.seq.chars().count();
            n > 0 && i + n <= chars.len() && chars[i..i + n].iter().copied().eq(e.seq.chars())
        });
        let rx = lx + cw * i as f32;
        match found {
            Some(e) => {
                let n = e.seq.chars().count();
                let t = ui.tween(TK::Morph(key_hash(&e.seq) ^ 1), (on && e.enabled) as u8 as f32, 360, Curve::Spring);
                let a = t.clamp(0.0, 1.0);
                let center = Vector2 { X: rx + cw * n as f32 / 2.0, Y: cy + 10.0 };
                if a < 0.99 {
                    let s = 1.0 - 0.3 * t;
                    ui.push_xf(Matrix3x2::scale_around(s, s, center) * Matrix3x2::translation(0.0, 7.0 * t));
                    ui.push_fade(1.0 - a);
                    ui.r.text(&e.seq, &f14, Rect::new(rx, cy, rx + cw * (n as f32 + 1.0), cy + 20.0), ui.pal.text);
                    ui.pop_fade();
                    ui.pop_xf();
                }
                if a > 0.01 {
                    let s = 0.7 + 0.3 * t;
                    ui.push_xf(Matrix3x2::scale_around(s, s, Vector2 { X: rx + cw / 2.0, Y: cy + 10.0 }) * Matrix3x2::translation(0.0, -7.0 * (1.0 - t)));
                    ui.push_fade(a);
                    ui.r.text(&e.glyph.to_string(), &f14, Rect::new(rx, cy, rx + cw * 2.0, cy + 20.0), ui.pal.accent);
                    ui.pop_fade();
                    ui.pop_xf();
                }
                i += n;
            }
            None => {
                let s = chars[i].to_string();
                ui.r.text(&s, &f14, Rect::new(rx, cy, rx + cw * 2.0, cy + 20.0), ui.pal.text);
                i += 1;
            }
        }
    }
    y = card.bottom + 14.0;

    // Añadir: secuencia → símbolo.
    let seq_r = Rect::new(x, y, x + 90.0, y + 30.0);
    ui.input(seq_r, InputId::LigSeq, &d.inputs.seq, d.focus == Some(InputId::LigSeq), d.caret_on, "|>", &f14, true);
    ui.icon(icon::FLECHA, seq_r.right + 16.0, y + 15.0, 16.0, 1.8, ui.pal.text_3);
    let gl_r = Rect::new(seq_r.right + 32.0, y, seq_r.right + 102.0, y + 30.0);
    ui.input(gl_r, InputId::LigGlyph, &d.inputs.glyph, d.focus == Some(InputId::LigGlyph), d.caret_on, "▷", &f14, true);
    let can = model::can_add_ligature(&d.inputs.seq.text, &d.inputs.glyph.text).is_ok();
    let bw = ui.button_w("Añadir ligadura", true);
    let br = Rect::new(gl_r.right + 8.0, y - 1.0, gl_r.right + 8.0 + bw, y + 31.0);
    ui.button(br, "Añadir ligadura", true, can, Hit::LigAdd);
    let note = "Se guarda en config.toml";
    if br.right + 14.0 + ui.measure(note, 12.0, false) < x + w {
        ui.text(note, 12.0, false, Rect::new(br.right + 14.0, y, x + w, y + 30.0), ui.pal.text_3);
    }
    y += 30.0 + 14.0;

    // Cuadrícula de 2 columnas.
    let colw = (w - 6.0) / 2.0;
    let ch = 42.0;
    for (j, e) in entries.iter().enumerate() {
        let col = j % 2;
        let rowi = j / 2;
        let cx = x + (colw + 6.0) * col as f32;
        let cyy = y + (ch + 6.0) * rowi as f32;
        let r = Rect::new(cx, cyy, cx + colw, cyy + ch);
        item(ui, d, j);
        let tg = Hit::LigToggle(j as u16);
        let hv = ui.hover_t(Hit::Static(100 + j as u16), 200);
        let active = e.enabled && on;
        let op = ui.tween(TK::On(Hit::Static(300 + j as u16)), if active { 1.0 } else { 0.45 }, 250, Curve::Linear);
        ui.push_fade(op);
        ui.r.fill_round(r, 6.0, ui.row_c(hv * 2.0));
        ui.hit(r, Hit::Static(100 + j as u16));
        let mid = r.top + ch / 2.0;
        ui.r.text(&e.seq, &f14, Rect::new(cx + 12.0, r.top, cx + 56.0, r.bottom), ui.pal.text_2);
        ui.icon(icon::FLECHA, cx + 64.0, mid, 14.0, 1.8, ui.pal.text_3);
        let f18 = ui.mono(ui.r.mono_family(), 18.0);
        let gs = ui.hover_spring(Hit::Static(100 + j as u16), 350);
        let gc = Vector2 { X: cx + 90.0, Y: mid };
        ui.push_xf(Matrix3x2::scale_around(1.0 + 0.35 * gs, 1.0 + 0.35 * gs, gc));
        ui.r.text_center(&e.glyph.to_string(), &f18, Rect::new(cx + 76.0, r.top, cx + 104.0, r.bottom), ui.pal.accent);
        ui.pop_xf();
        ui.text(if e.builtin { "De serie" } else { "Propia" }, 11.0, false, Rect::new(cx + 114.0, r.top, r.right - 80.0, r.bottom), ui.pal.text_3);
        let tr = Rect::new(r.right - 52.0, mid - 11.0, r.right - 12.0, mid + 11.0);
        if !e.builtin {
            let del = Hit::LigDelete(j as u16);
            let dh = ui.hover_spring(del, 250);
            let dr = Rect::new(tr.left - 34.0, mid - 13.0, tr.left - 8.0, mid + 13.0);
            ui.r.fill_round(dr, 5.0, ui.pal.danger.faded(0.14 * dh.clamp(0.0, 1.0)));
            let dc = Vector2 { X: dr.left + 13.0, Y: mid };
            ui.push_xf(Matrix3x2::rotation_around(90.0 * dh, dc));
            ui.icon(icon::CRUZ, dc.X, dc.Y, 14.0, 1.8, ui.pal.text_3.mix(ui.pal.danger, dh.clamp(0.0, 1.0)));
            ui.pop_xf();
            ui.hit(dr, del);
        }
        ui.pop_fade();
        ui.toggle(tr, tg, e.enabled);
        ui.end_enter();
    }
    let rows = entries.len().div_ceil(2);
    y + (ch + 6.0) * rows as f32 - 6.0
}

// --- Sintaxis -----------------------------------------------------------------------

fn sintaxis(ui: &mut Ui, d: &PageData, x: f32, top: f32, w: f32) -> f32 {
    use crate::syntax::LANGS;
    let cfg = d.cfg;
    let pal = ui.pal;
    let key = SettingKey::SyntaxHighlight;
    let on = cfg.ui.syntax_highlight;
    let mut y = crumb_header(ui, d, x, top, w);
    let tr = Rect::new(x + w - 40.0, y - 25.0, x + w, y - 3.0);
    ui.toggle(tr, Hit::Toggle(key), on);
    let aw = ui.measure("Activado", 12.0, false);
    ui.text("Activado", 12.0, false, Rect::new(tr.left - 10.0 - aw, tr.top, tr.left - 8.0, tr.bottom), pal.text_2);
    y += 14.0;

    // Vista previa del lenguaje elegido, con los colores de verdad.
    let pick = d.syn_pick.min(LANGS.len() - 1);
    let info = &LANGS[pick];
    let lang_on = on && !cfg.syntax_disabled.iter().any(|id| id == info.id);
    let family = cfg.ui.font_family.primary_name();
    let f = ui.mono(family, 13.0);
    let cw = ui.r.measure("0", &f).max(1.0);
    let lh = 20.0;
    const ROWS: usize = 6;
    let card = Rect::new(x, y, x + w, y + 28.0 + lh * ROWS as f32);
    ui.r.fill_round(card, 8.0, pal.chrome);
    ui.r.fill_round(Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0), 7.0, pal.cmd);
    let syn = ui.tween(TK::Preview(40), lang_on as u8 as f32, 350, Curve::Linear);
    let since = d.syn_pick_at.elapsed().as_secs_f32() * 1000.0;
    let fade = if ui.anim() { (since / 220.0).clamp(0.0, 1.0) } else { 1.0 };
    let lift = 8.0 * (1.0 - crate::anim::ease_out_cubic(fade));
    let (doc, cache) = &d.preview.langs[pick];
    let lines: Vec<&str> = info.sample.lines().take(ROWS).collect();
    let spans = cache.spans_for(doc, Some(info.lang), 0..lines.len());
    ui.r.push_clip(Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0));
    ui.push_fade(fade);
    for (li, line) in lines.iter().enumerate() {
        let ly = card.top + 14.0 + lh * li as f32 + lift;
        let chars: Vec<char> = line.chars().collect();
        let mut colors: Vec<Rgba> = vec![pal.text; chars.len()];
        for s in spans.get(li).into_iter().flatten() {
            if let Some(c) = crate::syntax::color(pal, s.highlight) {
                for col in s.start as usize..(s.start + s.len) as usize {
                    if let Some(slot) = colors.get_mut(col) {
                        *slot = pal.text.mix(c, syn);
                    }
                }
            }
        }
        let mut start = 0;
        while start < chars.len() {
            let mut end = start + 1;
            while end < chars.len() && colors[end] == colors[start] {
                end += 1;
            }
            let s: String = chars[start..end].iter().collect();
            if !s.trim().is_empty() {
                let rx = card.left + 16.0 + cw * start as f32;
                ui.r.text(&s, &f, Rect::new(rx, ly, rx + cw * (end - start) as f32 + cw, ly + lh), colors[start]);
            }
            start = end;
        }
    }
    ui.pop_fade();
    ui.r.pop_clip();
    let chip = format!("{} · .{}", info.name, info.exts.first().copied().unwrap_or(""));
    let chip_w = ui.measure(&chip, 11.0, false) + 16.0;
    let cr = Rect::new(card.right - 12.0 - chip_w, card.top + 10.0, card.right - 12.0, card.top + 30.0);
    ui.r.fill_round(cr, 4.0, ui.row_c(0.0));
    ui.r.text_center(&chip, &ui.font(11.0, false), cr, pal.text_2);
    y = card.bottom + 10.0;

    // Leyenda de colores.
    let legend = [
        ("Palabra clave", pal.syn_keyword),
        ("Cadena", pal.syn_string),
        ("Número", pal.syn_number),
        ("Tipo", pal.syn_type),
        ("Función", pal.syn_function),
        ("Comentario", pal.syn_comment),
    ];
    let mut lx = x + 2.0;
    for (name, c) in legend {
        let nw = ui.measure(name, 11.0, false);
        if lx + 14.0 + nw > x + w {
            break;
        }
        ui.r.fill_round(Rect::new(lx, y + 4.0, lx + 8.0, y + 12.0), 2.0, c);
        ui.text(name, 11.0, false, Rect::new(lx + 12.0, y, lx + 12.0 + nw + 2.0, y + 16.0), pal.text_3);
        lx += 12.0 + nw + 14.0;
    }
    y += 16.0 + 14.0;

    // Contador + activar/desactivar todos.
    let active = LANGS.iter().filter(|l| !cfg.syntax_disabled.iter().any(|id| id == l.id)).count();
    ui.text(&format!("{active} de {} lenguajes activos", LANGS.len()), 12.0, false, Rect::new(x, y, x + w, y + 18.0), pal.text_2);
    let all_w = ui.measure("Desactivar todos", 12.0, false);
    ui.more(x + w - all_w, y + 9.0, "Desactivar todos", Hit::LangAll(false));
    let on_w = ui.measure("Activar todos", 12.0, false);
    ui.more(x + w - all_w - 16.0 - on_w, y + 9.0, "Activar todos", Hit::LangAll(true));
    y += 18.0 + 10.0;

    // Cuadrícula: clic en la celda = verla arriba; interruptor = encender/apagar.
    let colw = (w - 6.0) / 2.0;
    let ch = 50.0;
    let fm = ui.mono(ui.r.mono_family(), 11.0);
    for (j, l) in LANGS.iter().enumerate() {
        let cx = x + (colw + 6.0) * (j % 2) as f32;
        let cyy = y + (ch + 6.0) * (j / 2) as f32;
        let r = Rect::new(cx, cyy, cx + colw, cyy + ch);
        item(ui, d, j);
        let pick_hit = Hit::LangPick(j as u16);
        let enabled = !cfg.syntax_disabled.iter().any(|id| id == l.id);
        let hv = ui.hover_t(pick_hit, 200);
        let op = ui.tween(TK::On(Hit::Static(400 + j as u16)), if enabled && on { 1.0 } else { 0.45 }, 250, Curve::Linear);
        let sel = ui.tween(TK::On(pick_hit), (j == pick) as u8 as f32, 250, Curve::Out);
        ui.push_fade(op);
        ui.r.fill_round(r, 6.0, ui.row_c(hv * 2.0).mix(pal.accent_soft, sel * 0.6));
        ui.pop_fade();
        if sel > 0.01 {
            ui.r.stroke_round_rect(Rect::new(r.left + 0.75, r.top + 0.75, r.right - 0.75, r.bottom - 0.75), 6.0, 1.5, pal.accent.faded(sel));
        }
        ui.hit(r, pick_hit);
        ui.push_fade(op);
        ui.text(l.name, 13.0, false, Rect::new(cx + 14.0, r.top + 8.0, r.right - 64.0, r.top + 26.0), pal.text);
        let exts: Vec<String> = l.exts.iter().take(4).map(|e| format!(".{e}")).collect();
        ui.r.text(&exts.join(" "), &fm, Rect::new(cx + 14.0, r.top + 27.0, r.right - 64.0, r.top + 43.0), pal.text_3);
        ui.pop_fade();
        let tr = Rect::new(r.right - 52.0, r.top + ch / 2.0 - 11.0, r.right - 12.0, r.top + ch / 2.0 + 11.0);
        ui.toggle(tr, Hit::LangToggle(j as u16), enabled);
        ui.end_enter();
    }
    y + (ch + 6.0) * LANGS.len().div_ceil(2) as f32 - 6.0
}

// --- Ventana ------------------------------------------------------------------------

fn ventana(ui: &mut Ui, d: &PageData, x: f32, top: f32, w: f32) -> f32 {
    let cfg = d.cfg;
    blk(ui, d, 0);
    let mut y = header(ui, x, top, w, "Ventana", "Así queda la ventana principal") + 16.0;
    blk_end(ui);

    blk(ui, d, 1);
    y = window_diagram(ui, cfg, x, y, w) + 16.0;
    blk_end(ui);

    blk(ui, d, 2);
    let sel = model::selected_index(cfg, SettingKey::Files, model::FILES_OPTS);
    let desc = model::FILES_DESC[sel.unwrap_or(0)];
    let dh = desc_h(ui, w - 28.0, desc);
    let rect = Rect::new(x, y, x + w, y + 12.0 + 17.0 + dh + 10.0 + 38.0 + 12.0);
    ui.row_bg(rect, Hit::None, 6.0);
    title_desc(ui, x + 14.0, y + 12.0, w - 28.0, "Varios archivos", desc);
    let labels: Vec<&str> = model::FILES_OPTS.iter().map(|(l, _)| *l).collect();
    ui.big_seg(Rect::new(x + 14.0, rect.bottom - 12.0 - 38.0, x + w - 14.0, rect.bottom - 12.0), SettingKey::Files, &labels, sel);
    y = rect.bottom + 16.0;
    blk_end(ui);

    blk(ui, d, 3);
    let rows: [(&str, &str, Option<SettingKey>, bool); 5] = [
        ("Posición de las pestañas", "", Some(SettingKey::TabsPosition), true),
        ("Barra de menús", "", Some(SettingKey::MenuBar), true),
        ("Barra de atajos", "Estilo nano. Cambia según lo que estés haciendo.", Some(SettingKey::HintsBar), false),
        ("Barra de estado", "Si la ocultas, reaparece para rutas, búsquedas y avisos.", Some(SettingKey::StatusBar), false),
        ("Línea de comandos fusionada", "Una sola línea abajo para estado y comandos, como en Zen.", Some(SettingKey::MergedCommandLine), false),
    ];
    for (j, (title, desc, key, is_seg)) in rows.iter().enumerate() {
        let Some(key) = *key else { continue };
        item(ui, d, j);
        y = if *is_seg { seg_row(ui, x, y, w, cfg, key, title) } else { toggle_row(ui, x, y, w, cfg, key, title, desc) } + 4.0;
        ui.end_enter();
    }
    blk_end(ui);
    y - 4.0
}

/// Maqueta viva de la ventana principal: cada banda crece/encoge al cambiar su ajuste.
fn window_diagram(ui: &mut Ui, cfg: &Config, x: f32, y: f32, w: f32) -> f32 {
    let pal = ui.pal;
    let card = Rect::new(x, y, x + w, y + 176.0);
    ui.r.fill_round(card, 8.0, pal.chrome);
    ui.r.fill_round(Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0), 7.0, pal.cmd);
    let win = Rect::new(card.left + 40.0, card.top + 14.0, card.right - 40.0, card.bottom - 14.0);
    ui.r.draw_popup_shadow(win, 6.0, 0.8, pal.shadow);
    ui.r.fill_round(win, 6.0, pal.surface);
    ui.r.push_clip(win);

    let ui_cfg = &cfg.ui;
    let tabs_shown = ui_cfg.files != Files::Buffers && ui_cfg.tabs_position != TabsPosition::Hidden;
    let title_tabs = tabs_shown && matches!(ui_cfg.tabs_position, TabsPosition::Title | TabsPosition::Auto);
    let below = tabs_shown && ui_cfg.tabs_position == TabsPosition::Below;
    let menu = ui_cfg.menubar == MenuBar::Visible;
    let buf = ui_cfg.files == Files::Buffers;
    let hints = ui_cfg.hints_bar && !ui_cfg.merged_command_line;
    let status = ui_cfg.status_bar || ui_cfg.merged_command_line;
    let merged = ui_cfg.merged_command_line;
    let splits = ui_cfg.files == Files::Splits;
    let tv = |ui: &mut Ui, k: u8, on: bool| ui.tween(TK::Preview(40 + k), on as u8 as f32, 400, Curve::Out);
    let t_title = tv(ui, 0, title_tabs);
    let t_menu = tv(ui, 1, menu);
    let t_below = tv(ui, 2, below);
    let t_buf = tv(ui, 3, buf);
    let t_hints = tv(ui, 4, hints);
    let t_status = tv(ui, 5, status);
    let t_merged = tv(ui, 6, merged);
    let t_split = tv(ui, 7, splits);

    let mut yy = win.top;
    // Título.
    ui.r.fill(Rect::new(win.left, yy, win.right, yy + 18.0), pal.chrome);
    if t_title > 0.01 {
        ui.push_fade(t_title);
        let tw = 124.0 * t_title;
        ui.r.push_clip(Rect::new(win.left + 6.0, yy, win.left + 6.0 + tw, yy + 18.0));
        ui.r.fill_round(Rect::new(win.left + 6.0, yy + 5.0, win.left + 64.0, yy + 20.0), 3.0, pal.surface);
        ui.r.fill(Rect::new(win.left + 6.0, yy + 5.0, win.left + 64.0, yy + 7.0), pal.accent);
        ui.r.fill_round(Rect::new(win.left + 67.0, yy + 5.0, win.left + 125.0, yy + 20.0), 3.0, pal.line);
        ui.r.pop_clip();
        ui.pop_fade();
    }
    ui.r.fill(Rect::new(win.right - 30.0, yy + 9.0, win.right - 25.0, yy + 10.0), pal.text_2);
    ui.r.stroke_rect(Rect::new(win.right - 19.5, yy + 6.5, win.right - 14.5, yy + 11.5), 1.0, pal.text_2);
    yy += 18.0;
    // Menú.
    let mh = 14.0 * t_menu;
    if mh > 0.3 {
        ui.push_fade(t_menu);
        ui.r.fill(Rect::new(win.left, yy, win.right, yy + mh), pal.chrome);
        for i in 0..3 {
            bar(ui, win.left + 8.0 + 30.0 * i as f32, yy + mh / 2.0 - 1.5, 22.0, 3.0, pal.text_3);
        }
        ui.pop_fade();
    }
    yy += mh;
    // Pestañas bajo el menú.
    let bh = 15.0 * t_below;
    if bh > 0.3 {
        ui.push_fade(t_below);
        ui.r.fill(Rect::new(win.left, yy, win.right, yy + bh), pal.surface_2);
        for i in 0..3 {
            let tx = win.left + 6.0 + 61.0 * i as f32;
            let c = if i == 0 { pal.surface } else { pal.chrome };
            ui.r.fill_round(Rect::new(tx, yy + bh - 12.0, tx + 58.0, yy + bh + 3.0), 3.0, c);
            if i == 0 {
                ui.r.fill(Rect::new(tx, yy + bh - 12.0, tx + 58.0, yy + bh - 10.0), pal.accent);
            }
        }
        ui.pop_fade();
    }
    yy += bh;
    // Abajo: estado, atajos, buffer.
    let sh = 12.0 * t_status;
    let hh = 12.0 * t_hints;
    let bufh = 12.0 * t_buf;
    let panes_bottom = win.bottom - sh - hh - bufh;
    // Paneles.
    let area = Rect::new(win.left, yy, win.right, panes_bottom);
    ui.r.fill(area, pal.line);
    let g = [1.0, t_split, t_split];
    let total: f32 = g.iter().sum();
    let mut px = area.left;
    for (i, gi) in g.iter().enumerate() {
        let pw = (area.width() - 2.0 * t_split) * gi / total;
        if pw < 0.5 {
            continue;
        }
        let pr = Rect::new(px, area.top, px + pw, area.bottom);
        ui.push_fade(if i == 0 { 1.0 } else { t_split });
        ui.r.fill(pr, pal.surface);
        if i == 0 && t_split > 0.01 {
            ui.r.fill(Rect::new(pr.left, pr.top, pr.right, pr.top + 2.0), pal.accent.faded(t_split));
        }
        ui.r.push_clip(pr);
        for (k, fr) in [0.55f32, 0.75, 0.4].iter().enumerate() {
            bar(ui, pr.left + 8.0, pr.top + 8.0 + 7.0 * k as f32, (pw - 16.0).max(0.0) * fr, 3.0, ui.track_c());
        }
        ui.r.pop_clip();
        ui.pop_fade();
        px += pw + t_split;
    }
    let mut by = panes_bottom;
    if bufh > 0.3 {
        ui.push_fade(t_buf);
        ui.r.fill(Rect::new(win.left, by, win.right, by + bufh), pal.surface);
        let f8 = ui.mono(ui.r.mono_family(), 8.0);
        ui.r.text(":b 2 · notas.md", &f8, Rect::new(win.left + 8.0, by, win.right, by + bufh), pal.accent);
        ui.pop_fade();
    }
    by += bufh;
    if hh > 0.3 {
        ui.push_fade(t_hints);
        ui.r.fill(Rect::new(win.left, by, win.right, by + hh), pal.surface);
        ui.r.fill(Rect::new(win.left, by, win.right, by + 1.0), pal.chrome);
        for i in 0..4 {
            bar(ui, win.left + 8.0 + 40.0 * i as f32, by + hh / 2.0 - 1.0, 30.0, 3.0, pal.text_3);
        }
        ui.pop_fade();
    }
    by += hh;
    if sh > 0.3 {
        ui.push_fade(t_status);
        ui.r.fill(Rect::new(win.left, by, win.right, by + sh), pal.cmd);
        let lw = 80.0 + 70.0 * t_merged;
        bar(ui, win.left + 8.0, by + sh / 2.0 - 1.5, lw, 3.0, pal.text_3.mix(pal.accent, t_merged));
        bar(ui, win.right - 48.0, by + sh / 2.0 - 1.5, 40.0, 3.0, pal.text_3);
        ui.pop_fade();
    }
    ui.r.pop_clip();
    card.bottom
}

// --- Teclado ------------------------------------------------------------------------

fn teclado(ui: &mut Ui, d: &PageData, x: f32, top: f32, w: f32) -> f32 {
    let cfg = d.cfg;
    blk(ui, d, 0);
    let mut y = header(ui, x, top, w, "Teclado", "") + 16.0;
    blk_end(ui);

    blk(ui, d, 1);
    let key = SettingKey::VimAlways;
    let hit = Hit::Toggle(key);
    let on = cfg.ui.vim_always;
    let rect = Rect::new(x, y, x + w, y + 76.0);
    let hv = ui.row_bg(rect, hit, 6.0);
    ui.hit(rect, hit);
    let ic = Rect::new(x + 16.0, y + 16.0, x + 60.0, y + 60.0);
    let t = ui.tween(TK::Preview(50), on as u8 as f32, 350, Curve::Spring);
    let bg = ui.pal.surface.mix(ui.pal.accent, t.clamp(0.0, 1.0));
    let (cx, cy) = ui.ic_begin(ic, hv, bg);
    let fb = ui.fmts.get(ui.r, Some(ui.r.mono_family()), 12.0, true);
    let fg = ui.pal.text_2.mix(ui.pal.on_accent, t.clamp(0.0, 1.0));
    swap_text(ui, "INS", "NOR", t, cx, cy, &fb, fg);
    ui.pop_xf();
    let tx = ic.right + 12.0;
    let tw = x + w - 16.0 - 52.0 - tx;
    ui.text("Modo vim siempre", 13.0, false, Rect::new(tx, y + 20.0, tx + tw, y + 37.0), ui.pal.text);
    // Descripción con las teclas como `.kbd` si cabe en una línea.
    let parts = ["Cada ventana arranca en modo vim. ", " para insertar, ", " para salir."];
    let need = parts.iter().map(|p| ui.measure(p, 12.0, false)).sum::<f32>() + ui.kbd_w("i", 11.0) + ui.kbd_w("Esc", 11.0);
    let dy = y + 48.0;
    if need <= tw {
        let mut px = tx;
        for (i, p) in parts.iter().enumerate() {
            let pw = ui.measure(p, 12.0, false);
            ui.text(p, 12.0, false, Rect::new(px, dy - 9.0, px + pw + 2.0, dy + 9.0), ui.pal.text_2);
            px += pw;
            if i < 2 {
                let k = if i == 0 { "i" } else { "Esc" };
                px = ui.kbd(px, dy, k, 0.0, 0.0, 11.0).right;
            }
        }
    } else {
        ui.text("i para insertar, Esc para salir.", 12.0, false, Rect::new(tx, dy - 9.0, tx + tw, dy + 9.0), ui.pal.text_2);
    }
    ui.toggle(Rect::new(x + w - 56.0, y + 27.0, x + w - 16.0, y + 49.0), hit, on);
    y = rect.bottom;
    blk_end(ui);

    blk(ui, d, 2);
    y = group(ui, x, y, w, "Atajos · haz clic en uno para cambiarlo") + 6.0;
    blk_end(ui);

    blk(ui, d, 3);
    for (j, cmd) in model::BINDINGS.iter().enumerate() {
        item(ui, d, j);
        y = binding_row(ui, x, y, w, *cmd, &notty_input::binding_spec(cfg, *cmd), true) + 4.0;
        ui.end_enter();
    }
    blk_end(ui);

    blk(ui, d, 4);
    y = group(ui, x, y - 4.0, w, "Atajos globales · funcionan aunque notty no tenga el foco") + 6.0;
    blk_end(ui);
    blk(ui, d, 5);
    let (a, b) = if cfg.hotkey.mechanism == HotkeyMechanism::Daemon {
        ("Win+Alt+N", "Win+Alt+Shift+N")
    } else {
        ("Ctrl+Alt+letra", "Ctrl+Alt+letra")
    };
    y = static_bind(ui, x, y, w, 200, "Nuevo temporal", a) + 4.0;
    y = static_bind(ui, x, y, w, 201, "Nuevo permanente", b) + 8.0;
    blk_end(ui);

    blk(ui, d, 6);
    let hit = Hit::Link(LinkAction::OpenConfig);
    let desc = "Cada acción es un comando con nombre; también en config.toml, sección [keys].";
    let mw = ui.measure("Abrir [keys]", 12.0, false);
    let g = row(ui, x, y, w, hit, 0.0, desc, mw, 18.0);
    title_desc(ui, g.text_x, text_y(&g, desc, ui), g.text_w, "Todos los atajos", desc);
    ui.more(g.ctrl.left, g.ctrl.top + 9.0, "Abrir [keys]", hit);
    y = g.rect.bottom;
    blk_end(ui);
    y
}

/// `.bind`: título y combinación; se adelanta al pasar el ratón y destella en verde
/// tras guardar un atajo nuevo.
fn binding_row(ui: &mut Ui, x: f32, y: f32, w: f32, cmd: Command, spec: &str, _clickable: bool) -> f32 {
    let hit = Hit::Binding(cmd);
    let hv = ui.hover_spring(hit, 200);
    let h = 38.0;
    let r = Rect::new(x, y, x + w, y + h);
    ui.push_xf(Matrix3x2::translation(3.0 * hv, 0.0));
    let flash = ui.tw.to(TK::Flash(cmd), 0.0, 1200, Curve::Out, ui.now);
    let bg = ui.row_c(hv.clamp(0.0, 1.0) * 2.0).mix(ui.pal.ok, 0.28 * flash);
    ui.r.fill_round(r, 6.0, bg);
    ui.text(cmd.title(), 13.0, false, Rect::new(x + 14.0, y, x + w - 120.0, y + h), ui.pal.text);
    let keys = if spec.is_empty() { "sin atajo" } else { spec };
    let kw = ui.kbd_w(keys, 11.0);
    let press = ui.press_t(hit, 120);
    let kr = ui.kbd(x + w - 14.0 - kw, y + h / 2.0, keys, hv.clamp(0.0, 1.0), press, 11.0);
    let _ = kr;
    ui.pop_xf();
    ui.hit(r, hit);
    r.bottom
}

fn static_bind(ui: &mut Ui, x: f32, y: f32, w: f32, id: u16, title: &str, keys: &str) -> f32 {
    let hit = Hit::Static(id);
    let hv = ui.hover_t(hit, 150);
    let h = 38.0;
    let r = Rect::new(x, y, x + w, y + h);
    ui.r.fill_round(r, 6.0, ui.row_c(0.0));
    ui.text(title, 13.0, false, Rect::new(x + 14.0, y, x + w - 120.0, y + h), ui.pal.text);
    let kw = ui.kbd_w(keys, 11.0);
    ui.kbd(x + w - 14.0 - kw, y + h / 2.0, keys, hv, hv, 11.0);
    ui.hit(r, hit);
    r.bottom
}

// --- Archivos -----------------------------------------------------------------------

fn archivos(ui: &mut Ui, d: &PageData, x: f32, top: f32, w: f32) -> f32 {
    let cfg = d.cfg;
    blk(ui, d, 0);
    let mut y = header(ui, x, top, w, "Archivos", "Así se abre y guarda") + 16.0;
    blk_end(ui);

    blk(ui, d, 1);
    y = label(ui, x, y, w, "Archivos temporales") + 8.0;
    let sel = model::selected_index(cfg, SettingKey::TempMode, model::TEMP_MODE_OPTS);
    let cw = (w - 10.0) / 2.0;
    let defs = [
        ("Borrador", "Se guarda solo; se borra al cerrar si no le das ruta.", icon::BORRADOR, ui.pal.accent),
        ("Volátil", "Nunca toca el disco. Al cerrar, desaparece.", icon::GOTA, ui.pal.warn),
    ];
    let mut hmax: f32 = 68.0;
    for (_, desc, _, _) in &defs {
        hmax = hmax.max(28.0 + 17.0 + desc_h(ui, cw - 28.0 - 52.0, desc));
    }
    for (j, (title, desc, ic, c)) in defs.iter().enumerate() {
        let hit = Hit::Choice(SettingKey::TempMode, j as u8);
        let cx = x + (cw + 10.0) * j as f32;
        let r = Rect::new(cx, y, cx + cw, y + hmax);
        let selected = sel == Some(j);
        ui.card_begin(r, hit, selected);
        let fl = if selected && ui.anim() {
            let s = millis(d, ui.now) / 1800.0 * std::f32::consts::TAU;
            -1.5 + 1.5 * s.cos()
        } else {
            0.0
        };
        let icr = Rect::new(r.left + 14.0, r.top + (hmax - 40.0) / 2.0 + fl, r.left + 54.0, r.top + (hmax + 40.0) / 2.0 + fl);
        ui.r.fill_round(icr, 6.0, ui.pal.surface);
        ui.icon(ic, icr.left + 20.0, icr.top + 20.0, 20.0, 1.7, *c);
        let tx = icr.right + 12.0;
        let th = 17.0 + desc_h(ui, r.right - 14.0 - tx, desc);
        title_desc(ui, tx, r.top + (hmax - th) / 2.0, r.right - 14.0 - tx, title, desc);
        ui.card_end(r, hit, selected);
    }
    y += hmax + 16.0;
    blk_end(ui);

    blk(ui, d, 2);
    y = path_preview(ui, d, x, y, w) + 16.0;
    blk_end(ui);

    blk(ui, d, 3);
    let rows = [
        (SettingKey::NativeFileDialog, "Selector nativo de Windows", "Abrir y Guardar como usan el diálogo de Windows en vez de la línea de ruta."),
        (SettingKey::SuggestionIcons, "Iconos en las sugerencias", "Carpeta o archivo delante de cada sugerencia al escribir una ruta."),
        (SettingKey::Autosave, "Autoguardado", "Guarda sola tras dejar de escribir. Solo si el archivo ya tiene ruta."),
    ];
    for (j, (key, title, desc)) in rows.iter().enumerate() {
        item(ui, d, j);
        y = toggle_row(ui, x, y, w, cfg, *key, title, desc) + 4.0;
        ui.end_enter();
    }
    blk_end(ui);
    y - 4.0
}

/// Línea de ruta con sugerencias o, con el selector nativo, un diálogo de Windows.
fn path_preview(ui: &mut Ui, d: &PageData, x: f32, y: f32, w: f32) -> f32 {
    let cfg = d.cfg;
    let pal = ui.pal;
    let card = Rect::new(x, y, x + w, y + 128.0);
    ui.r.fill_round(card, 8.0, pal.chrome);
    let inner = Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0);
    ui.r.fill_round(inner, 7.0, pal.cmd);
    ui.r.push_clip(inner);
    let t = ui.tween(TK::Preview(60), cfg.ui.native_file_dialog as u8 as f32, 400, Curve::Out);
    let icons = ui.tween(TK::Preview(61), cfg.ui.suggestion_icons as u8 as f32, 300, Curve::Out);
    let center = Vector2 { X: card.left + w / 2.0, Y: card.top + 64.0 };
    let fm = ui.mono(ui.r.mono_family(), 12.0);
    if t < 0.99 {
        let s = 1.0 - 0.03 * t;
        ui.push_xf(Matrix3x2::scale_around(s, s, center) * Matrix3x2::translation(0.0, -12.0 * t));
        ui.push_fade(1.0 - t);
        let prompt_top = inner.bottom - 28.0;
        let sugg = [("crates\\", true), ("Cargo.toml", false), ("CHANGELOG.md", false)];
        let mut sy = prompt_top - 6.0 - 24.0 * sugg.len() as f32;
        for (j, (name, dir)) in sugg.iter().enumerate() {
            item(ui, d, j);
            let r = Rect::new(inner.left + 12.0, sy, inner.right - 12.0, sy + 22.0);
            if j == 0 {
                ui.r.fill_round(r, 4.0, pal.accent_soft);
            }
            let mut tx = r.left + 8.0;
            let iw = 14.0 * icons;
            if iw > 0.5 {
                ui.push_fade(icons);
                let ic = if *dir { icon::CARPETA } else { icon::ARCHIVO };
                let c = if *dir { pal.warn } else { pal.text_2 };
                ui.icon(ic, tx + 6.5, r.top + 11.0, 13.0, 1.8, c);
                ui.pop_fade();
            }
            tx += iw + 8.0 * icons;
            let c = if j == 0 { pal.text } else { pal.text_2 };
            ui.r.text(name, &fm, Rect::new(tx, r.top, r.right, r.bottom), c);
            ui.end_enter();
            sy += 24.0;
        }
        ui.r.fill(Rect::new(inner.left, prompt_top, inner.right, inner.bottom), pal.cmd.mix(pal.surface, 0.0).mix(Rgba(0.0, 0.0, 0.0, 1.0), 0.15));
        ui.r.fill(Rect::new(inner.left, prompt_top, inner.right, prompt_top + 1.0), pal.chrome);
        let aw = ui.r.measure("Abrir", &fm);
        ui.r.text("Abrir", &fm, Rect::new(inner.left + 12.0, prompt_top, inner.left + 12.0 + aw + 2.0, inner.bottom), pal.accent);
        let path = "E:\\notty\\cra";
        let px = inner.left + 12.0 + aw + 8.0;
        let pw = ui.r.measure(path, &fm);
        ui.r.text(path, &fm, Rect::new(px, prompt_top, px + pw + 4.0, inner.bottom), pal.text);
        if (millis(d, ui.now) as u64 % 1000) < 500 || !ui.anim() {
            ui.r.fill(Rect::new(px + pw + 2.0, prompt_top + 7.0, px + pw + 9.0, prompt_top + 21.0), pal.text);
        }
        ui.pop_fade();
        ui.pop_xf();
    }
    if t > 0.01 {
        let s = 0.97 + 0.03 * t;
        ui.push_xf(Matrix3x2::scale_around(s, s, center) * Matrix3x2::translation(0.0, 12.0 * (1.0 - t)));
        ui.push_fade(t);
        let lw = ui.measure("Diálogo de Windows", 12.0, false);
        let total = 170.0 + 14.0 + lw;
        let dx = card.left + (w - total) / 2.0;
        let dlg = Rect::new(dx, card.top + 18.0, dx + 170.0, card.top + 110.0);
        ui.r.draw_popup_shadow(dlg, 6.0, 0.8, pal.shadow);
        ui.r.fill_round(dlg, 6.0, pal.chrome);
        ui.r.fill(Rect::new(dlg.left, dlg.top + 3.0, dlg.right, dlg.top + 14.0), pal.line);
        ui.r.fill_round(Rect::new(dlg.left, dlg.top, dlg.right, dlg.top + 14.0), 6.0, pal.line);
        ui.r.fill(Rect::new(dlg.left, dlg.top + 14.0, dlg.left + 44.0, dlg.bottom - 6.0), pal.surface_2);
        bar(ui, dlg.left + 52.0, dlg.top + 22.0, 118.0 * 0.7 * 0.85, 4.0, ui.track_c());
        bar(ui, dlg.left + 52.0, dlg.top + 31.0, 118.0 * 0.55 * 0.85, 4.0, pal.accent);
        bar(ui, dlg.left + 52.0, dlg.top + 40.0, 118.0 * 0.62 * 0.85, 4.0, ui.track_c());
        ui.text("Diálogo de Windows", 12.0, false, Rect::new(dlg.right + 14.0, card.top, dlg.right + 14.0 + lw + 4.0, card.bottom), pal.text_2);
        ui.pop_fade();
        ui.pop_xf();
    }
    ui.r.pop_clip();
    card.bottom
}

// --- Atajo global ---------------------------------------------------------------------

/// Interpola por tramos `(tiempo 0..1, valor)` con `Out` en cada tramo (keyframes).
fn keyframes(p: f32, kf: &[(f32, f32)]) -> f32 {
    let p = p.rem_euclid(1.0);
    for w in kf.windows(2) {
        let (t0, v0) = w[0];
        let (t1, v1) = w[1];
        if p >= t0 && p <= t1 {
            let k = if t1 > t0 { (p - t0) / (t1 - t0) } else { 1.0 };
            return v0 + (v1 - v0) * Curve::Out.apply(k);
        }
    }
    kf.last().map(|k| k.1).unwrap_or(0.0)
}

fn atajo(ui: &mut Ui, d: &PageData, x: f32, top: f32, w: f32) -> f32 {
    let cfg = d.cfg;
    blk(ui, d, 0);
    let mut y = header(ui, x, top, w, "Atajo global", "") + 16.0;
    blk_end(ui);

    blk(ui, d, 1);
    let pal = ui.pal;
    let card = Rect::new(x, y, x + w, y + 120.0);
    ui.r.fill_round(card, 8.0, pal.chrome);
    ui.r.fill_round(Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0), 7.0, pal.cmd);
    let daemon = cfg.hotkey.mechanism == HotkeyMechanism::Daemon;
    let keys = [if daemon { "Win" } else { "Ctrl" }, "Alt", "N"];
    let plus_w = ui.measure("+", 15.0, false);
    let kws: Vec<f32> = keys.iter().map(|k| ui.kbd_w(k, 15.0) + 10.0).collect();
    let total = kws.iter().sum::<f32>() + (plus_w + 20.0) * 2.0 + 22.0 + 16.0 + 96.0 + 10.0;
    let mut kx = card.left + (w - total) / 2.0;
    let cy = card.top + 60.0;
    let el = if ui.anim() { millis(d, ui.now) } else { 1500.0 };
    let period = 3400.0;
    for (i, k) in keys.iter().enumerate() {
        let p = (el - 180.0 * i as f32) / period;
        let pr = if ui.anim() { keyframes(p, &[(0.0, 0.0), (0.06, 0.0), (0.10, 1.0), (0.62, 1.0), (0.70, 0.0), (1.0, 0.0)]) } else { 0.0 };
        let r = ui.kbd(kx, cy, k, pr, pr * 1.3, 15.0);
        kx = r.right + 10.0;
        if i < 2 {
            ui.text("+", 15.0, false, Rect::new(kx, cy - 12.0, kx + plus_w + 2.0, cy + 12.0), pal.text_3);
            kx += plus_w + 10.0;
        }
    }
    let p = el / period;
    let ax = if ui.anim() { keyframes(p, &[(0.0, -4.0), (0.2, -4.0), (0.3, 3.0), (0.82, 3.0), (1.0, 0.0)]) } else { 3.0 };
    let ao = if ui.anim() { keyframes(p, &[(0.0, 0.3), (0.2, 0.3), (0.3, 1.0), (0.82, 1.0), (1.0, 0.3)]) } else { 1.0 };
    ui.push_fade(ao);
    ui.icon(icon::FLECHA, kx + 8.0 + 11.0 + ax, cy, 22.0, 1.8, pal.accent);
    ui.pop_fade();
    kx += 8.0 + 22.0 + 16.0;
    let wo = if ui.anim() { keyframes(p, &[(0.0, 0.0), (0.26, 0.0), (0.36, 1.0), (0.82, 1.0), (0.92, 0.0), (1.0, 0.0)]) } else { 1.0 };
    let ws = if ui.anim() { keyframes(p, &[(0.0, 0.8), (0.26, 0.8), (0.36, 1.0), (0.82, 1.0), (0.92, 0.96), (1.0, 0.96)]) } else { 1.0 };
    let wdy = if ui.anim() { keyframes(p, &[(0.0, 8.0), (0.26, 8.0), (0.36, 0.0), (1.0, 0.0)]) } else { 0.0 };
    let wr = Rect::new(kx, cy - 31.0, kx + 96.0, cy + 31.0);
    let wc = Vector2 { X: wr.left + 48.0, Y: cy };
    ui.push_xf(Matrix3x2::scale_around(ws, ws, wc) * Matrix3x2::translation(0.0, wdy));
    ui.push_fade(wo);
    ui.r.draw_popup_shadow(wr, 5.0, 0.8, pal.shadow);
    ui.r.fill_round(wr, 5.0, pal.surface);
    ui.r.fill_round(Rect::new(wr.left, wr.top, wr.right, wr.top + 10.0), 5.0, pal.chrome);
    ui.r.fill(Rect::new(wr.left, wr.top + 5.0, wr.right, wr.top + 10.0), pal.chrome);
    let t10 = ui.font(10.0, false);
    let sw = ui.r.measure("sin título", &t10);
    ui.r.text("sin título", &t10, Rect::new(wr.left + 8.0, wr.top + 14.0, wr.left + 8.0 + sw + 2.0, wr.top + 30.0), pal.text_2);
    if (el as u64 % 1000) < 500 {
        ui.r.text("▏", &t10, Rect::new(wr.left + 8.0 + sw, wr.top + 14.0, wr.right, wr.top + 30.0), pal.accent);
    }
    ui.pop_fade();
    ui.pop_xf();
    y = card.bottom + 16.0;
    blk_end(ui);

    blk(ui, d, 2);
    y = label(ui, x, y, w, "Cómo se escucha el atajo") + 8.0;
    let sel = model::selected_index(cfg, SettingKey::HotkeyMechanism, model::HOTKEY_OPTS);
    let cw = (w - 10.0) / 2.0;
    let defs = [("Segundo plano", "~1 MB de RAM · cualquier combinación · instantáneo"), ("Acceso directo", "Nada residente · solo Ctrl+Alt+letra")];
    let mut h: f32 = 0.0;
    for (_, desc) in &defs {
        h = h.max(28.0 + 17.0 + 6.0 + desc_h(ui, cw - 28.0, desc));
    }
    for (j, (title, desc)) in defs.iter().enumerate() {
        let hit = Hit::Choice(SettingKey::HotkeyMechanism, j as u8);
        let cx = x + (cw + 10.0) * j as f32;
        let r = Rect::new(cx, y, cx + cw, y + h);
        ui.card_begin(r, hit, sel == Some(j));
        ui.text(title, 13.0, false, Rect::new(cx + 14.0, y + 14.0, r.right - 14.0, y + 31.0), pal.text);
        let dh = desc_h(ui, cw - 28.0, desc);
        ui.r.text_wrapped(desc, &ui.font(12.0, false), Rect::new(cx + 14.0, y + 37.0, r.right - 14.0, y + 37.0 + dh), pal.text_2, Some(DESC_LH));
        ui.card_end(r, hit, sel == Some(j));
    }
    y += h + 16.0;
    blk_end(ui);

    blk(ui, d, 3);
    y = toggle_row(ui, x, y, w, cfg, SettingKey::StartWithWindows, "Iniciar con Windows", "");
    blk_end(ui);
    y
}

// --- Actualizaciones ------------------------------------------------------------------

fn actualizaciones(ui: &mut Ui, d: &PageData, x: f32, top: f32, w: f32) -> f32 {
    let cfg = d.cfg;
    let pal = ui.pal;
    let up = d.update;
    let version = crate::app_version();
    let newv = up.new_version.clone().unwrap_or_default();
    blk(ui, d, 0);
    let mut y = header(ui, x, top, w, "Actualizaciones", "") + 14.0;
    blk_end(ui);

    let (headline, sub): (String, String) = match &up.phase {
        UpdatePhase::Idle if up.up_to_date => ("notty está al día".into(), "Tienes la última versión publicada.".into()),
        UpdatePhase::Idle => ("Sin comprobar en esta sesión".into(), "Busca cuando quieras: se consultan las releases de GitHub.".into()),
        UpdatePhase::Checking => ("Buscando…".into(), "Consultando las releases de GitHub.".into()),
        UpdatePhase::Found => (format!("Hay una versión nueva: {newv}"), format!("{version} → {newv}")),
        UpdatePhase::Downloading(..) => (format!("Descargando {newv}"), "Se verifica la firma al terminar.".into()),
        UpdatePhase::Error(e) => (
            if up.new_version.is_some() { "No se pudo actualizar".into() } else { "No se pudo comprobar".into() },
            e.clone(),
        ),
    };
    let has_new = matches!(up.phase, UpdatePhase::Found | UpdatePhase::Downloading(..));
    let phase_id: u8 = match up.phase {
        UpdatePhase::Idle => 0,
        UpdatePhase::Checking => 1,
        UpdatePhase::Found => 2,
        UpdatePhase::Downloading(..) => 3,
        UpdatePhase::Error(_) => 4,
    };

    blk(ui, d, 1);
    // Botones a la derecha.
    let (btn_label, primary, enabled, hit) = match up.phase {
        UpdatePhase::Found => ("Descargar e instalar", true, true, Hit::Link(LinkAction::DownloadUpdate)),
        UpdatePhase::Downloading(..) => ("", false, false, Hit::None),
        UpdatePhase::Checking => ("Buscar actualizaciones", false, false, Hit::Link(LinkAction::CheckUpdatesNow)),
        _ => ("Buscar actualizaciones", false, true, Hit::Link(LinkAction::CheckUpdatesNow)),
    };
    let bw = if btn_label.is_empty() { 0.0 } else { ui.button_w(btn_label, primary) };
    let text_x = x + 22.0 + 56.0 + 18.0;
    let text_w = (x + w - 22.0 - bw - 18.0 - text_x).max(80.0);
    let sub_h = ui.r.measure_wrapped(&sub, &ui.font(12.0, false), text_w, Some(DESC_LH)).max(DESC_LH);
    let head_h = (22.0 + 3.0 + sub_h).max(56.0);
    let bar_h = if matches!(up.phase, UpdatePhase::Downloading(..)) { 14.0 + 6.0 + 6.0 + 16.0 } else { 0.0 };
    let card = Rect::new(x, y, x + w, y + 40.0 + head_h + bar_h);
    ui.r.fill_round(card, 10.0, ui.row_c(0.0));
    let icy = card.top + 20.0 + head_h / 2.0;
    let icx = x + 22.0 + 28.0;
    let (ibg, ifg) = match up.phase {
        UpdatePhase::Idle if up.up_to_date => (pal.ok.faded(0.14), pal.ok),
        UpdatePhase::Idle => (pal.hover, pal.text_2),
        UpdatePhase::Error(_) => (pal.danger.faded(0.14), pal.danger),
        _ => (pal.accent_soft, pal.accent),
    };
    if matches!(up.phase, UpdatePhase::Found) && ui.anim() {
        let p = (millis(d, ui.now) % 1600.0) / 1600.0;
        let rr = 28.0 + 16.0 * Curve::Out.apply(p);
        ui.r.fill_circle(icx, icy, rr, pal.accent.faded(0.45 * (1.0 - p)));
    }
    ui.r.fill_circle(icx, icy, 28.0, ibg);
    let pop = ui.tw.from_to(TK::Preview(70 + phase_id), 0.0, 1.0, 450, Curve::Spring, ui.now);
    let c = Vector2 { X: icx, Y: icy };
    match up.phase {
        UpdatePhase::Checking => {
            let ang = if ui.anim() { (millis(d, ui.now) % 900.0) / 900.0 * 360.0 } else { 0.0 };
            ui.push_xf(Matrix3x2::rotation_around(ang, c));
            ui.icon(icon::GIRO, icx, icy, 24.0, 2.0, ifg);
            ui.pop_xf();
        }
        _ => {
            let d_icon = match up.phase {
                UpdatePhase::Idle if up.up_to_date => icon::CHECK,
                UpdatePhase::Idle => icon::ACTUALIZAR,
                UpdatePhase::Error(_) => icon::CRUZ,
                _ => icon::DESCARGA,
            };
            let s = 0.4 + 0.6 * pop;
            ui.push_xf(Matrix3x2::scale_around(s, s, c));
            ui.push_fade(pop.clamp(0.0, 1.0));
            ui.icon(d_icon, icx, icy, 24.0, 2.0, ifg);
            ui.pop_fade();
            ui.pop_xf();
        }
    }
    let ty = card.top + 20.0 + (head_h - 22.0 - 3.0 - sub_h) / 2.0;
    ui.text(&headline, 16.0, true, Rect::new(text_x, ty, text_x + text_w, ty + 22.0), pal.text);
    ui.r.text_wrapped(&sub, &ui.font(12.0, false), Rect::new(text_x, ty + 25.0, text_x + text_w, ty + 25.0 + sub_h), pal.text_2, Some(DESC_LH));
    if !btn_label.is_empty() {
        let br = Rect::new(x + w - 22.0 - bw, icy - 16.0, x + w - 22.0, icy + 16.0);
        let bp = ui.tw.from_to(TK::Preview(80 + phase_id), 0.0, 1.0, 450, Curve::Spring, ui.now);
        let bc = Vector2 { X: br.left + bw / 2.0, Y: icy };
        let s = 0.4 + 0.6 * bp;
        ui.push_xf(Matrix3x2::scale_around(s, s, bc));
        ui.push_fade(bp.clamp(0.0, 1.0));
        ui.button(br, btn_label, primary, enabled, hit);
        ui.pop_fade();
        ui.pop_xf();
    }
    if let UpdatePhase::Downloading(done, total) = up.phase {
        let pct = if total > 0 { (done as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
        let pv = ui.tween(TK::Preview(90), pct, 120, Curve::Linear);
        let by = card.top + 20.0 + head_h + 14.0;
        let track = Rect::new(text_x, by, x + w - 22.0, by + 6.0);
        ui.r.fill_round(track, 3.0, pal.cmd);
        let fill = Rect::new(track.left, track.top, track.left + track.width() * pv, track.bottom);
        ui.r.fill_round(fill, 3.0, pal.accent);
        if ui.anim() && fill.width() > 2.0 {
            // Brillo que recorre la barra.
            ui.r.push_clip(fill);
            let p = (millis(d, ui.now) % 1200.0) / 1200.0;
            let sw = fill.width() * 0.4;
            let sx = fill.left - sw + (fill.width() + sw) * p;
            for k in 0..6 {
                let a = 0.25 * (1.0 - (k as f32 - 2.5).abs() / 3.0);
                let seg = sw / 6.0;
                ui.r.fill(Rect::new(sx + seg * k as f32, fill.top, sx + seg * (k + 1) as f32, fill.bottom), Rgba(1.0, 1.0, 1.0, a));
            }
            ui.r.pop_clip();
        }
        let fm = ui.mono(ui.r.mono_family(), 11.0);
        ui.r.text("notty-setup.exe", &fm, Rect::new(track.left, by + 12.0, track.right, by + 28.0), pal.text_2);
        let label = if total > 0 { format!("{} %", (pct * 100.0).round() as u32) } else { format!("{} KB", done / 1024) };
        ui.r.text_right(&label, &fm, Rect::new(track.left, by + 12.0, track.right, by + 28.0), pal.text_2);
    }
    y = card.bottom + 14.0;
    blk_end(ui);

    if has_new {
        blk(ui, d, 2);
        let bullets = model::release_bullets(&up.notes, 6);
        let mut bh = 0.0;
        for b in &bullets {
            bh += ui.r.measure_wrapped(b, &ui.font(12.0, false), w - 44.0 - 12.0, Some(18.0)).max(18.0) + 4.0;
        }
        let nc = Rect::new(x, y, x + w, y + 16.0 + 20.0 + 10.0 + bh + (if up.release_url.is_some() { 24.0 } else { 0.0 }) + 12.0);
        ui.r.fill_round(nc, 10.0, pal.accent.faded(0.35));
        ui.r.fill_round(Rect::new(nc.left + 1.0, nc.top + 1.0, nc.right - 1.0, nc.bottom - 1.0), 9.0, ui.row_c(0.0));
        let mut ny = nc.top + 16.0;
        ui.text(&format!("Novedades de v{newv}"), 13.0, true, Rect::new(x + 22.0, ny, x + w - 22.0, ny + 20.0), pal.text);
        ny += 30.0;
        if bullets.is_empty() {
            ui.text("Esta versión no trae notas.", 12.0, false, Rect::new(x + 22.0, ny, x + w - 22.0, ny + 18.0), pal.text_2);
        }
        for (j, b) in bullets.iter().enumerate() {
            item(ui, d, j + 3);
            ny += bullets_dot(ui, x + 22.0, ny, b, w - 44.0, ui.text_soft()) + 4.0;
            ui.end_enter();
        }
        if up.release_url.is_some() {
            ui.more(x + 22.0, ny + 10.0, "Ver todas las notas en GitHub", Hit::Link(LinkAction::ReleaseNotes));
        }
        y = nc.bottom + 14.0;
        blk_end(ui);
    }

    blk(ui, d, 3);
    item(ui, d, 0);
    y = toggle_row(ui, x, y, w, cfg, SettingKey::UpdatesCheck, "Buscar al abrir notty", "Una vez al día contra GitHub Releases. Nunca se activa solo.") + 4.0;
    ui.end_enter();
    item(ui, d, 1);
    let desc = "Cada instalador se comprueba con Ed25519 antes de ejecutarse.";
    let sw = ui.measure("Siempre", 12.0, false) + 20.0;
    let g = row(ui, x, y, w, Hit::None, 0.0, desc, sw, 18.0);
    title_desc(ui, g.text_x, text_y(&g, desc, ui), g.text_w, "Firma verificada", desc);
    let cy = g.ctrl.top + 9.0;
    ui.icon(icon::ESCUDO, g.ctrl.left + 7.0, cy, 14.0, 2.0, pal.ok);
    ui.text("Siempre", 12.0, false, Rect::new(g.ctrl.left + 20.0, cy - 9.0, g.ctrl.right + 4.0, cy + 9.0), pal.ok);
    y = g.rect.bottom + 14.0;
    ui.end_enter();
    blk_end(ui);

    blk(ui, d, 4);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let a = "Versión instalada ";
    let aw = ui.measure(a, 12.0, false);
    ui.text(a, 12.0, false, Rect::new(x, y, x + aw + 2.0, y + 18.0), pal.text_3);
    let fm = ui.mono(ui.r.mono_family(), 12.0);
    let vw = ui.r.measure(version, &fm);
    ui.r.text(version, &fm, Rect::new(x + aw, y, x + aw + vw + 2.0, y + 18.0), pal.text_2);
    let last = format!("Última comprobación: {}", model::relative_time(now, cfg.updates.last_check));
    ui.text(&last, 12.0, false, Rect::new(x + aw + vw + 16.0, y, x + w, y + 18.0), pal.text_3);
    y += 18.0;
    blk_end(ui);
    y
}

// --- Acerca de ----------------------------------------------------------------------

fn acerca(ui: &mut Ui, d: &PageData, x: f32, top: f32, w: f32) -> f32 {
    let pal = ui.pal;
    blk(ui, d, 0);
    let mut y = header(ui, x, top, w, "Acerca de", "") + 16.0;
    blk_end(ui);

    blk(ui, d, 1);
    let card = Rect::new(x, y, x + w, y + 116.0);
    ui.r.fill_round(card, 10.0, pal.chrome);
    ui.r.fill_round(Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0), 9.0, pal.cmd);
    let (fy, rot) = if ui.anim() {
        let s = millis(d, ui.now) / 4000.0 * std::f32::consts::TAU;
        (-2.0 + 2.0 * s.cos(), -1.0 + s.cos())
    } else {
        (0.0, 0.0)
    };
    let logo = Rect::new(x + 26.0, card.top + 26.0 + fy, x + 90.0, card.top + 90.0 + fy);
    let c = Vector2 { X: logo.left + 32.0, Y: logo.top + 32.0 };
    ui.push_xf(Matrix3x2::rotation_around(rot, c));
    match d.logo {
        Some(bmp) => ui.r.draw_bitmap(bmp, logo, 1.0),
        None => ui.r.fill_round(logo, 14.0, pal.accent),
    }
    ui.pop_xf();
    let tx = logo.right + 20.0;
    ui.text("notty", 22.0, true, Rect::new(tx, card.top + 24.0, x + w - 20.0, card.top + 54.0), pal.text);
    ui.text("Editor de texto para Windows. Rust + Direct2D.", 12.0, false, Rect::new(tx, card.top + 56.0, x + w - 20.0, card.top + 74.0), pal.text_2);
    let fm = ui.mono(ui.r.mono_family(), 12.0);
    ui.r.text(&format!("v{}", crate::app_version()), &fm, Rect::new(tx, card.top + 76.0, x + w, card.top + 94.0), pal.accent);
    y = card.bottom + 16.0;
    blk_end(ui);

    blk(ui, d, 2);
    let repo = format!("github.com/{}", notty_update::REPO);
    item(ui, d, 0);
    y = link_row(ui, x, y, w, Hit::Link(LinkAction::OpenRepo), "Código fuente", &repo, false, icon::EXTERNO) + 4.0;
    ui.end_enter();
    item(ui, d, 1);
    y = link_row(ui, x, y, w, Hit::Link(LinkAction::OpenChangelog), "Novedades", "Historial de cambios de cada versión", false, icon::CHEVRON) + 4.0;
    ui.end_enter();
    item(ui, d, 2);
    y = link_row(ui, x, y, w, Hit::Link(LinkAction::OpenConfigFolder), "Carpeta de configuración", "%APPDATA%\\notty\\config.toml", true, icon::CHEVRON);
    ui.end_enter();
    blk_end(ui);
    y
}

// --- Ayuda --------------------------------------------------------------------------

fn ayuda(ui: &mut Ui, d: &PageData, x: f32, top: f32, w: f32) -> f32 {
    let cfg = d.cfg;
    let pal = ui.pal;
    blk(ui, d, 0);
    let mut y = header(ui, x, top, w, "Ayuda", "") + 16.0;
    blk_end(ui);

    blk(ui, d, 1);
    let hit = Hit::Link(LinkAction::RepeatTutorial);
    let bw = ui.button_w("Empezar", true);
    let desc = "El recorrido guiado sobre la ventana principal, paso a paso.";
    let tx = x + 20.0 + 48.0 + 16.0;
    let tw = x + w - 20.0 - bw - 16.0 - tx;
    let h = (36.0 + 20.0 + desc_h(ui, tw, desc)).max(84.0);
    let r = Rect::new(x, y, x + w, y + h);
    let hv = ui.row_bg(r, Hit::Static(50), 10.0);
    ui.hit(r, Hit::Static(50));
    let ic = Rect::new(x + 20.0, y + (h - 48.0) / 2.0, x + 68.0, y + (h + 48.0) / 2.0);
    let (cx, cy) = ui.ic_begin(ic, hv, pal.surface);
    let ang = if ui.anim() { (millis(d, ui.now) % 6000.0) / 6000.0 * 360.0 } else { 0.0 };
    ui.push_xf(Matrix3x2::rotation_around(ang, Vector2 { X: cx, Y: cy }));
    ui.r.stroke_svg(icon::CIRCULO, cx, cy, 22.0, 1.7, pal.accent, Some((4.0, 3.0)));
    ui.pop_xf();
    ui.icon(icon::CIRCULO_PEQ, cx, cy, 22.0, 1.7, pal.accent);
    ui.pop_xf();
    let th = 20.0 + desc_h(ui, tw, desc);
    let ty = y + (h - th) / 2.0;
    ui.text("Repetir tutorial", 14.0, false, Rect::new(tx, ty, tx + tw, ty + 20.0), pal.text);
    ui.r.text_wrapped(desc, &ui.font(12.0, false), Rect::new(tx, ty + 22.0, tx + tw, ty + th), pal.text_2, Some(DESC_LH));
    ui.button(Rect::new(x + w - 20.0 - bw, y + h / 2.0 - 16.0, x + w - 20.0, y + h / 2.0 + 16.0), "Empezar", true, true, hit);
    y = r.bottom + 16.0;
    blk_end(ui);

    blk(ui, d, 2);
    y = label(ui, x, y, w, "Lo básico") + 8.0;
    let spec = |c: Command| {
        let s = notty_input::binding_spec(cfg, c);
        if s.is_empty() { "sin atajo".to_string() } else { s }
    };
    let basics: Vec<(&str, String)> = vec![
        ("Abrir archivo", "Ctrl+O".to_string()),
        ("Guardar", "Ctrl+S".to_string()),
        ("Buscar", "Ctrl+F".to_string()),
        ("Nueva pestaña", spec(Command::NewTab)),
        ("Tamaño del texto", format!("{} / {}", spec(Command::ZoomIn), spec(Command::ZoomOut))),
        ("Dividir panel", spec(Command::SplitPane)),
    ];
    let colw = (w - 6.0) / 2.0;
    for (j, (title, keys)) in basics.iter().enumerate() {
        let cx = x + (colw + 6.0) * (j % 2) as f32;
        let cy = y + 44.0 * (j / 2) as f32;
        item(ui, d, j);
        static_bind(ui, cx, cy, colw, 60 + j as u16, title, keys);
        ui.end_enter();
    }
    y += 44.0 * basics.len().div_ceil(2) as f32 - 6.0;
    blk_end(ui);
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_search_is_case_insensitive() {
        let fonts = vec!["Cascadia Mono".to_string(), "Consolas".to_string(), "Fira Code".to_string()];
        assert_eq!(filtered_fonts(&fonts, "  CAS ").len(), 1);
        assert_eq!(filtered_fonts(&fonts, "").len(), 3);
        assert!(filtered_fonts(&fonts, "zzz").is_empty());
    }

    #[test]
    fn slider_maps_the_track_onto_font_px() {
        assert_eq!(slider_px(0.0, 80.0, -5.0), model::FONT_PX_MIN);
        assert_eq!(slider_px(0.0, 80.0, 500.0), model::FONT_PX_MAX);
        assert_eq!(slider_px(0.0, 80.0, 40.0), 17);
    }

    #[test]
    fn keyframes_hold_and_interpolate() {
        let kf = [(0.0, 0.0), (0.5, 0.0), (1.0, 1.0)];
        assert_eq!(keyframes(0.25, &kf), 0.0);
        assert!(keyframes(0.75, &kf) > 0.0);
        assert!((keyframes(1.25, &kf) - 0.0).abs() < 1e-6);
    }
}
