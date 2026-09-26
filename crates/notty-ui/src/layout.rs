//! Geometría de la ventana principal en DIPs (= px CSS de la maqueta).

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }
    pub fn width(&self) -> f32 {
        self.right - self.left
    }
    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
    pub fn is_empty(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }
}

pub const TITLEBAR_H: f32 = 32.0; // más plana/compacta, como Notepad/Ajustes/Explorador en Windows 11
pub const APPICON_W: f32 = 36.0; // .appicon{width:36px}
pub const APPICON_SIZE: f32 = 16.0; // ICON() 16x16
pub const CAPTION_BTN_W: f32 = 46.0; // .caption button{width:46px}
pub const TAB_H: f32 = 28.0; // .tab{height:28px}
pub const TAB_GAP: f32 = 2.0; // .tabs{gap:2px}
pub const TAB_PAD_L: f32 = 12.0; // .tab{padding:0 6px 0 12px}
pub const TAB_PAD_R: f32 = 6.0;
pub const TAB_INNER_GAP: f32 = 6.0; // .tab{gap:6px}
pub const TAB_CLOSE: f32 = 18.0; // .tab .x{width:18px;height:18px}
pub const TAB_MAX_W: f32 = 190.0; // .tab{max-width:190px}
pub const TAB_RADIUS: f32 = 6.0; // border-radius:6px 6px 0 0
pub const PLUS_W: f32 = 28.0; // .tabs .plus{width:28px;height:24px;margin-left:2px;margin-top:2px}
pub const PLUS_H: f32 = 24.0;
pub const PLUS_ML: f32 = 2.0;
pub const MENUBAR_H: f32 = 30.0; // .menubar{padding:2px 6px} + button{padding:4px 10px} a 12.5px
pub const MENUBAR_PAD_X: f32 = 6.0;
pub const MENU_BTN_PAD_X: f32 = 10.0;
pub const MENU_BTN_GAP: f32 = 2.0;
pub const TABS_BELOW_H: f32 = 32.0; // .tabs.below{padding:4px 6px 0} + 28px
pub const TABS_BELOW_PAD_X: f32 = 6.0;
pub const HINTS_H: f32 = 22.0; // .hints{padding:4px 12px;border-top:1px} a 11px mono
pub const HINTS_PAD_X: f32 = 12.0;
pub const HINTS_GAP: f32 = 14.0; // .hints{gap:14px}
pub const STATUS_H: f32 = 24.0; // .status{height:24px}
pub const STATUS_MERGED_H: f32 = 26.0; // .win.merged .status{height:26px}
pub const STATUS_PAD_X: f32 = 12.0; // .status{padding:0 12px}
pub const STATUS_GAP: f32 = 16.0; // .status{gap:16px}
pub const STATUS_L_GAP: f32 = 10.0; // .status .l{gap:10px}
pub const STATUS_R_GAP: f32 = 6.0; // .status .r{gap:6px}
pub const STATUS_FLD_PAD_X: f32 = 5.0; // .status .fld{padding:2px 5px}
pub const GUTTER_W: f32 = 52.0; // .gutter{width:52px}
pub const GUTTER_PAD_R: f32 = 14.0; // .gutter div{padding-right:14px}
pub const TEXT_PAD_L: f32 = 4.0; // .ta{padding:10px 16px 10px 4px}
pub const TEXT_PAD_T: f32 = 10.0;
pub const LINE_H: f32 = 20.0; // --lh:20px
pub const HEX_PAD_L: f32 = 16.0; // .hex{padding:10px 16px}
pub const POPUP_RADIUS: f32 = 8.0; // .psuggest / .dropdown{border-radius:8px}
pub const POPUP_PAD: f32 = 4.0;

pub const FONT_UI: f32 = 12.0; // tabs, título
pub const FONT_MENU: f32 = 12.5; // .menubar > button
pub const FONT_STATUS: f32 = 11.0; // .status
pub const FONT_MONO: f32 = 13.0; // .ta / .gutter / .hex
pub const FONT_HINTS: f32 = 11.0; // .hints (mono)
pub const FONT_PROMPT: f32 = 12.5; // .pfield (mono)
pub const FONT_PROMPT_LABEL: f32 = 11.5; // .plabel / .pword / .count
pub const FONT_SUGGEST: f32 = 12.0; // .prow (mono)
pub const FONT_OPT: f32 = 11.0; // .opt (mono)

