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

pub fn frame(w: f32, h: f32, bands: Bands) -> Frame {
    let titlebar = Rect::new(0.0, 0.0, w, TITLEBAR_H);
    let close = Rect::new(w - CAPTION_BTN_W, 0.0, w, TITLEBAR_H);
    let max = Rect::new(close.left - CAPTION_BTN_W, 0.0, close.left, TITLEBAR_H);
    let min = Rect::new(max.left - CAPTION_BTN_W, 0.0, max.left, TITLEBAR_H);
    let settings_btn = Rect::new(min.left - SETTINGS_BTN_W, 0.0, min.left, TITLEBAR_H);

    let mut y = TITLEBAR_H;
    let menubar = if bands.menubar {
        let r = Rect::new(0.0, y, w, y + MENUBAR_H);
        y += MENUBAR_H;
        r
    } else {
        Rect::default()
    };
    let tabs_below = if bands.tabs_below {
        let r = Rect::new(0.0, y, w, y + TABS_BELOW_H);
        y += TABS_BELOW_H;
        r
    } else {
        Rect::default()
    };

    let status_h = if bands.merged_status { STATUS_MERGED_H } else { STATUS_H };
    let status = Rect::new(0.0, (h - status_h).max(y), w, h);
    let hints = if bands.hints && !bands.merged_status {
        Rect::new(0.0, (status.top - HINTS_H).max(y), w, status.top)
    } else {
        Rect::default()
    };
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
pub fn visible_lines(body: Rect) -> usize {
    (((body.height() - TEXT_PAD_T).max(0.0)) / LINE_H).floor().max(1.0) as usize
}

/// Geometría de una pestaña ya colocada.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabGeom {
    pub rect: Rect,
    /// Dónde empieza el nombre y cuánto ancho tiene disponible (se recorta con «…»).
    pub name_x: f32,
    pub name_w: f32,
    /// Posición del punto ● de «sin guardar», si lo hay.
    pub dot_x: Option<f32>,
    pub close: Rect,
}

/// Coloca las pestañas de izquierda a derecha empezando en `x0`, con su borde
/// inferior en `bottom`. `name_w[i]` es el ancho medido del nombre, `dirty[i]` si
/// lleva ●, `dot_w` el ancho medido de «●». Se detiene (sin dibujar a medias) al
/// llegar a `max_right`. Devuelve también el rectángulo del botón `+`.
pub fn tabs(x0: f32, bottom: f32, max_right: f32, name_w: &[f32], dirty: &[bool], dot_w: f32) -> (Vec<TabGeom>, Rect) {
    let mut out = Vec::with_capacity(name_w.len());
    let mut x = x0;
    let top = bottom - TAB_H;
    for (i, &nw) in name_w.iter().enumerate() {
        let dot_extra = if dirty.get(i).copied().unwrap_or(false) { TAB_INNER_GAP + dot_w } else { 0.0 };
        let fixed = TAB_PAD_L + dot_extra + TAB_INNER_GAP + TAB_CLOSE + TAB_PAD_R;
        let width = (fixed + nw).min(TAB_MAX_W);
        if x + width > max_right {
            break;
        }
        let rect = Rect::new(x, top, x + width, bottom);
        let close_left = rect.right - TAB_PAD_R - TAB_CLOSE;
        let close_top = top + (TAB_H - TAB_CLOSE) / 2.0;
        let close = Rect::new(close_left, close_top, close_left + TAB_CLOSE, close_top + TAB_CLOSE);
        let name_x = rect.left + TAB_PAD_L;
        let name_w = (width - fixed).max(0.0);
        let dot_x = if dot_extra > 0.0 { Some(name_x + name_w + TAB_INNER_GAP) } else { None };
        out.push(TabGeom { rect, name_x, name_w, dot_x, close });
        x = rect.right + TAB_GAP;
    }
    let plus_left = (x - TAB_GAP) + PLUS_ML;
    // .plus{align-self:center;margin-top:2px}: centrado en la fila de 34 px, 1 px más abajo.
    let row_top = bottom - TITLEBAR_H.max(TAB_H);
    let plus_top = row_top + (TITLEBAR_H - PLUS_H) / 2.0 + 1.0;
    let plus = Rect::new(plus_left, plus_top, plus_left + PLUS_W, plus_top + PLUS_H);
    (out, plus)
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

    #[test]
    fn tab_width_is_padding_plus_name_plus_close() {
        let (t, _) = tabs(36.0, 34.0, 700.0, &[50.0], &[false], 6.0);
        assert_eq!(t[0].rect, Rect::new(36.0, 6.0, 36.0 + 12.0 + 50.0 + 6.0 + 18.0 + 6.0, 34.0));
        assert_eq!(t[0].name_x, 48.0);
        assert_eq!(t[0].close.width(), 18.0);
        assert_eq!(t[0].close.right, t[0].rect.right - 6.0);
        assert!(t[0].dot_x.is_none());
    }

    #[test]
    fn dirty_tab_reserves_room_for_the_dot() {
        let (t, _) = tabs(36.0, 34.0, 700.0, &[50.0], &[true], 6.0);
        assert_eq!(t[0].rect.width(), 12.0 + 50.0 + 6.0 + 6.0 + 6.0 + 18.0 + 6.0);
        assert_eq!(t[0].dot_x, Some(48.0 + 50.0 + 6.0));
    }

    #[test]
    fn long_names_are_capped_at_190() {
        let (t, _) = tabs(36.0, 34.0, 700.0, &[400.0], &[false], 6.0);
        assert_eq!(t[0].rect.width(), 190.0);
        assert_eq!(t[0].name_w, 190.0 - (12.0 + 6.0 + 18.0 + 6.0));
    }

    #[test]
    fn tabs_stop_before_max_right_and_plus_follows_last() {
        let (t, plus) = tabs(36.0, 34.0, 250.0, &[50.0, 50.0, 50.0], &[false; 3], 6.0);
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].rect.left, t[0].rect.right + 2.0);
        assert_eq!(plus.left, t[1].rect.right + 2.0);
        assert_eq!(plus.height(), 24.0);
    }
}
