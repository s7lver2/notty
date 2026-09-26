//! Ventana de Ajustes, dibujada con Direct2D igual que la principal (`docs/mockups/
//! notty-ui.html` → «Ajustes»): columna de navegación + panel con las filas de
//! `settings_model::sections`. Reutiliza `Renderer` (helpers de dibujo, formatos de
//! texto) y la misma técnica de barra de título propia que `window.rs`, simplificada
//! porque esta ventana no se redimensiona ni tiene min/max.

use std::cell::RefCell;
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW, GetWindowLongPtrW,
    GetWindowRect, HTCAPTION, HTCLIENT, IDC_ARROW, KillTimer, LoadCursorW, MSG, PostQuitMessage, RegisterClassExW,
    SW_SHOW, SWP_NOZORDER, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, WM_DESTROY,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCHITTEST, WM_PAINT, WM_TIMER, WNDCLASSEXW,
    WS_CLIPSIBLINGS, WS_POPUP, WS_VISIBLE,
};
use windows::core::{PCWSTR, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

use notty_config::Config;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::layout::Rect;
use crate::settings_model::{Row, SettingKey, SettingValue};
use crate::theme::{self, Rgba};

/// Geometría de un `Select` abierto, para dibujar su desplegable al final de `paint`
/// (igual que `pending_dropdown` en `render.rs`): fila, caja y sus opciones.
/// `(fila, caja, opciones, índice de la opción actualmente seleccionada)`.
type OpenSelectGeom = (usize, Rect, &'static [(&'static str, SettingValue)], usize);
use crate::{Renderer, is_dark};

const TITLEBAR_H: f32 = 36.0;
const WIN_W: f32 = 820.0;
const WIN_H: f32 = 620.0;
const NAV_W: f32 = 200.0;
const CLOSE_W: f32 = 46.0;

/// Id del `SetTimer` de animación de esta ventana (interruptores + fundido/escala de
/// apertura), igual que `ID_ANIM_TIMER` en `window.rs` pero local a esta ventana.
const ID_ANIM_TIMER: usize = 1;

/// Zonas de clic propias de esta ventana (no comparte `render::Hit` con la principal:
/// los controles son distintos y el desplegable de `.select` solo tiene sentido aquí).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Hit {
    #[default]
    None,
    Caption,
    Close,
    Nav(usize),
    EditConfig,
    /// Fila `row` (índice dentro de la sección activa), control `Link`.
    Link(usize),
    /// Fila `row` (índice dentro de la sección activa), control `Toggle`.
    Toggle(usize),
    /// Fila `row`, opción `opt` de un `Seg`.
    Seg(usize, usize),
    /// Abre/cierra el desplegable de la fila `row` (`Select`).
    SelectBox(usize),
    /// Opción `opt` del desplegable abierto.
    SelectOption(usize),
}

struct State {
    cfg: Rc<RefCell<Config>>,
    on_change: Box<dyn Fn()>,
    on_open_path: Box<dyn Fn(std::path::PathBuf)>,
    on_repeat_tutorial: Box<dyn Fn()>,
    renderer: Renderer,
    active_section: usize,
    hover: Hit,
    open_select: Option<usize>,
    hits: Vec<(Rect, Hit)>,
    /// Si las animaciones del sistema están activadas, ver `window::system_animations_enabled`.
    animations_enabled: bool,
    /// Animación en curso del fundido+escala de apertura de la ventana. `None` en
    /// reposo (se dibuja siempre a partir de entonces igual que antes de este plan).
    open_anim: Option<crate::Anim>,
    /// Animación en curso de cada `Toggle` que se acaba de alternar, indexada por su
    /// fila dentro de la sección activa: `(animación, estado 'on' de destino)`. Se
    /// limpia entera al cambiar de sección (Hit::Nav).
    toggle_anims: HashMap<usize, (crate::Anim, bool)>,
    /// Animación en curso del desplegable (`Select`) que se acaba de abrir (fundido +
    /// desplazamiento, mismo tratamiento que el menú/sugerencias de la ventana
    /// principal, ver Task 3 del plan). `None` en reposo.
    select_open_anim: Option<crate::Anim>,
    /// Si el `SetTimer` de animación (`ID_ANIM_TIMER`) está corriendo.
    anim_timer_running: bool,
}