/// Qué franjas se muestran, ya resuelto a partir de `UiConfig`, el número de
/// documentos y si el menú `Alt` está desplegado.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Bands {
    pub tabs_in_title: bool,
    pub menubar: bool,
    pub tabs_below: bool,
    pub hints: bool,
    pub merged_status: bool,
}

/// Rectángulos de cada franja para una ventana de `w`×`h` DIPs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Frame {
    pub titlebar: Rect,
    /// Botón de Ajustes: no es un botón de ventana (no está en la maqueta), es propio
    /// de notty para que Ajustes se pueda encontrar sin saber el atajo de memoria.
    /// Más estrecho que los de ventana para distinguirlo de ellos a simple vista.
    pub settings_btn: Rect,
    pub caption_min: Rect,
    pub caption_max: Rect,
    pub caption_close: Rect,
    pub menubar: Rect,
    pub tabs_below: Rect,
    pub body: Rect,
    pub hints: Rect,
    pub status: Rect,
}

/// Ancho del botón de Ajustes de la barra de título: más estrecho que los 46px de los
/// botones de ventana (min/max/cerrar), para que no parezca uno de ellos.
pub const SETTINGS_BTN_W: f32 = 40.0;

/// Cuánto de cada banda se ve (`0.0` oculta, `1.0` entera) a mitad de una transición.
/// A diferencia de `Bands`, admite valores intermedios: así una transición nueva
/// (p.ej. pulsar Alt otra vez mientras la barra de menús aún se está desplegando)
/// arranca desde donde está ahora en vez de saltar al final de la anterior.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BandFrac {
    pub tabs_in_title: f32,
    pub menubar: f32,
    pub tabs_below: f32,
    /// Barra de atajos propia (ya descontado el caso de línea de comandos fusionada).
    pub hints: f32,
    pub merged_status: f32,
}

impl BandFrac {
    pub fn of(b: Bands) -> Self {
        let f = |v: bool| if v { 1.0 } else { 0.0 };
        Self {
            tabs_in_title: f(b.tabs_in_title),
            menubar: f(b.menubar),
            tabs_below: f(b.tabs_below),
            hints: f(b.hints && !b.merged_status),
            merged_status: f(b.merged_status),
        }
    }

    pub fn lerp(self, to: Self, t: f32) -> Self {
        let m = |a: f32, b: f32| a + (b - a) * t;
        Self {
            tabs_in_title: m(self.tabs_in_title, to.tabs_in_title),
            menubar: m(self.menubar, to.menubar),
            tabs_below: m(self.tabs_below, to.tabs_below),
            hints: m(self.hints, to.hints),
            merged_status: m(self.merged_status, to.merged_status),
        }
    }
}

/// Como `frame`, pero con bandas a medio aparecer/desaparecer: cada una ocupa la
/// fracción de su altura que diga `f`, en vez de saltar.
pub fn frame_frac(w: f32, h: f32, f: BandFrac) -> Frame {
    frame_with(
        w,
        h,
        MENUBAR_H * f.menubar,
        TABS_BELOW_H * f.tabs_below,
        HINTS_H * f.hints,
        STATUS_H + (STATUS_MERGED_H - STATUS_H) * f.merged_status,
    )
}

pub fn frame(w: f32, h: f32, bands: Bands) -> Frame {
    frame_with(
        w,
        h,
        if bands.menubar { MENUBAR_H } else { 0.0 },
        if bands.tabs_below { TABS_BELOW_H } else { 0.0 },
        if bands.hints && !bands.merged_status { HINTS_H } else { 0.0 },
        if bands.merged_status { STATUS_MERGED_H } else { STATUS_H },
    )
}

fn frame_with(w: f32, h: f32, menubar_h: f32, tabs_h: f32, hints_h: f32, status_h: f32) -> Frame {
    let titlebar = Rect::new(0.0, 0.0, w, TITLEBAR_H);
    let close = Rect::new(w - CAPTION_BTN_W, 0.0, w, TITLEBAR_H);
    let max = Rect::new(close.left - CAPTION_BTN_W, 0.0, close.left, TITLEBAR_H);
    let min = Rect::new(max.left - CAPTION_BTN_W, 0.0, max.left, TITLEBAR_H);
    let settings_btn = Rect::new(min.left - SETTINGS_BTN_W, 0.0, min.left, TITLEBAR_H);

    let mut y = TITLEBAR_H;
    let menubar = if menubar_h > 0.0 {
        let r = Rect::new(0.0, y, w, y + menubar_h);
        y += menubar_h;
        r
    } else {
        Rect::default()
    };
    let tabs_below = if tabs_h > 0.0 {
        let r = Rect::new(0.0, y, w, y + tabs_h);
        y += tabs_h;
        r
    } else {
        Rect::default()
    };

    let status = Rect::new(0.0, (h - status_h).max(y), w, h);
    let hints = if hints_h > 0.0 { Rect::new(0.0, (status.top - hints_h).max(y), w, status.top) } else { Rect::default() };
    let body_bottom = if hints.is_empty() { status.top } else { hints.top };
    let body = Rect::new(0.0, y, w, body_bottom.max(y));

    Frame { titlebar, settings_btn, caption_min: min, caption_max: max, caption_close: close, menubar, tabs_below, body, hints, status }
}

/// Ancho del canal de números: 52 DIPs como la maqueta, o más si el número más largo
/// no cabe con su margen derecho de 14 (a partir de 5 dígitos con Cascadia 13 px).
pub fn gutter_width(total_lines: usize, digit_w: f32) -> f32 {
    let digits = total_lines.max(1).to_string().len() as f32;
    GUTTER_W.max(digits * digit_w + GUTTER_PAD_R + TEXT_PAD_L)
}

/// Líneas de texto que caben en el cuerpo (con su margen superior de 10 DIPs).
/// `line_h` es `LINE_H` ya escalado por el zoom de texto (Ctrl+/Ctrl-), ver `Renderer::line_h`.
pub fn visible_lines(body: Rect, line_h: f32) -> usize {
    (((body.height() - TEXT_PAD_T).max(0.0)) / line_h).floor().max(1.0) as usize
}

/// Tamaño mínimo de la ventana principal en DIPs (`WM_GETMINMAXINFO`): por debajo de
/// esto la barra de título ya no tiene sitio para icono + pestañas + botones, ni el
/// cuerpo para un par de líneas con la barra de estado.
pub const MIN_WINDOW_W: f32 = 480.0;
pub const MIN_WINDOW_H: f32 = 320.0;

/// Ancho mínimo al que se encogen las pestañas cuando no caben todas a su ancho
/// natural: aún deja ver unas letras del nombre y la ✕.
pub const TAB_MIN_W: f32 = 72.0;
/// Botones ‹ › que aparecen a los lados de la fila cuando ni encogidas caben todas.
pub const TAB_SCROLL_W: f32 = 20.0;
/// Hueco libre que se reserva a la derecha de las pestañas de la barra de título para
/// poder arrastrar la ventana aunque haya muchas pestañas abiertas.
pub const TITLE_DRAG_MIN: f32 = 40.0;

/// Geometría de una pestaña ya colocada.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabGeom {
    /// Rectángulo que ocupa de verdad (encogido mientras entra/sale animada).
    pub rect: Rect,
    /// Dónde empieza el nombre y cuánto ancho tiene disponible (se recorta con «…»).
    pub name_x: f32,
    pub name_w: f32,
    /// Posición del punto ● de «sin guardar», si lo hay.
    pub dot_x: Option<f32>,
    pub close: Rect,
}

/// Una pestaña a colocar: ancho medido del nombre, si lleva ●, y escala horizontal
/// (`1.0` normal; entre 0 y 1 mientras crece al abrirse o encoge al cerrarse).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabSlot {
    pub name_w: f32,
    pub dirty: bool,
    pub scale: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabsLayout {
    /// Una entrada por `TabSlot`; `None` si queda fuera de la parte visible.
    pub tabs: Vec<Option<TabGeom>>,
    pub plus: Rect,
    /// Botones ‹ / › (solo si hay pestañas ocultas hacia ese lado).
    pub scroll_left: Option<Rect>,
    pub scroll_right: Option<Rect>,
}

fn tab_fixed(dirty: bool, dot_w: f32) -> f32 {
    let dot_extra = if dirty { TAB_INNER_GAP + dot_w } else { 0.0 };
    TAB_PAD_L + dot_extra + TAB_INNER_GAP + TAB_CLOSE + TAB_PAD_R
}