/// Arranca el temporizador de animación (60Hz) de esta ventana si no estaba corriendo.
fn ensure_anim_timer(st: &mut State, hwnd: HWND) {
    if !st.anim_timer_running {
        unsafe {
            let _ = SetTimer(Some(hwnd), ID_ANIM_TIMER, 16, None);
        }
        st.anim_timer_running = true;
    }
}

/// Mezcla lineal componente a componente entre `a` y `b` (`t == 0.0` da `a`, `t == 1.0`
/// da `b`): usada para el fundido de color del interruptor deslizante.
fn lerp_color(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    Rgba(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t, a.3 + (b.3 - a.3) * t)
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Abre la ventana de Ajustes, centrada sobre `parent`. `on_change` se llama cada vez
/// que el usuario cambia algo (ya guardado en disco); `on_open_path` cuando pide abrir
/// `config.toml` (o los atajos) como documento en la ventana principal.
pub fn open(
    parent: HWND,
    cfg: Rc<RefCell<Config>>,
    on_change: Box<dyn Fn()>,
    on_open_path: Box<dyn Fn(std::path::PathBuf)>,
    on_repeat_tutorial: Box<dyn Fn()>,
) -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("NottySettingsClass");
        let icon = crate::window::app_icon(instance.into());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DROPSHADOW,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hIcon: icon.unwrap_or_default(),
            hIconSm: icon.unwrap_or_default(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let dpi = GetDpiForWindow(parent).max(96);
        let scale = dpi as f32 / 96.0;
        let (w_px, h_px) = ((WIN_W * scale) as i32, (WIN_H * scale) as i32);

        let mut parent_rect = RECT::default();
        let _ = GetWindowRect(parent, &mut parent_rect);
        let x = parent_rect.left + ((parent_rect.right - parent_rect.left) - w_px) / 2;
        let y = parent_rect.top + ((parent_rect.bottom - parent_rect.top) - h_px) / 2;

        let title = to_wide("Ajustes · notty");
        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_CLIPSIBLINGS | WS_VISIBLE,
            x,
            y,
            w_px,
            h_px,
            Some(parent),
            None,
            Some(instance.into()),
            None,
        )?;

        let dark = is_dark(cfg.borrow().ui.theme, crate::window::system_uses_dark_mode());
        let prefer_round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &prefer_round as *const _ as *const _,
            std::mem::size_of_val(&prefer_round) as u32,
        );
        let margins =
            windows::Win32::UI::Controls::MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 1, cyBottomHeight: 0 };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        crate::window::apply_dark_mode(hwnd, dark);

        let renderer = Renderer::new(hwnd, dpi)?;
        let animations_enabled = crate::window::system_animations_enabled();
        let state = Box::new(State {
            cfg,
            on_change,
            on_open_path,
            on_repeat_tutorial,
            renderer,
            active_section: 0,
            hover: Hit::None,
            open_select: None,
            hits: Vec::new(),
            animations_enabled,
            open_anim: Some(crate::Anim::new_maybe(Instant::now(), Duration::from_millis(150), animations_enabled)),
            toggle_anims: HashMap::new(),
            select_open_anim: None,
            anim_timer_running: false,
        });
        let ptr = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
        if let Some(st) = ptr.as_mut() {
            ensure_anim_timer(st, hwnd);
        }

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetWindowPos(hwnd, None, x, y, w_px, h_px, SWP_NOZORDER);

        // Bucle de mensajes propio: bloquea hasta que se cierra Ajustes, igual que un
        // diálogo modal (la ventana principal no vuelve a procesar mensajes mientras
        // tanto, que es justo lo que la maqueta espera de "Ajustes" como panel modal).
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
            if ptr.is_null() {
                break;
            }
        }
    }
    Ok(())
}