/// Coloca las pestañas de izquierda a derecha empezando en `x0`, con su borde
/// inferior en `bottom`, sin pasar nunca de `max_right` (ni ellas ni el botón `+`).
/// Si no caben a su ancho natural se encogen todas por igual (hasta `TAB_MIN_W`); si
/// ni así caben, se muestra el tramo que contiene `focus` (la pestaña activa) con
/// botones ‹ › a los lados.
pub fn tabs_fit(x0: f32, bottom: f32, max_right: f32, slots: &[TabSlot], focus: usize, dot_w: f32) -> TabsLayout {
    let top = bottom - TAB_H;
    let natural: Vec<f32> = slots.iter().map(|s| (tab_fixed(s.dirty, dot_w) + s.name_w).min(TAB_MAX_W)).collect();
    let scale = |i: usize| slots[i].scale.clamp(0.0, 1.0);
    let plus_space = PLUS_ML + PLUS_W;
    let avail = (max_right - x0 - plus_space).max(0.0);
    let row_w = |cap: f32, range: std::ops::Range<usize>| -> f32 {
        let sum: f32 = range.map(|i| (natural[i].min(cap) + TAB_GAP) * scale(i)).sum();
        (sum - TAB_GAP).max(0.0)
    };

    let n = slots.len();
    let (cap, first, last, overflow) = if row_w(TAB_MAX_W, 0..n) <= avail {
        (TAB_MAX_W, 0, n, false)
    } else if row_w(TAB_MIN_W, 0..n) <= avail {
        let (mut lo, mut hi) = (TAB_MIN_W, TAB_MAX_W);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if row_w(mid, 0..n) <= avail { lo = mid } else { hi = mid }
        }
        (lo, 0, n, false)
    } else {
        let avail2 = (avail - TAB_SCROLL_W * 2.0).max(0.0);
        let k = (((avail2 + TAB_GAP) / (TAB_MIN_W + TAB_GAP)).floor() as usize).clamp(1, n.max(1));
        let focus = focus.min(n.saturating_sub(1));
        let first = if focus < k { 0 } else { focus + 1 - k };
        let first = first.min(n.saturating_sub(k));
        (TAB_MIN_W, first, (first + k).min(n), true)
    };

    let mut out = vec![None; n];
    let mut x = if overflow { x0 + TAB_SCROLL_W } else { x0 };
    let mut last_gap = 0.0;
    for (i, slot) in slots.iter().enumerate().take(last).skip(first) {
        let s = scale(i);
        let content_w = natural[i].min(cap);
        let fixed = tab_fixed(slot.dirty, dot_w);
        let rect = Rect::new(x, top, x + content_w * s, bottom);
        let content_right = rect.left + content_w;
        let close_left = content_right - TAB_PAD_R - TAB_CLOSE;
        let close_top = top + (TAB_H - TAB_CLOSE) / 2.0;
        let close = Rect::new(close_left, close_top, close_left + TAB_CLOSE, close_top + TAB_CLOSE);
        let name_x = rect.left + TAB_PAD_L;
        let name_w = (content_w - fixed).max(0.0);
        let dot_x = if slot.dirty { Some(name_x + name_w + TAB_INNER_GAP) } else { None };
        out[i] = Some(TabGeom { rect, name_x, name_w, dot_x, close });
        last_gap = TAB_GAP * s;
        x = rect.right + last_gap;
    }
    let tabs_end = x - last_gap;

    let (scroll_left, scroll_right, plus_left) = if overflow {
        let l = Rect::new(x0, top, x0 + TAB_SCROLL_W, bottom);
        let r = Rect::new(tabs_end, top, tabs_end + TAB_SCROLL_W, bottom);
        (if first > 0 { Some(l) } else { None }, if last < n { Some(r) } else { None }, r.right + PLUS_ML)
    } else {
        (None, None, tabs_end + PLUS_ML)
    };
    let plus_left = plus_left.min(max_right - PLUS_W).max(x0);
    // .plus{align-self:center;margin-top:2px}: centrado en la fila de 34 px, 1 px más abajo.
    let row_top = bottom - TITLEBAR_H.max(TAB_H);
    let plus_top = row_top + (TITLEBAR_H - PLUS_H) / 2.0 + 1.0;
    let plus = Rect::new(plus_left, plus_top, plus_left + PLUS_W, plus_top + PLUS_H);
    TabsLayout { tabs: out, plus, scroll_left, scroll_right }
}

/// Rectángulo final de un popup de `w`×`h` que se quiere abrir con su esquina
/// superior izquierda en `(x, y)`, sin salirse de `bounds`: se desplaza a la izquierda
/// si no cabe a la derecha, y se abre hacia arriba (terminando en `flip_y`) si no cabe
/// por debajo.
pub fn clamp_popup(x: f32, y: f32, w: f32, h: f32, flip_y: f32, bounds: Rect) -> Rect {
    let left = x.min(bounds.right - w).max(bounds.left);
    let top = if y + h <= bounds.bottom {
        y
    } else if flip_y - h >= bounds.top {
        flip_y - h
    } else {
        (bounds.bottom - h).max(bounds.top)
    };
    Rect::new(left, top, left + w, top + h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moderna_bands_match_mockup() {
        let f = frame(920.0, 600.0, Bands { tabs_in_title: true, hints: true, ..Default::default() });
        assert_eq!(f.body.top, TITLEBAR_H);
        assert_eq!(f.status, Rect::new(0.0, 576.0, 920.0, 600.0));
        assert_eq!(f.hints, Rect::new(0.0, 554.0, 920.0, 576.0));
        assert_eq!(f.body.bottom, 554.0);
        assert_eq!(f.caption_close.left, 874.0);
        assert_eq!(f.caption_min.left, 782.0);
    }

    #[test]
    fn settings_button_sits_left_of_the_window_buttons_and_is_narrower() {
        let f = frame(920.0, 600.0, Bands::default());
        assert_eq!(f.settings_btn.right, f.caption_min.left);
        assert_eq!(f.settings_btn.width(), SETTINGS_BTN_W);
        assert!(f.settings_btn.width() < CAPTION_BTN_W);
    }

    #[test]
    fn clasica_stacks_menubar_and_tabs_below() {
        let f = frame(920.0, 600.0, Bands { menubar: true, tabs_below: true, ..Default::default() });
        assert_eq!(f.menubar, Rect::new(0.0, TITLEBAR_H, 920.0, TITLEBAR_H + MENUBAR_H));
        assert_eq!(f.tabs_below, Rect::new(0.0, TITLEBAR_H + MENUBAR_H, 920.0, TITLEBAR_H + MENUBAR_H + TABS_BELOW_H));
        assert_eq!(f.body.top, TITLEBAR_H + MENUBAR_H + TABS_BELOW_H);
        assert!(f.hints.is_empty());
        assert_eq!(f.body.bottom, 576.0);
    }

    #[test]
    fn merged_status_is_taller_and_hides_hints() {
        let f = frame(920.0, 600.0, Bands { hints: true, merged_status: true, ..Default::default() });
        assert_eq!(f.status.top, 574.0);
        assert!(f.hints.is_empty());
    }

    #[test]
    fn frame_frac_matches_frame_at_the_ends_and_retargets_from_the_middle() {
        let a = Bands { hints: true, ..Default::default() };
        let b = Bands { hints: true, menubar: true, ..Default::default() };
        assert_eq!(frame_frac(920.0, 600.0, BandFrac::of(a)), frame(920.0, 600.0, a));
        assert_eq!(frame_frac(920.0, 600.0, BandFrac::of(b)), frame(920.0, 600.0, b));
        let half = BandFrac::of(a).lerp(BandFrac::of(b), 0.5);
        assert_eq!(frame_frac(920.0, 600.0, half).menubar.height(), MENUBAR_H / 2.0);
        // Volver a `a` desde la mitad empieza en la mitad, no en `b`.
        assert_eq!(half.lerp(BandFrac::of(a), 0.0).menubar, 0.5);
    }

    #[test]
    fn tiny_window_never_inverts_rects() {
        let f = frame(100.0, 40.0, Bands { menubar: true, tabs_below: true, hints: true, ..Default::default() });
        assert!(f.body.height() >= 0.0);
        assert!(f.status.top >= f.body.top);
    }

    #[test]
    fn gutter_is_52_until_numbers_do_not_fit() {
        assert_eq!(gutter_width(13, 7.8), 52.0);
        assert_eq!(gutter_width(9999, 7.8), 52.0);
        assert!(gutter_width(100_000, 7.8) > 52.0);
    }

    fn slots(names: &[f32], dirty: bool) -> Vec<TabSlot> {
        names.iter().map(|&name_w| TabSlot { name_w, dirty, scale: 1.0 }).collect()
    }

    fn geoms(l: &TabsLayout) -> Vec<TabGeom> {
        l.tabs.iter().flatten().copied().collect()
    }

    #[test]
    fn tab_width_is_padding_plus_name_plus_close() {
        let l = tabs_fit(36.0, 34.0, 700.0, &slots(&[50.0], false), 0, 6.0);
        let t = geoms(&l);
        assert_eq!(t[0].rect, Rect::new(36.0, 6.0, 36.0 + 12.0 + 50.0 + 6.0 + 18.0 + 6.0, 34.0));
        assert_eq!(t[0].name_x, 48.0);
        assert_eq!(t[0].close.width(), 18.0);
        assert_eq!(t[0].close.right, t[0].rect.right - 6.0);
        assert!(t[0].dot_x.is_none());
        assert!(l.scroll_left.is_none() && l.scroll_right.is_none());
    }

    #[test]
    fn dirty_tab_reserves_room_for_the_dot() {
        let t = geoms(&tabs_fit(36.0, 34.0, 700.0, &slots(&[50.0], true), 0, 6.0));
        assert_eq!(t[0].rect.width(), 12.0 + 50.0 + 6.0 + 6.0 + 6.0 + 18.0 + 6.0);
        assert_eq!(t[0].dot_x, Some(48.0 + 50.0 + 6.0));
    }

    #[test]
    fn long_names_are_capped_at_190() {
        let t = geoms(&tabs_fit(36.0, 34.0, 700.0, &slots(&[400.0], false), 0, 6.0));
        assert_eq!(t[0].rect.width(), 190.0);
        assert_eq!(t[0].name_w, 190.0 - (12.0 + 6.0 + 18.0 + 6.0));
    }

    #[test]
    fn plus_follows_the_last_tab() {
        let l = tabs_fit(36.0, 34.0, 700.0, &slots(&[50.0, 50.0], false), 0, 6.0);
        let t = geoms(&l);
        assert_eq!(t[1].rect.left, t[0].rect.right + 2.0);
        assert_eq!(l.plus.left, t[1].rect.right + 2.0);
        assert_eq!(l.plus.height(), 24.0);
    }

    #[test]
    fn crowded_tabs_shrink_instead_of_disappearing() {
        let l = tabs_fit(36.0, 34.0, 500.0, &slots(&[150.0; 4], false), 0, 6.0);
        let t = geoms(&l);
        assert_eq!(t.len(), 4);
        assert!(t[0].rect.width() < TAB_MAX_W && t[0].rect.width() >= TAB_MIN_W);
        assert!(l.plus.right <= 500.0 + 0.01);
        assert!(l.scroll_left.is_none() && l.scroll_right.is_none());
    }

    #[test]
    fn overflowing_tabs_keep_the_focused_one_visible_and_stay_in_bounds() {
        let many = slots(&[150.0; 20], false);
        for focus in [0, 7, 19] {
            let l = tabs_fit(36.0, 34.0, 400.0, &many, focus, 6.0);
            assert!(l.tabs[focus].is_some(), "la pestaña {focus} debe verse");
            for g in geoms(&l) {
                assert!(g.rect.right <= 400.0 && g.rect.width() >= TAB_MIN_W - 0.01);
            }
            assert!(l.plus.right <= 400.0 + 0.01);
        }
        let l = tabs_fit(36.0, 34.0, 400.0, &many, 19, 6.0);
        assert!(l.scroll_left.is_some() && l.scroll_right.is_none());
        let l = tabs_fit(36.0, 34.0, 400.0, &many, 0, 6.0);
        assert!(l.scroll_left.is_none() && l.scroll_right.is_some());
    }

    #[test]
    fn zero_scale_tab_takes_no_room() {
        let mut s = slots(&[50.0, 50.0], false);
        s[0].scale = 0.0;
        let l = tabs_fit(36.0, 34.0, 700.0, &s, 1, 6.0);
        let t = geoms(&l);
        assert_eq!(t[0].rect.width(), 0.0);
        assert_eq!(t[1].rect.left, 36.0);
    }

    #[test]
    fn popups_are_clamped_inside_the_bounds() {
        let b = Rect::new(0.0, 0.0, 400.0, 300.0);
        assert_eq!(clamp_popup(10.0, 10.0, 100.0, 50.0, 10.0, b), Rect::new(10.0, 10.0, 110.0, 60.0));
        let r = clamp_popup(380.0, 280.0, 100.0, 50.0, 280.0, b);
        assert_eq!(r, Rect::new(300.0, 230.0, 400.0, 280.0));
        let r = clamp_popup(10.0, 20.0, 100.0, 500.0, 20.0, b);
        assert_eq!(r.top, 0.0);
    }
}