fn point_from_lparam(lparam: LPARAM) -> (f32, f32) {
    let x = (lparam.0 as i16) as f32;
    let y = ((lparam.0 >> 16) as i16) as f32;
    (x, y)
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        match msg {
            WM_PAINT => {
                if let Some(st) = ptr.as_mut() {
                    paint(st);
                }
                let _ = windows::Win32::Graphics::Gdi::ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let def = DefWindowProcW(hwnd, msg, wparam, lparam);
                if def.0 as u32 != HTCLIENT {
                    return def;
                }
                if let Some(st) = ptr.as_mut() {
                    let x = (lparam.0 as i16) as i32;
                    let y = ((lparam.0 >> 16) as i16) as i32;
                    let mut pt = windows::Win32::Foundation::POINT { x, y };
                    let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
                    let scale = st.renderer.scale();
                    let hit = hit_test(st, pt.x as f32 / scale, pt.y as f32 / scale);
                    if hit == Hit::Caption {
                        return LRESULT(HTCAPTION as isize);
                    }
                }
                LRESULT(HTCLIENT as isize)
            }
            WM_LBUTTONDOWN => {
                if let Some(st) = ptr.as_mut() {
                    let (x, y) = point_from_lparam(lparam);
                    let scale = st.renderer.scale();
                    let (x, y) = (x / scale, y / scale);
                    handle_click(hwnd, st, x, y);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if let Some(st) = ptr.as_mut() {
                    let (x, y) = point_from_lparam(lparam);
                    let scale = st.renderer.scale();
                    let hit = hit_test(st, x / scale, y / scale);
                    if hit != st.hover {
                        st.hover = hit;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let _ = ReleaseCapture();
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if wparam.0 as u32 == 0x1B {
                    // Esc: cierra el desplegable si hay uno, si no cierra Ajustes.
                    if let Some(st) = ptr.as_mut() {
                        if st.open_select.take().is_some() {
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                    }
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        Some(hwnd),
                        windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == ID_ANIM_TIMER {
                    if let Some(st) = ptr.as_mut() {
                        let now = Instant::now();
                        if st.open_anim.is_some_and(|a| a.is_done(now)) {
                            st.open_anim = None;
                        }
                        st.toggle_anims.retain(|_, (a, _)| !a.is_done(now));
                        if st.select_open_anim.is_some_and(|a| a.is_done(now)) {
                            st.select_open_anim = None;
                        }
                        let still_animating =
                            st.open_anim.is_some() || !st.toggle_anims.is_empty() || st.select_open_anim.is_some();
                        if !still_animating {
                            let _ = KillTimer(Some(hwnd), ID_ANIM_TIMER);
                            st.anim_timer_running = false;
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_DESTROY => {
                let _ = KillTimer(Some(hwnd), ID_ANIM_TIMER);
                if !ptr.is_null() {
                    drop(Box::from_raw(ptr));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn hit_test(st: &State, x: f32, y: f32) -> Hit {
    for &(r, h) in st.hits.iter().rev() {
        if r.contains(x, y) {
            return h;
        }
    }
    if y < TITLEBAR_H { Hit::Caption } else { Hit::None }
}

fn handle_click(hwnd: HWND, st: &mut State, x: f32, y: f32) {
    let hit = hit_test(st, x, y);
    match hit {
        Hit::Close => unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                Some(hwnd),
                windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        },
        Hit::Nav(i) => {
            st.active_section = i;
            st.open_select = None;
            st.toggle_anims.clear();
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
        Hit::EditConfig => {
            (st.on_open_path)(notty_config::default_path());
        }
        Hit::Link(row) => {
            let action = crate::settings_model::sections(&st.cfg.borrow())[st.active_section].rows.get(row).and_then(
                |r| match r {
                    Row::Link { action, .. } => Some(*action),
                    _ => None,
                },
            );
            match action {
                Some(crate::settings_model::LinkAction::OpenKeys) => (st.on_open_path)(notty_config::default_path()),
                Some(crate::settings_model::LinkAction::RepeatTutorial) => {
                    (st.on_repeat_tutorial)();
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                            Some(hwnd),
                            windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                            WPARAM(0),
                            LPARAM(0),
                        );
                    }
                }
                None => {}
            }
        }
        Hit::Toggle(row) => {
            toggle_row(st, row, hwnd);
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
        Hit::Seg(row, opt) => {
            set_row_option(st, row, opt);
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
        Hit::SelectBox(row) => {
            st.open_select = if st.open_select == Some(row) { None } else { Some(row) };
            if st.open_select.is_some() {
                st.select_open_anim =
                    Some(crate::Anim::new_maybe(Instant::now(), Duration::from_millis(120), st.animations_enabled));
                ensure_anim_timer(st, hwnd);
            }
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
        Hit::SelectOption(opt) => {
            if let Some(row) = st.open_select.take() {
                set_row_option(st, row, opt);
            }
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
        Hit::None | Hit::Caption => {
            if st.open_select.take().is_some() {
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
        }
    }
}

/// Invierte el `Toggle` de la fila `row` (de la sección activa) y arranca su animación
/// de deslizamiento.
fn toggle_row(st: &mut State, row: usize, hwnd: HWND) {
    let Some(Row::Toggle { key, .. }) = crate::settings_model::sections(&st.cfg.borrow())[st.active_section]
        .rows
        .get(row)
        .copied()
    else {
        return;
    };
    let target_on = !current_bool(&st.cfg.borrow(), key);
    crate::settings_model::apply(&mut st.cfg.borrow_mut(), key, SettingValue::Bool(target_on));
    save_and_notify(st);
    st.toggle_anims.insert(
        row,
        (crate::Anim::new_maybe(Instant::now(), Duration::from_millis(140), st.animations_enabled), target_on),
    );
    ensure_anim_timer(st, hwnd);
}

fn current_bool(cfg: &Config, key: SettingKey) -> bool {
    match key {
        SettingKey::LineNumbers => cfg.ui.line_numbers,
        SettingKey::HintsBar => cfg.ui.hints_bar,
        SettingKey::StatusBar => cfg.ui.status_bar,
        SettingKey::MergedCommandLine => cfg.ui.merged_command_line,
        SettingKey::VimAlways => cfg.ui.vim_always,
        _ => false,
    }
}

/// Fija la fila `row` (de la sección activa) a su opción `opt` (usado por `Seg`,
/// `Select` y su desplegable): aplica el `SettingValue` de esa opción con
/// `settings_model::apply`.
fn set_row_option(st: &mut State, row: usize, opt: usize) {
    let row_def = crate::settings_model::sections(&st.cfg.borrow())[st.active_section].rows.get(row).copied();
    let (key, value) = match row_def {
        Some(Row::Seg { key, options, .. }) | Some(Row::Select { key, options, .. }) => match options.get(opt) {
            Some((_, v)) => (key, *v),
            None => return,
        },
        _ => return,
    };
    crate::settings_model::apply(&mut st.cfg.borrow_mut(), key, value);
    save_and_notify(st);
}

fn save_and_notify(st: &State) {
    let _ = notty_config::save(&st.cfg.borrow(), &notty_config::default_path());
    (st.on_change)();
}

fn selected_option_index(cfg: &Config, key: SettingKey, options: &[(&str, SettingValue)]) -> Option<usize> {
    let current: SettingValue = match key {
        SettingKey::Preset => SettingValue::Preset(cfg.ui.preset),
        SettingKey::Theme => SettingValue::Theme(cfg.ui.theme),
        SettingKey::Files => SettingValue::Files(cfg.ui.files),
        SettingKey::TabsPosition => SettingValue::TabsPosition(cfg.ui.tabs_position),
        SettingKey::MenuBar => SettingValue::MenuBar(cfg.ui.menubar),
        _ => return None,
    };
    options.iter().position(|(_, v)| *v == current)
}

// --- Dibujo ---------------------------------------------------------------------------

fn paint(st: &mut State) {
    st.hits.clear();
    let dark = is_dark(st.cfg.borrow().ui.theme, crate::window::system_uses_dark_mode());
    let pal = theme::palette(dark);
    let (w, h) = st.renderer.size_dips();
    let r = &st.renderer;

    r.begin_paint(pal.chrome);

    // Fundido de 0 a 1 y escala de 97% a 100% al abrir la ventana (Task 5 del plan de
    // animaciones): `t == None` (animación terminada o desactivada) dibuja exactamente
    // igual que antes, sin coste extra. El fondo ya se acaba de pintar opaco arriba
    // (`Clear`, ajeno al `fade`); lo que sigue son los controles/texto, cuya opacidad
    // si se funde ya blend-ea visualmente contra ese fondo opaco.
    let open_t = st.open_anim.map(|a| a.value(std::time::Instant::now(), 0.0, 1.0));
    if let Some(t) = open_t {
        let scale = 0.97 + 0.03 * t;
        r.set_transform(Matrix3x2::scale_around(scale, scale, Vector2 { X: w / 2.0, Y: h / 2.0 }));
        r.set_fade(t);
    }

    // Barra de título: icono + "Ajustes · notty" + botón cerrar.
    let titlebar = Rect::new(0.0, 0.0, w, TITLEBAR_H);
    r.fill(titlebar, pal.chrome);
    r.fill_round(Rect::new(11.0, 11.0, 25.0, 25.0), 3.0, pal.accent);
    r.text("Ajustes · notty", &r.fonts().ui_12, Rect::new(32.0, 0.0, w - CLOSE_W, TITLEBAR_H), pal.text_2);
    let close_r = Rect::new(w - CLOSE_W, 0.0, w, TITLEBAR_H);
    if st.hover == Hit::Close {
        r.fill(close_r, pal.close_hover);
    }
    let cc = if st.hover == Hit::Close { pal.close_hover_fg } else { pal.text_2 };
    let ccx = close_r.left + close_r.width() / 2.0;
    let ccy = close_r.top + close_r.height() / 2.0;
    r.stroke_line(ccx - 5.0, ccy - 5.0, ccx + 5.0, ccy + 5.0, 1.0, cc);
    r.stroke_line(ccx + 5.0, ccy - 5.0, ccx - 5.0, ccy + 5.0, 1.0, cc);
    st.hits.push((titlebar, Hit::Caption));
    st.hits.push((close_r, Hit::Close));

    // Columna de navegación.
    let nav = Rect::new(0.0, TITLEBAR_H, NAV_W, h);
    r.fill(nav, pal.chrome);
    let sections = crate::settings_model::sections(&st.cfg.borrow());
    let mut ny = nav.top + 6.0;
    for (i, sec) in sections.iter().enumerate() {
        let br = Rect::new(nav.left + 8.0, ny, nav.right - 8.0, ny + 34.0);
        let active = i == st.active_section;
        if active || st.hover == Hit::Nav(i) {
            r.fill_round(br, 6.0, pal.hover);
        }
        if active {
            r.fill_round(Rect::new(br.left, br.top + 9.0, br.left + 3.0, br.bottom - 9.0), 1.5, pal.accent);
        }
        let tc = if active { pal.text } else { pal.text_2 };
        r.text(sec.name, &r.fonts().ui_13, Rect::new(br.left + 12.0, br.top, br.right - 8.0, br.bottom), tc);
        st.hits.push((br, Hit::Nav(i)));
        ny += 36.0;
    }
    // Pie: "Todo se guarda en config.toml." + "Editar el archivo".
    let foot_h = 44.0;
    let foot = Rect::new(nav.left + 10.0, nav.bottom - foot_h, nav.right - 10.0, nav.bottom - 12.0);
    let prefix = "Todo se guarda en ";
    let prefix_w = r.measure(prefix, &r.fonts().ui_11_5);
    r.text(prefix, &r.fonts().ui_11_5, Rect::new(foot.left, foot.top, foot.left + prefix_w, foot.top + 18.0), pal.text_3);
    let bold_w = r.measure("config.toml", &r.fonts().ui_11_5_semibold);
    r.text(
        "config.toml",
        &r.fonts().ui_11_5_semibold,
        Rect::new(foot.left + prefix_w, foot.top, foot.left + prefix_w + bold_w, foot.top + 18.0),
        pal.text_3,
    );
    r.text(".", &r.fonts().ui_11_5, Rect::new(foot.left + prefix_w + bold_w, foot.top, foot.right, foot.top + 18.0), pal.text_3);
    let link_r = Rect::new(foot.left, foot.top + 18.0, foot.right, foot.top + 36.0);
    r.text("Editar el archivo", &r.fonts().ui_11_5, link_r, pal.accent);
    if st.hover == Hit::EditConfig {
        // La maqueta subraya estos enlaces al pasar el ratón (`.snav .foot a:hover`),
        // en vez de dejar el color de acento como único estado (Task 7 del plan).
        let lw = r.measure("Editar el archivo", &r.fonts().ui_11_5);
        let uy = link_r.bottom - 3.0;
        r.stroke_line(link_r.left, uy, link_r.left + lw, uy, 1.0, pal.accent);
    }
    st.hits.push((link_r, Hit::EditConfig));

    // Panel de la sección activa.
    let panel = Rect::new(NAV_W, TITLEBAR_H, w, h);
    r.fill(panel, pal.surface);
    let section = &sections[st.active_section];
    let title_r = Rect::new(panel.left + 22.0, panel.top + 28.0, panel.right - 22.0, panel.top + 28.0 + 28.0);
    let title_w = r.measure(section.name, &r.fonts().ui_20_semibold);
    r.text(section.name, &r.fonts().ui_20_semibold, title_r, pal.text);
    if section.id == "apariencia" && st.cfg.borrow().ui.preset == notty_config::Preset::Custom {
        let suffix = " · Personalizado";
        r.text(suffix, &r.fonts().ui_12, Rect::new(title_r.left + title_w, title_r.top, title_r.right, title_r.bottom), pal.text_3);
    }

    let mut ry = title_r.bottom + 14.0;
    let row_left = panel.left + 22.0;
    let row_right = panel.right - 22.0;
    let mut open_select_geom: Option<OpenSelectGeom> = None;

    for (i, row) in section.rows.iter().enumerate() {
        match row {
            Row::Group(label) => {
                r.text(label, &r.fonts().ui_12, Rect::new(row_left, ry, row_right, ry + 20.0), pal.text_2);
                ry += 24.0;
                continue;
            }
            Row::Toggle { title, desc, key } => {
                let rr = Rect::new(row_left, ry, row_right, ry + row_height(desc));
                r.fill_round(rr, 6.0, pal.surface_2);
                draw_row_text(r, rr, title, desc, pal);
                let on = current_bool(&st.cfg.borrow(), *key);
                let track = Rect::new(rr.right - 12.0 - 40.0, rr.top + (rr.height() - 20.0) / 2.0, rr.right - 12.0, rr.top + (rr.height() - 20.0) / 2.0 + 20.0);
                let anim = st.toggle_anims.get(&i).copied();
                draw_toggle(r, track, on, pal, anim);
                st.hits.push((rr, Hit::Toggle(i)));
                ry += rr.height() + 3.0;
            }
            Row::Seg { title, desc, key, options } => {
                let rr = Rect::new(row_left, ry, row_right, ry + row_height(desc));
                r.fill_round(rr, 6.0, pal.surface_2);
                draw_row_text(r, rr, title, desc, pal);
                let selected = selected_option_index(&st.cfg.borrow(), *key, options).unwrap_or(usize::MAX);
                let seg_r = draw_seg(r, rr, options, selected, pal, st.hover, i);
                for (j, opt_r) in seg_r.iter().enumerate() {
                    st.hits.push((*opt_r, Hit::Seg(i, j)));
                }
                ry += rr.height() + 3.0;
            }
            Row::Select { title, desc, key, options } => {
                let rr = Rect::new(row_left, ry, row_right, ry + row_height(desc));
                r.fill_round(rr, 6.0, pal.surface_2);
                draw_row_text(r, rr, title, desc, pal);
                let selected = selected_option_index(&st.cfg.borrow(), *key, options).unwrap_or(0);
                let label = options.get(selected).map(|(l, _)| *l).unwrap_or("");
                let box_w = 200.0f32.max(r.measure(label, &r.fonts().ui_12_5) + 40.0);
                let box_r = Rect::new(rr.right - 12.0 - box_w, rr.top + (rr.height() - 30.0) / 2.0, rr.right - 12.0, rr.top + (rr.height() - 30.0) / 2.0 + 30.0);
                r.fill(box_r, pal.surface);
                r.stroke_rect(box_r, 1.0, pal.line);
                r.text(label, &r.fonts().ui_12_5, Rect::new(box_r.left + 8.0, box_r.top, box_r.right - 20.0, box_r.bottom), pal.text);
                r.text("˅", &r.fonts().ui_12_5, Rect::new(box_r.right - 20.0, box_r.top, box_r.right - 6.0, box_r.bottom), pal.text_2);
                st.hits.push((box_r, Hit::SelectBox(i)));
                if st.open_select == Some(i) {
                    open_select_geom = Some((i, box_r, options, selected));
                }
                ry += rr.height() + 3.0;
            }
            Row::Kbd { title, keys } => {
                let rr = Rect::new(row_left, ry, row_right, ry + row_height(""));
                r.fill_round(rr, 6.0, pal.surface_2);
                draw_row_text(r, rr, title, "", pal);
                let kw = r.measure(keys, &r.fonts().mono_11) + 10.0;
                let kr = Rect::new(rr.right - 12.0 - kw, rr.top + (rr.height() - 20.0) / 2.0, rr.right - 12.0, rr.top + (rr.height() - 20.0) / 2.0 + 20.0);
                r.fill_round(kr, 4.0, pal.hover);
                // Igual que en el segmentado: `kw` ya lleva 5px de relleno a cada lado.
                r.text(keys, &r.fonts().mono_11, Rect::new(kr.left + 5.0, kr.top, kr.right - 5.0, kr.bottom), pal.text_2);
                ry += rr.height() + 3.0;
            }
            Row::Link { title, desc, label, .. } => {
                let rr = Rect::new(row_left, ry, row_right, ry + row_height(desc));
                r.fill_round(rr, 6.0, pal.surface_2);
                draw_row_text(r, rr, title, desc, pal);
                let lw = r.measure(label, &r.fonts().ui_13);
                let lr = Rect::new(rr.right - 12.0 - lw, rr.top, rr.right - 12.0, rr.bottom);
                r.text(label, &r.fonts().ui_13, lr, pal.accent);
                if st.hover == Hit::Link(i) {
                    // Mismo subrayado al pasar el ratón que el pie de la navegación.
                    let uy = (lr.top + lr.bottom) / 2.0 + 8.0;
                    r.stroke_line(lr.left, uy, lr.right, uy, 1.0, pal.accent);
                }
                st.hits.push((rr, Hit::Link(i)));
                ry += rr.height() + 3.0;
            }
        }
    }

    if let Some((row_idx, box_r, options, selected)) = open_select_geom {
        // Fundido + 4px de desplazamiento al abrirse, mismo tratamiento que el menú y
        // las sugerencias de la ventana principal (Task 3 del plan de animaciones).
        let st_t = st.select_open_anim.map(|a| a.value(Instant::now(), 0.0, 1.0)).unwrap_or(1.0);
        let dy = (1.0 - st_t) * 4.0;
        let draw_y = |y: f32| y - dy;

        let row_h = 26.0;
        let dd_h = row_h * options.len() as f32 + 8.0;
        // Posición final (sin desplazar): la usada para el hit-testing, que no anima.
        let dd = Rect::new(box_r.left, box_r.bottom + 2.0, box_r.right, box_r.bottom + 2.0 + dd_h);
        let draw_dd = Rect::new(dd.left, draw_y(dd.top), dd.right, draw_y(dd.bottom));
        r.draw_popup_shadow(draw_dd, 8.0, st_t, pal.shadow);
        r.fill_round(draw_dd, 8.0, pal.chrome_hi.faded(st_t));
        r.stroke_round_rect(draw_dd, 8.0, 1.0, pal.shadow_ring.faded(st_t));
        let mut oy = dd.top + 4.0;
        for (j, (label, _)) in options.iter().enumerate() {
            let or_ = Rect::new(dd.left + 4.0, oy, dd.right - 4.0, oy + row_h);
            let draw_or = Rect::new(draw_dd.left + 4.0, draw_y(oy), draw_dd.right - 4.0, draw_y(oy + row_h));
            // La opción actualmente seleccionada se marca (fondo accent_soft + texto
            // accent) aunque el ratón esté sobre otra, igual que `.prow.sel` en las
            // sugerencias de ruta (Task 6 del plan de animaciones/pulido).
            if j == selected {
                r.fill_round(draw_or, 4.0, pal.accent_soft.faded(st_t));
            } else if st.hover == Hit::SelectOption(j) {
                r.fill_round(draw_or, 4.0, pal.hover.faded(st_t));
            }
            let label_c = if j == selected { pal.accent } else { pal.text };
            r.text(
                label,
                &r.fonts().mono_12,
                Rect::new(draw_or.left + 8.0, draw_or.top, draw_or.right - 8.0, draw_or.bottom),
                label_c.faded(st_t),
            );
            st.hits.push((or_, Hit::SelectOption(j)));
            oy += row_h;
        }
        let _ = row_idx;
    }

    if open_t.is_some() {
        r.reset_transform();
        r.set_fade(1.0);
    }

    r.end_paint();
}

/// Alto de una fila (`.srow`): 14 de relleno arriba y abajo + el título (17) y, si
/// hay descripción, otra línea (16) más 3 de separación entre título y descripción.
fn row_height(desc: &str) -> f32 {
    if desc.is_empty() { 14.0 * 2.0 + 17.0 } else { 14.0 * 2.0 + 17.0 + 3.0 + 16.0 }
}

fn draw_row_text(r: &Renderer, rr: Rect, title: &str, desc: &str, pal: &theme::Palette) {
    let title_top = rr.top + 14.0;
    r.text(title, &r.fonts().ui_13, Rect::new(rr.left + 12.0, title_top, rr.left + 12.0 + 340.0, title_top + 17.0), pal.text);
    if !desc.is_empty() {
        let desc_top = title_top + 17.0 + 3.0;
        r.text(desc, &r.fonts().ui_11_5, Rect::new(rr.left + 12.0, desc_top, rr.left + 12.0 + 360.0, desc_top + 16.0), pal.text_3);
    }
}

/// `.toggle`: pista 40x20 radio 10; apagada = borde `text_2` y bolita `text_2` a la
/// izquierda; encendida = fondo `accent` y bolita `on_accent` desplazada a la derecha.
/// Si `anim` es `Some((a, target_on))`, la bolita desliza y la pista funde entre los
/// dos estados en vez de saltar directamente al destino (Task 5 del plan de
/// animaciones); `t` (`0.0` = apagado, `1.0` = encendido) resume el progreso.
fn draw_toggle(r: &Renderer, track: Rect, on: bool, pal: &theme::Palette, anim: Option<(crate::Anim, bool)>) {
    let t = match anim {
        Some((a, target_on)) => {
            let raw = a.value(Instant::now(), 0.0, 1.0);
            if target_on { raw } else { 1.0 - raw }
        }
        None => {
            if on {
                1.0
            } else {
                0.0
            }
        }
    };
    r.fill_round(track, 10.0, pal.accent.faded(t));
    r.stroke_round_rect(track, 10.0, 1.0, pal.text_2.faded(1.0 - t));
    let ball_d = 10.0;
    let bx = track.left + 5.0 + (20.0 - 5.0) * t;
    let by = track.top + (track.height() - ball_d) / 2.0;
    let ball_c = lerp_color(pal.text_2, pal.on_accent, t);
    r.fill_round(Rect::new(bx, by, bx + ball_d, by + ball_d), ball_d / 2.0, ball_c);
}

/// `.seg`: contenedor `hover` radio 6 con las opciones dentro; devuelve el rectángulo
/// de cada opción (para el hit-testing).
fn draw_seg(
    r: &Renderer,
    rr: Rect,
    options: &[(&str, SettingValue)],
    selected: usize,
    pal: &theme::Palette,
    hover: Hit,
    row_idx: usize,
) -> Vec<Rect> {
    let widths: Vec<f32> = options.iter().map(|(label, _)| r.measure(label, &r.fonts().ui_12) + 18.0).collect();
    let total_w: f32 = widths.iter().sum::<f32>() + 4.0;
    let seg_r = Rect::new(rr.right - 12.0 - total_w, rr.top + (rr.height() - 28.0) / 2.0, rr.right - 12.0, rr.top + (rr.height() - 28.0) / 2.0 + 28.0);
    r.fill_round(seg_r, 6.0, pal.hover);
    let mut x = seg_r.left + 2.0;
    let mut out = Vec::with_capacity(options.len());
    for (j, (label, _)) in options.iter().enumerate() {
        let w = widths[j];
        let opt_r = Rect::new(x, seg_r.top + 2.0, x + w, seg_r.bottom - 2.0);
        if j == selected {
            r.fill(opt_r, pal.surface);
            r.stroke_rect(opt_r, 1.0, pal.line);
        } else if hover == Hit::Seg(row_idx, j) {
            r.fill_round(opt_r, 4.0, pal.press);
        }
        let tc = if j == selected { pal.text } else { pal.text_2 };
        // `w` ya lleva 9px de relleno a cada lado (ver `widths` arriba): dibujar con
        // `opt_r` entero deja el texto pegado al borde izquierdo y todo el relleno
        // amontonado a la derecha, en vez de centrado en la pastilla.
        let label_r = Rect::new(opt_r.left + 9.0, opt_r.top, opt_r.right - 9.0, opt_r.bottom);
        r.text(label, &r.fonts().ui_12, label_r, tc);
        out.push(opt_r);
        x += w;
    }
    out
}
