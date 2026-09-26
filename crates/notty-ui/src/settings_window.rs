//! Ventana de Ajustes (maqueta `Prototipo.dc.html`, dirección A), dibujada con
//! Direct2D igual que la principal: barra lateral de iconos que se despliega por
//! encima del contenido, páginas con vistas previas vivas y subpáginas con miga de
//! pan. Aquí vive la ventana (mensajes, estado, clics, teclado, la captura de
//! atajos); el contenido de cada página está en `settings_pages` y los controles en
//! `settings_ui`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow, ScreenToClient,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetClientRect, GetMessageW,
    GetWindowLongPtrW, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT,
    HTTOP, HTTOPLEFT, HTTOPRIGHT, IDC_ARROW, IsIconic, IsZoomed, KillTimer, LoadCursorW, MINMAXINFO, MSG,
    NCCALCSIZE_PARAMS, PostMessageW, PostQuitMessage, RegisterClassExW, SC_KEYMENU, SM_CXFRAME, SM_CXPADDEDBORDER, SW_RESTORE,
    SW_SHOW, SWP_FRAMECHANGED, SWP_NOZORDER, SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, TranslateMessage, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_GETMINMAXINFO, WM_KEYDOWN,
    WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE,
    WM_NCHITTEST, WM_PAINT, WM_SETTINGCHANGE, WM_SIZE, WM_SYSCHAR, WM_SYSCOMMAND, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_QUIT, WM_TIMER, WM_XBUTTONDOWN, WNDCLASSEXW, WS_CLIPSIBLINGS, WS_POPUP, WS_THICKFRAME,
};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::core::{PCWSTR, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

use notty_config::{Config, FontFamily};
use notty_input::Command;

use crate::anim::{Curve, Tweens};
use crate::keyboard_widget::{self, GREEN};
use crate::layout::Rect;
use crate::settings_model::{self as model, LinkAction, Page, SettingKey, SettingValue};
use crate::settings_pages::{self as pages, Inputs, PageData, Preview};
use crate::settings_ui::{FormatCache, Hit, InputId, TK, Ui, icon};
use crate::theme::{self, Rgba};
use crate::{Renderer, is_dark};

pub use crate::settings_pages::UpdateInfo;

const TITLEBAR_H: f32 = 32.0;
const WIN_W: f32 = 820.0;
const WIN_H: f32 = 620.0;
const MIN_W: f32 = 560.0;
const MIN_H: f32 = 420.0;
const CLOSE_W: f32 = 46.0;
const RAIL_W: f32 = 52.0;
const RAIL_OPEN_W: f32 = 208.0;
const RESIZE_BORDER: f32 = 6.0;
const WHEEL_STEP: f32 = 48.0;

const ID_ANIM_TIMER: usize = 1;
const CAPTURE_IN_MS: u64 = 240;
const CAPTURE_OUT_MS: u64 = 140;
const CONFIRM_MS: u64 = 220;
const THEME_MS: u64 = 350;
/// Lo que dura como mucho la entrada escalonada de una página.
const ENTER_MS: u64 = 800;

/// Capa de "pulsa la nueva combinación" para un atajo de Teclado.
#[derive(Debug, Clone, Copy)]
struct Capture {
    cmd: Command,
    opened: Instant,
    closing: Option<Instant>,
    captured: Option<(u32, notty_input::Modifiers)>,
    captured_at: Instant,
    error: Option<&'static str>,
}

struct State {
    cfg: Rc<RefCell<Config>>,
    on_change: Box<dyn Fn()>,
    on_open_path: Box<dyn Fn(std::path::PathBuf)>,
    on_check_updates: Box<dyn Fn()>,
    on_download: Box<dyn Fn()>,
    update_info: Box<dyn Fn() -> UpdateInfo>,
    on_repeat_tutorial: Box<dyn Fn()>,
    renderer: Renderer,
    page: Page,
    page_enter: Instant,
    hover: Hit,
    pressed: Hit,
    hits: Vec<(Rect, Hit)>,
    animations_enabled: bool,
    open_anim: Option<crate::Anim>,
    theme_from: Option<(bool, Instant)>,
    anim_timer_running: bool,
    /// El último `paint` dijo que hay algo moviéndose (hay que seguir repintando).
    needs_frames: bool,
    tw: Tweens<TK>,
    fmts: FormatCache,
    scroll: f32,
    content_h: f32,
    panel_h: f32,
    scroll_drag: Option<(f32, f32)>,
    slider_drag: bool,
    capture: Option<Capture>,
    tracking_mouse: bool,
    inputs: Inputs,
    focus: Option<InputId>,
    /// Desde cuándo parpadea el cursor (se reinicia al escribir).
    caret_since: Instant,
    /// Mitad alta de un par sustituto pendiente de `WM_CHAR`.
    pending_surrogate: Option<u16>,
    fonts: Vec<String>,
    preview: Preview,
    syn_pick: usize,
    syn_pick_at: Instant,
    logo: Option<ID2D1Bitmap>,
    logo_px: i32,
}

fn ensure_anim_timer(st: &mut State, hwnd: HWND) {
    if !st.anim_timer_running {
        unsafe {
            let _ = SetTimer(Some(hwnd), ID_ANIM_TIMER, 16, None);
        }
        st.anim_timer_running = true;
    }
}

fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn post_close(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn shell_open(hwnd: HWND, file: &str, params: Option<&str>) {
    let file_w = to_wide(file);
    let params_w = params.map(to_wide);
    unsafe {
        let _ = windows::Win32::UI::Shell::ShellExecuteW(
            Some(hwnd),
            w!("open"),
            PCWSTR(file_w.as_ptr()),
            params_w.as_ref().map(|p| PCWSTR(p.as_ptr())).unwrap_or(PCWSTR::null()),
            PCWSTR::null(),
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
    }
}

/// Abre la ventana de Ajustes, centrada sobre `parent`. `on_change` se llama cada vez
/// que el usuario cambia algo (ya guardado en disco); `on_open_path` cuando pide abrir
/// `config.toml` como documento en la ventana principal; `update_info`/`on_download`
/// conectan la página Actualizaciones con el actualizador de la ventana principal.
/// `start_section` es el `id` de la página con la que se abre (`""` = la primera).
#[allow(clippy::too_many_arguments)]
pub fn open(
    parent: HWND,
    cfg: Rc<RefCell<Config>>,
    on_change: Box<dyn Fn()>,
    on_open_path: Box<dyn Fn(std::path::PathBuf)>,
    on_check_updates: Box<dyn Fn()>,
    on_download: Box<dyn Fn()>,
    update_info: Box<dyn Fn() -> UpdateInfo>,
    on_repeat_tutorial: Box<dyn Fn()>,
    start_section: &str,
    on_created: impl Fn(HWND),
) -> Result<()> {
    let start_page = Page::from_id(start_section).unwrap_or(Page::Apariencia);
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
        let (x, y, w_px, h_px) = initial_rect(parent, scale);

        let title = to_wide("Ajustes · notty");
        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_THICKFRAME | WS_CLIPSIBLINGS,
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
        let now = Instant::now();
        let mut state = Box::new(State {
            cfg,
            on_change,
            on_open_path,
            on_check_updates,
            on_download,
            update_info,
            on_repeat_tutorial,
            renderer,
            page: start_page,
            page_enter: now,
            hover: Hit::None,
            pressed: Hit::None,
            hits: Vec::new(),
            animations_enabled,
            open_anim: Some(crate::Anim::new_maybe(now, Duration::from_millis(150), animations_enabled)),
            theme_from: None,
            anim_timer_running: false,
            needs_frames: true,
            tw: Tweens::new(!animations_enabled),
            fmts: FormatCache::default(),
            scroll: 0.0,
            content_h: 0.0,
            panel_h: 0.0,
            scroll_drag: None,
            slider_drag: false,
            capture: None,
            tracking_mouse: false,
            inputs: Inputs::default(),
            focus: None,
            caret_since: now,
            pending_surrogate: None,
            fonts: Vec::new(),
            preview: Preview::default(),
            syn_pick: 0,
            syn_pick_at: Instant::now(),
            logo: None,
            logo_px: 0,
        });
        if start_page == Page::Fuentes {
            state.fonts = state.renderer.monospace_families();
        }
        let ptr = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
        if let Some(st) = ptr.as_mut() {
            ensure_anim_timer(st, hwnd);
        }

        let _ = SetWindowPos(hwnd, None, x, y, w_px, h_px, SWP_NOZORDER | SWP_FRAMECHANGED);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetWindowPos(hwnd, None, x, y, w_px, h_px, SWP_NOZORDER);
        let mut rc = RECT::default();
        if GetClientRect(hwnd, &mut rc).is_ok() {
            if let Some(st) = ptr.as_mut() {
                st.renderer.resize((rc.right - rc.left).max(1) as u32, (rc.bottom - rc.top).max(1) as u32);
            }
        }

        on_created(hwnd);

        // Bucle propio hasta que se cierra Ajustes. Sin filtro de `hWnd` a propósito: la
        // ventana principal tiene que seguir repintándose para enseñar al momento cada
        // cambio (y para que Windows no la marque como "No responde").
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
            if ptr.is_null() {
                break;
            }
        }
        if msg.message == WM_QUIT {
            // Este bucle se ha comido el WM_QUIT (p. ej. relanzar tras actualizar):
            // se cierra Ajustes y se reenvía para que el bucle principal también salga.
            if !(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State).is_null() {
                let _ = DestroyWindow(hwnd);
            }
            PostQuitMessage(msg.wParam.0 as i32);
        }
    }
    Ok(())
}

fn initial_rect(parent: HWND, scale: f32) -> (i32, i32, i32, i32) {
    unsafe {
        let mut w_px = (WIN_W * scale) as i32;
        let mut h_px = (WIN_H * scale) as i32;
        let mut parent_rect = RECT::default();
        let _ = GetWindowRect(parent, &mut parent_rect);
        let monitor = MonitorFromWindow(parent, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let work = if GetMonitorInfoW(monitor, &mut mi).as_bool() { Some(mi.rcWork) } else { None };
        if let Some(wk) = work {
            let margin = (16.0 * scale) as i32;
            w_px = w_px.min((wk.right - wk.left - margin * 2).max((MIN_W * scale) as i32));
            h_px = h_px.min((wk.bottom - wk.top - margin * 2).max((MIN_H * scale) as i32));
        }
        let mut x = parent_rect.left + ((parent_rect.right - parent_rect.left) - w_px) / 2;
        let mut y = parent_rect.top + ((parent_rect.bottom - parent_rect.top) - h_px) / 2;
        if let Some(wk) = work {
            x = x.min(wk.right - w_px).max(wk.left);
            y = y.min(wk.bottom - h_px).max(wk.top);
        }
        (x, y, w_px, h_px)
    }
}

fn point_from_lparam(lparam: LPARAM) -> (f32, f32) {
    ((lparam.0 as i16) as f32, ((lparam.0 >> 16) as i16) as f32)
}

fn key_is_down(vk: u16) -> bool {
    unsafe { GetKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

fn current_mods() -> notty_input::Modifiers {
    notty_input::Modifiers { ctrl: key_is_down(0x11), shift: key_is_down(0x10), alt: key_is_down(0x12) }
}

fn capture_active(st: &State) -> bool {
    st.capture.is_some_and(|c| c.closing.is_none())
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        match msg {
            WM_PAINT => {
                if let Some(st) = ptr.as_mut() {
                    paint(st);
                    if st.needs_frames {
                        ensure_anim_timer(st, hwnd);
                    }
                }
                let _ = windows::Win32::Graphics::Gdi::ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }
            WM_SETTINGCHANGE => {
                // Cambio de tema de Windows con Ajustes abierto: sin esto el marco de DWM
                // (y su borde) se quedaba con el tema anterior.
                if let Some(st) = ptr.as_ref() {
                    let dark = is_dark(st.cfg.borrow().ui.theme, crate::window::system_uses_dark_mode());
                    crate::window::apply_dark_mode(hwnd, dark);
                    invalidate(hwnd);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_NCCALCSIZE if wparam.0 != 0 => {
                if IsZoomed(hwnd).as_bool() {
                    let params = &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS);
                    let dpi = GetDpiForWindow(hwnd);
                    let f = GetSystemMetricsForDpi(SM_CXFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
                    params.rgrc[0].left += f;
                    params.rgrc[0].top += f;
                    params.rgrc[0].right -= f;
                    params.rgrc[0].bottom -= f;
                }
                LRESULT(0)
            }
            WM_SIZE => {
                if let Some(st) = ptr.as_mut() {
                    let width = (lparam.0 as u32) & 0xFFFF;
                    let height = ((lparam.0 as u32) >> 16) & 0xFFFF;
                    st.renderer.resize(width.max(1), height.max(1));
                    invalidate(hwnd);
                }
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                let scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
                let info = &mut *(lparam.0 as *mut MINMAXINFO);
                info.ptMinTrackSize.x = (MIN_W * scale) as i32;
                info.ptMinTrackSize.y = (MIN_H * scale) as i32;
                LRESULT(0)
            }
            WM_DPICHANGED => {
                if let Some(st) = ptr.as_mut() {
                    st.renderer.set_dpi((wparam.0 & 0xFFFF) as u32);
                    st.logo = None;
                    let suggested = &*(lparam.0 as *const RECT);
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        suggested.left,
                        suggested.top,
                        suggested.right - suggested.left,
                        suggested.bottom - suggested.top,
                        SWP_NOZORDER,
                    );
                    invalidate(hwnd);
                }
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let Some(st) = ptr.as_mut() else { return DefWindowProcW(hwnd, msg, wparam, lparam) };
                let mut pt = POINT { x: (lparam.0 as i16) as i32, y: ((lparam.0 >> 16) as i16) as i32 };
                let _ = ScreenToClient(hwnd, &mut pt);
                let scale = st.renderer.scale();
                let (x, y) = (pt.x as f32 / scale, pt.y as f32 / scale);
                if !IsZoomed(hwnd).as_bool() {
                    let (w, h) = st.renderer.size_dips();
                    let b = RESIZE_BORDER;
                    let (l, r, t, bo) = (x < b, x >= w - b, y < b, y >= h - b);
                    let code = match (l, r, t, bo) {
                        (true, _, true, _) => Some(HTTOPLEFT),
                        (_, true, true, _) => Some(HTTOPRIGHT),
                        (true, _, _, true) => Some(HTBOTTOMLEFT),
                        (_, true, _, true) => Some(HTBOTTOMRIGHT),
                        (true, ..) => Some(HTLEFT),
                        (_, true, ..) => Some(HTRIGHT),
                        (_, _, true, _) => Some(HTTOP),
                        (_, _, _, true) => Some(HTBOTTOM),
                        _ => None,
                    };
                    if let Some(code) = code {
                        if !(code == HTTOPRIGHT && hit_test(st, x, y) == Hit::Close) || y < 2.0 {
                            return LRESULT(code as isize);
                        }
                    }
                }
                if hit_test(st, x, y) == Hit::Caption {
                    return LRESULT(HTCAPTION as isize);
                }
                LRESULT(HTCLIENT as isize)
            }
            WM_LBUTTONDOWN => {
                if let Some(st) = ptr.as_mut() {
                    let (x, y) = point_from_lparam(lparam);
                    let scale = st.renderer.scale();
                    handle_click(hwnd, st, x / scale, y / scale);
                    ensure_anim_timer(st, hwnd);
                }
                LRESULT(0)
            }
            WM_XBUTTONDOWN => {
                // Botón "atrás" del ratón (XBUTTON1).
                if let Some(st) = ptr.as_mut() {
                    if (wparam.0 >> 16) & 0xFFFF == 1 && st.capture.is_none() {
                        go_back(st, hwnd);
                    }
                }
                LRESULT(1)
            }
            WM_MOUSEMOVE => {
                if let Some(st) = ptr.as_mut() {
                    if !st.tracking_mouse {
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        let _ = TrackMouseEvent(&mut tme);
                        st.tracking_mouse = true;
                    }
                    let (x, y) = point_from_lparam(lparam);
                    let scale = st.renderer.scale();
                    let (x, y) = (x / scale, y / scale);
                    if let Some((y0, s0)) = st.scroll_drag {
                        let (_, h) = st.renderer.size_dips();
                        let (track, thumb) = scrollbar_geom(st, h);
                        let free = (track.height() - thumb.height()).max(1.0);
                        st.scroll = (s0 + (y - y0) * max_scroll(st) / free).clamp(0.0, max_scroll(st));
                        invalidate(hwnd);
                        return LRESULT(0);
                    }
                    if st.slider_drag {
                        slider_to(st, x);
                        invalidate(hwnd);
                        return LRESULT(0);
                    }
                    let hit = hit_test(st, x, y);
                    if hit != st.hover {
                        st.hover = hit;
                        ensure_anim_timer(st, hwnd);
                        invalidate(hwnd);
                    }
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                if let Some(st) = ptr.as_mut() {
                    st.tracking_mouse = false;
                    if st.scroll_drag.is_none() && !st.slider_drag {
                        st.hover = Hit::None;
                        ensure_anim_timer(st, hwnd);
                        invalidate(hwnd);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if let Some(st) = ptr.as_mut() {
                    st.scroll_drag = None;
                    if st.slider_drag {
                        st.slider_drag = false;
                        save_and_notify(st);
                    }
                    st.pressed = Hit::None;
                    ensure_anim_timer(st, hwnd);
                    invalidate(hwnd);
                }
                let _ = ReleaseCapture();
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                if let Some(st) = ptr.as_mut() {
                    if st.capture.is_none() {
                        let delta = ((wparam.0 >> 16) as i16) as f32 / 120.0;
                        scroll_by(st, -delta * WHEEL_STEP);
                        invalidate(hwnd);
                    }
                }
                LRESULT(0)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if let Some(st) = ptr.as_mut() {
                    let vk = wparam.0 as u32;
                    if capture_active(st) {
                        let repeat = (lparam.0 >> 30) & 1 == 1;
                        capture_key(st, hwnd, vk, repeat);
                        return LRESULT(0);
                    }
                    if msg == WM_SYSKEYDOWN {
                        // Alt+← vuelve atrás, como en el navegador.
                        if vk == 0x25 {
                            go_back(st, hwnd);
                            return LRESULT(0);
                        }
                    } else {
                        if st.focus.is_some() {
                            input_key(st, hwnd, vk);
                        } else {
                            handle_nav_key(st, hwnd, vk);
                        }
                        return LRESULT(0);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_CHAR => {
                if let Some(st) = ptr.as_mut() {
                    if let Some(id) = st.focus {
                        let unit = wparam.0 as u16;
                        let s = if (0xD800..0xDC00).contains(&unit) {
                            st.pending_surrogate = Some(unit);
                            None
                        } else if (0xDC00..0xE000).contains(&unit) {
                            st.pending_surrogate.take().map(|hi| String::from_utf16_lossy(&[hi, unit]))
                        } else {
                            char::from_u32(unit as u32).filter(|c| !c.is_control()).map(|c| c.to_string())
                        };
                        if let Some(s) = s {
                            st.inputs.get_mut(id).insert(&s);
                            input_changed(st, id);
                            invalidate(hwnd);
                        }
                    }
                }
                LRESULT(0)
            }
            WM_KEYUP | WM_SYSKEYUP | WM_SYSCHAR => {
                if let Some(st) = ptr.as_mut() {
                    if st.capture.is_some() {
                        invalidate(hwnd);
                        return LRESULT(0);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_SYSCOMMAND if (wparam.0 & 0xFFF0) as u32 == SC_KEYMENU => {
                if ptr.as_ref().is_some_and(|st| st.capture.is_some()) {
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_TIMER => {
                if wparam.0 == ID_ANIM_TIMER {
                    if let Some(st) = ptr.as_mut() {
                        let now = Instant::now();
                        if st.open_anim.is_some_and(|a| a.is_done(now)) {
                            st.open_anim = None;
                        }
                        if st.theme_from.is_some_and(|(_, t0)| now.saturating_duration_since(t0) >= Duration::from_millis(THEME_MS)) {
                            st.theme_from = None;
                        }
                        let out_d = Duration::from_millis(CAPTURE_OUT_MS);
                        if st.capture.is_some_and(|c| c.closing.is_some_and(|t0| !st.animations_enabled || now.saturating_duration_since(t0) >= out_d)) {
                            st.capture = None;
                        }
                        st.tw.prune(now);
                        if !st.needs_frames {
                            let _ = KillTimer(Some(hwnd), ID_ANIM_TIMER);
                            st.anim_timer_running = false;
                        }
                        invalidate(hwnd);
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
                // Sin `PostQuitMessage`: el bucle de `open` sale al ver el puntero a
                // null y dejaría el WM_QUIT en la cola para el bucle principal.
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// Teclas fuera de la captura y de los campos de texto.
fn handle_nav_key(st: &mut State, hwnd: HWND, vk: u32) {
    let page = (st.panel_h - 40.0).max(WHEEL_STEP);
    match vk {
        0x1B => {
            if st.page.parent().is_some() {
                go_back(st, hwnd);
            } else {
                post_close(hwnd);
            }
        }
        0x08 => go_back(st, hwnd),
        0x21 => scroll_by(st, -page),
        0x22 => scroll_by(st, page),
        0x24 => st.scroll = 0.0,
        0x23 => st.scroll = max_scroll(st),
        0x26 => scroll_by(st, -WHEEL_STEP),
        0x28 => scroll_by(st, WHEEL_STEP),
        _ => return,
    }
    invalidate(hwnd);
}

/// Teclas con un campo de texto enfocado.
fn input_key(st: &mut State, hwnd: HWND, vk: u32) {
    let Some(id) = st.focus else { return };
    let shift = key_is_down(0x10);
    let ctrl = key_is_down(0x11);
    let inp = st.inputs.get_mut(id);
    match vk {
        0x25 => inp.left(shift),
        0x27 => inp.right(shift),
        0x24 => inp.home(shift),
        0x23 => inp.end(shift),
        0x08 => inp.backspace(),
        0x2E => inp.delete(),
        0x41 if ctrl => inp.select_all(),
        0x56 if ctrl => {
            if let Ok(text) = crate::clipboard::get_clipboard_text(hwnd) {
                inp.insert(&text);
            }
        }
        0x43 if ctrl => {
            if let Some((a, b)) = inp.selection() {
                let s: String = inp.text.chars().skip(a).take(b - a).collect();
                let _ = crate::clipboard::set_clipboard_text(hwnd, &s);
            }
        }
        0x09 => {
            // Tab salta entre los dos campos del formulario de ligaduras.
            st.focus = match id {
                InputId::LigSeq => Some(InputId::LigGlyph),
                InputId::LigGlyph => Some(InputId::LigSeq),
                InputId::Sample => Some(InputId::Search),
                InputId::Search => Some(InputId::Sample),
            };
        }
        0x0D => {
            if matches!(id, InputId::LigSeq | InputId::LigGlyph) {
                add_ligature(st);
            } else {
                st.focus = None;
            }
        }
        0x1B => st.focus = None,
        _ => return,
    }
    st.caret_since = Instant::now();
    input_changed(st, id);
    invalidate(hwnd);
}

fn input_changed(st: &mut State, id: InputId) {
    st.caret_since = Instant::now();
    if id == InputId::Search {
        st.scroll = st.scroll.min(max_scroll(st));
    }
}

fn add_ligature(st: &mut State) {
    let (seq, glyph) = (st.inputs.seq.text.clone(), st.inputs.glyph.text.clone());
    if model::add_ligature(&mut st.cfg.borrow_mut(), &seq, &glyph).is_ok() {
        st.inputs.seq.clear();
        st.inputs.glyph.clear();
        st.focus = Some(InputId::LigSeq);
        save_and_notify(st);
    }
}

fn max_scroll(st: &State) -> f32 {
    (st.content_h - st.panel_h).max(0.0)
}

fn scroll_by(st: &mut State, dy: f32) {
    st.scroll = (st.scroll + dy).clamp(0.0, max_scroll(st));
}

fn scrollbar_geom(st: &State, h: f32) -> (Rect, Rect) {
    let (w, _) = st.renderer.size_dips();
    let track = Rect::new(w - 10.0, TITLEBAR_H + 4.0, w - 4.0, h - 4.0);
    let max = max_scroll(st);
    if max <= 0.0 || st.content_h <= 0.0 {
        return (track, Rect::new(track.left, track.top, track.right, track.top));
    }
    let thumb_h = (track.height() * st.panel_h / st.content_h).clamp(24.0, track.height());
    let top = track.top + (track.height() - thumb_h) * (st.scroll / max).clamp(0.0, 1.0);
    (track, Rect::new(track.left, top, track.right, top + thumb_h))
}

fn hit_test(st: &State, x: f32, y: f32) -> Hit {
    for &(r, h) in st.hits.iter().rev() {
        if r.contains(x, y) {
            return h;
        }
    }
    if y < TITLEBAR_H { Hit::Caption } else { Hit::None }
}

fn hit_rect(st: &State, hit: Hit) -> Option<Rect> {
    st.hits.iter().rev().find(|(_, h)| *h == hit).map(|(r, _)| *r)
}

fn goto(st: &mut State, hwnd: HWND, page: Page) {
    if page != st.page {
        st.page = page;
        st.page_enter = Instant::now();
        st.scroll = 0.0;
        st.focus = None;
        st.tw.remove(TK::Preview(1));
        if page == Page::Fuentes && st.fonts.is_empty() {
            st.fonts = st.renderer.monospace_families();
        }
        ensure_anim_timer(st, hwnd);
    }
    invalidate(hwnd);
}

fn go_back(st: &mut State, hwnd: HWND) {
    if let Some(parent) = st.page.parent() {
        goto(st, hwnd, parent);
    }
}

/// Ventana de Ajustes ya abierta: se lleva el foco a ella y, si se pidió una página
/// concreta, se cambia a ella.
pub fn focus_existing(hwnd: HWND, section: &str) {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if let (Some(st), Some(page)) = (ptr.as_mut(), Page::from_id(section)) {
            goto(st, hwnd, page);
        }
    }
}

fn slider_to(st: &mut State, x: f32) {
    let Some(r) = hit_rect(st, Hit::Slider) else { return };
    let px = pages::slider_px(r.left + 8.0, r.width() - 16.0, x);
    if px != model::font_px(&st.cfg.borrow()) {
        model::set_font_px(&mut st.cfg.borrow_mut(), px);
        (st.on_change)();
    }
}

fn handle_click(hwnd: HWND, st: &mut State, x: f32, y: f32) {
    let hit = hit_test(st, x, y);
    st.pressed = hit;
    if !matches!(hit, Hit::Input(_)) && st.focus.is_some() {
        st.focus = None;
    }
    match hit {
        Hit::Close => post_close(hwnd),
        Hit::Nav(p) => goto(st, hwnd, p),
        Hit::Go(p) => goto(st, hwnd, p),
        Hit::Static(1) => goto(st, hwnd, Page::Fuentes),
        Hit::Back | Hit::Crumb => go_back(st, hwnd),
        Hit::EditConfig => (st.on_open_path)(notty_config::default_path()),
        Hit::Link(action) => run_link(st, hwnd, action),
        Hit::Toggle(key) => {
            let v = !model::current_bool(&st.cfg.borrow(), key);
            model::apply(&mut st.cfg.borrow_mut(), key, SettingValue::Bool(v));
            save_and_notify(st);
        }
        Hit::Choice(key, j) => {
            if let Some((_, v)) = model::options_for(key).get(j as usize) {
                set_value(st, hwnd, key, *v);
            }
        }
        Hit::Font(i) => {
            if let Some(name) = st.fonts.get(i as usize).cloned() {
                let f = if name == "Cascadia Mono" { FontFamily::AUTO } else { FontFamily::named(&name) };
                set_value(st, hwnd, SettingKey::FontFamily, SettingValue::FontFamily(f));
            }
        }
        Hit::Slider => {
            st.slider_drag = true;
            slider_to(st, x);
            unsafe {
                SetCapture(hwnd);
            }
        }
        Hit::Input(id) => {
            st.focus = Some(id);
            st.caret_since = Instant::now();
            if let Some(r) = hit_rect(st, hit) {
                let inp = st.inputs.get_mut(id);
                if matches!(id, InputId::Sample | InputId::Search) {
                    let fmt = st.fmts.get(&st.renderer, None, 12.0, false);
                    let at = Ui::char_at(&st.renderer, inp, &fmt, x - (r.left + 11.0));
                    inp.move_to(at, key_is_down(0x10));
                } else {
                    inp.end(false);
                }
            }
        }
        Hit::LigToggle(j) | Hit::LigDelete(j) => {
            let seq = {
                let cfg = st.cfg.borrow();
                crate::ligature::entries(&cfg.ligature_overrides, &cfg.ligature_disabled).get(j as usize).map(|e| e.seq.clone())
            };
            if let Some(seq) = seq {
                if matches!(hit, Hit::LigToggle(_)) {
                    model::toggle_ligature(&mut st.cfg.borrow_mut(), &seq);
                } else {
                    model::remove_ligature(&mut st.cfg.borrow_mut(), &seq);
                }
                save_and_notify(st);
            }
        }
        Hit::LigAdd => add_ligature(st),
        Hit::LangToggle(j) => {
            if let Some(l) = crate::syntax::LANGS.get(j as usize) {
                model::toggle_syntax_lang(&mut st.cfg.borrow_mut(), l.id);
                save_and_notify(st);
            }
        }
        Hit::LangPick(j) => {
            if (j as usize) != st.syn_pick {
                st.syn_pick = j as usize;
                st.syn_pick_at = Instant::now();
                ensure_anim_timer(st, hwnd);
            }
        }
        Hit::LangAll(on) => {
            model::set_all_syntax_langs(&mut st.cfg.borrow_mut(), on);
            save_and_notify(st);
        }
        Hit::Binding(cmd) => {
            let now = Instant::now();
            st.capture = Some(Capture { cmd, opened: now, closing: None, captured: None, captured_at: now, error: None });
            unsafe {
                let _ = SetFocus(Some(hwnd));
            }
        }
        Hit::ScrollThumb => {
            st.scroll_drag = Some((y, st.scroll));
            unsafe {
                SetCapture(hwnd);
            }
        }
        Hit::ScrollTrack => {
            let (_, h) = st.renderer.size_dips();
            let (_, thumb) = scrollbar_geom(st, h);
            let page = (st.panel_h - 40.0).max(WHEEL_STEP);
            scroll_by(st, if y < thumb.top { -page } else { page });
        }
        Hit::CapSave => capture_save(st, hwnd),
        Hit::CapCancel | Hit::CapBackdrop => close_capture(st, hwnd),
        Hit::CapReset => {
            if let Some(cap) = st.capture.as_mut() {
                cap.captured = notty_input::parse_key_spec(cap.cmd.default_spec());
                cap.captured_at = Instant::now();
                cap.error = None;
            }
        }
        Hit::CapCard | Hit::Static(_) | Hit::Rail | Hit::None | Hit::Caption => {}
    }
    invalidate(hwnd);
}

fn run_link(st: &mut State, hwnd: HWND, action: LinkAction) {
    let repo = notty_update::REPO;
    match action {
        LinkAction::OpenConfig => (st.on_open_path)(notty_config::default_path()),
        LinkAction::CheckUpdatesNow => (st.on_check_updates)(),
        LinkAction::DownloadUpdate => (st.on_download)(),
        LinkAction::ReleaseNotes => {
            if let Some(url) = (st.update_info)().release_url {
                shell_open(hwnd, &url, None);
            }
        }
        LinkAction::OpenRepo => shell_open(hwnd, &format!("https://github.com/{repo}"), None),
        LinkAction::OpenChangelog => shell_open(hwnd, &format!("https://github.com/{repo}/releases"), None),
        LinkAction::OpenConfigFolder => {
            let path = notty_config::default_path();
            if path.exists() {
                shell_open(hwnd, "explorer.exe", Some(&format!("/select,\"{}\"", path.display())));
            } else if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
                shell_open(hwnd, &dir.display().to_string(), None);
            }
        }
        LinkAction::RepeatTutorial => {
            (st.on_repeat_tutorial)();
            post_close(hwnd);
        }
    }
}

/// Aplica un valor elegido (selector, tarjeta, fuente) y, si cambió el tema, arranca
/// el fundido y avisa a DWM para que el marco nativo lo siga.
fn set_value(st: &mut State, hwnd: HWND, key: SettingKey, value: SettingValue) {
    let system_dark = crate::window::system_uses_dark_mode();
    let was_dark = is_dark(st.cfg.borrow().ui.theme, system_dark);
    model::apply(&mut st.cfg.borrow_mut(), key, value);
    save_and_notify(st);
    let now_dark = is_dark(st.cfg.borrow().ui.theme, system_dark);
    if now_dark != was_dark {
        st.theme_from = Some((was_dark, Instant::now()));
        st.logo = None;
        unsafe { crate::window::apply_dark_mode(hwnd, now_dark) };
    }
}

fn save_and_notify(st: &State) {
    let _ = notty_config::save(&st.cfg.borrow(), &notty_config::default_path());
    (st.on_change)();
}

// --- Captura de atajos ----------------------------------------------------------------

fn is_modifier_vk(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0xA0..=0xA5 | 0x5B | 0x5C | 0x14)
}

/// Una pulsación con la captura abierta: Esc cancela, Enter guarda (si ya hay una
/// combinación), los modificadores solo se ven en vivo, y cualquier otra tecla con
/// sus modificadores es la combinación nueva.
fn capture_key(st: &mut State, hwnd: HWND, vk: u32, repeat: bool) {
    invalidate(hwnd);
    if repeat || is_modifier_vk(vk) {
        return;
    }
    let mods = current_mods();
    let bare = !mods.ctrl && !mods.alt && !mods.shift;
    if vk == 0x1B && bare {
        close_capture(st, hwnd);
        return;
    }
    if vk == 0x0D && bare {
        capture_save(st, hwnd);
        return;
    }
    let Some(cap) = st.capture.as_mut() else { return };
    // Sin Ctrl, un Alt+tecla llega a la ventana principal como `WM_SYSKEYDOWN`, que
    // ahí no se enruta a los atajos: se exige Ctrl (o una F sin Alt).
    let fkey = (0x70..=0x7B).contains(&vk);
    if !(mods.ctrl || (fkey && !mods.alt)) {
        cap.error = Some("Usa Ctrl + tecla (Alt y Shift pueden acompañar), o una de F1–F12.");
        return;
    }
    if notty_input::format_key_spec(vk, mods).is_none() {
        cap.error = Some("Esa tecla no se puede usar en un atajo.");
        return;
    }
    cap.captured = Some((vk, mods));
    cap.captured_at = Instant::now();
    cap.error = None;
}

fn close_capture(st: &mut State, hwnd: HWND) {
    if let Some(cap) = st.capture.as_mut() {
        if cap.closing.is_none() {
            cap.closing = Some(Instant::now());
        }
    }
    ensure_anim_timer(st, hwnd);
    invalidate(hwnd);
}

/// Guarda la combinación capturada en `[keys]`. Si otro comando la tenía, ese se
/// queda sin atajo (el aviso de la captura ya lo decía y el botón pone "Reemplazar").
fn capture_save(st: &mut State, hwnd: HWND) {
    let Some(cap) = st.capture else { return };
    if cap.closing.is_some() {
        return;
    }
    let Some(key) = cap.captured else { return };
    {
        let mut cfg = st.cfg.borrow_mut();
        if let Some(other) = notty_input::conflict(&cfg, cap.cmd, key) {
            notty_input::set_binding(&mut cfg, other, None);
        }
        notty_input::set_binding(&mut cfg, cap.cmd, Some(key));
    }
    save_and_notify(st);
    // Destello verde en la fila al volver (`.flash` de la maqueta).
    st.tw.kick(TK::Flash(cap.cmd), 1.0, 0.0, 1200, Curve::Out, Instant::now());
    close_capture(st, hwnd);
}

/// Nombre en castellano de una acción del editor que ya usa esa combinación (el
/// atajo nuevo tiene prioridad sobre ella en `window.rs`).
fn editor_action_name(vk: u32, m: notty_input::Modifiers) -> Option<&'static str> {
    use crate::EditorAction::*;
    let name = match crate::action_for_vk(vk, crate::Modifiers { ctrl: m.ctrl, shift: m.shift, alt: m.alt }) {
        Undo => "Deshacer",
        Redo => "Rehacer",
        SelectAll => "Seleccionar todo",
        Copy => "Copiar",
        Cut => "Cortar",
        Paste => "Pegar",
        Save => "Guardar",
        Find => "Buscar",
        Replace => "Reemplazar",
        FindNext => "Siguiente coincidencia",
        FindPrev => "Coincidencia anterior",
        OpenPathPrompt => "Abrir ruta",
        MoveDocStart | MoveDocEnd => "ir al principio/final",
        Backspace | DeleteForward => "borrar",
        InsertNewline => "nueva línea",
        MoveLeft | MoveRight | MoveUp | MoveDown | MoveHome | MoveEnd | ExtendLeft | ExtendRight | ExtendUp
        | ExtendDown | ExtendHome | ExtendEnd => "mover el cursor",
        ToggleVim | ToggleRaw | None => return Option::None,
    };
    Some(name)
}

// --- Dibujo ---------------------------------------------------------------------------

/// Recorta `s` con «…» para que quepa en `max_w` con `fmt`.
fn ellipsize(r: &Renderer, s: &str, fmt: &windows::Win32::Graphics::DirectWrite::IDWriteTextFormat, max_w: f32) -> String {
    if r.measure(s, fmt) <= max_w {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let cand: String = chars[..mid].iter().collect::<String>().trim_end().to_string() + "…";
        if r.measure(&cand, fmt) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    if lo == 0 { "…".to_string() } else { chars[..lo].iter().collect::<String>().trim_end().to_string() + "…" }
}

fn paint(st: &mut State) {
    let now = Instant::now();
    st.hits.clear();
    let system_dark = crate::window::system_uses_dark_mode();
    let dark = is_dark(st.cfg.borrow().ui.theme, system_dark);
    let pal_mixed;
    let pal = match st.theme_from {
        Some((from_dark, t0)) if st.animations_enabled => {
            let p = (now.saturating_duration_since(t0).as_secs_f32() / (THEME_MS as f32 / 1000.0)).clamp(0.0, 1.0);
            pal_mixed = theme::palette(from_dark).mix(theme::palette(dark), crate::ease_out_cubic(p));
            &pal_mixed
        }
        _ => theme::palette(dark),
    };
    let (w, h) = st.renderer.size_dips();
    let logo_px = (64.0 * st.renderer.scale()).round() as i32;
    if st.page == Page::AcercaDe && (st.logo.is_none() || st.logo_px != logo_px) {
        st.logo = st.renderer.app_icon_bitmap(logo_px);
        st.logo_px = logo_px;
    }
    let update = (st.update_info)();
    let caret_on = st.focus.is_some() && (now.saturating_duration_since(st.caret_since).as_millis() % 1060) < 530;

    let r = &st.renderer;
    r.begin_paint(pal.surface);
    let open_t = st.open_anim.map(|a| a.value(now, 0.0, 1.0));
    let base = match open_t {
        Some(t) => {
            let s = 0.97 + 0.03 * t;
            r.set_fade(t);
            Matrix3x2::scale_around(s, s, Vector2 { X: w / 2.0, Y: h / 2.0 })
        }
        None => Matrix3x2::identity(),
    };
    r.set_transform(base);

    let cfg_ref = st.cfg.borrow();
    let mut ui = Ui::new(r, pal, &mut st.tw, &mut st.hits, &st.fmts, st.hover, st.pressed, now, base);

    // Barra de título.
    let titlebar = Rect::new(0.0, 0.0, w, TITLEBAR_H);
    r.fill(titlebar, pal.chrome);
    r.fill_round(Rect::new(12.0, 10.0, 24.0, 22.0), 3.0, pal.accent);
    ui.text("Ajustes · notty", 12.0, false, Rect::new(32.0, 0.0, w - CLOSE_W, TITLEBAR_H), pal.text_2);
    let close_r = Rect::new(w - CLOSE_W, 0.0, w, TITLEBAR_H);
    let ch = ui.hover_t(Hit::Close, 120);
    r.fill(close_r, pal.close_hover.faded(ch));
    let cc = pal.text_2.mix(pal.close_hover_fg, ch);
    ui.icon(icon::CRUZ, close_r.left + CLOSE_W / 2.0, TITLEBAR_H / 2.0, 20.0, 1.56, cc);
    ui.hits.push((titlebar, Hit::Caption));
    ui.hits.push((close_r, Hit::Close));

    // Página.
    let panel = Rect::new(RAIL_W, TITLEBAR_H, w, h);
    st.panel_h = panel.height();
    let scroll = st.scroll.clamp(0.0, (st.content_h - st.panel_h).max(0.0));
    ui.clip = panel;
    r.push_clip(panel);
    let data = PageData {
        cfg: &cfg_ref,
        page: st.page,
        enter: st.page_enter,
        fonts: &st.fonts,
        inputs: &st.inputs,
        focus: st.focus,
        caret_on,
        update: &update,
        preview: &st.preview,
        syn_pick: st.syn_pick,
        syn_pick_at: st.syn_pick_at,
        logo: st.logo.as_ref(),
        system_dark,
    };
    let area = Rect::new(panel.left, panel.top - scroll, panel.right, panel.bottom - scroll);
    let content_h = pages::draw_page(&mut ui, &data, area);
    let continuous = pages::has_continuous_anim(&data);
    r.pop_clip();
    ui.clip = Rect::new(-1e6, -1e6, 1e6, 1e6);

    // Barra de desplazamiento fina, solo si el contenido no cabe.
    let max = (content_h - panel.height()).max(0.0);
    if max > 0.0 {
        let track = Rect::new(w - 10.0, TITLEBAR_H + 4.0, w - 4.0, h - 4.0);
        let thumb_h = (track.height() * panel.height() / content_h).clamp(24.0, track.height());
        let top = track.top + (track.height() - thumb_h) * (scroll / max).clamp(0.0, 1.0);
        let thumb = Rect::new(track.left, top, track.right, top + thumb_h);
        let hv = ui.hover_t(Hit::ScrollThumb, 150).max(if st.scroll_drag.is_some() { 1.0 } else { 0.0 });
        r.fill_round(thumb, thumb.width() / 2.0, pal.text_3.mix(pal.text_2, hv));
        ui.hits.push((track, Hit::ScrollTrack));
        ui.hits.push((Rect::new(thumb.left - 2.0, thumb.top, thumb.right + 2.0, thumb.bottom), Hit::ScrollThumb));
    }

    draw_rail(&mut ui, st.page, &update, w, h, st.hover);

    let tw_animating = ui.tw.animating(now);
    drop(ui);

    if let Some(cap) = st.capture {
        r.set_transform(Matrix3x2::identity());
        r.set_fade(1.0);
        draw_capture(r, pal, &st.fmts, &cap, &cfg_ref, w, h, st.hover, &mut st.hits, st.animations_enabled);
    }
    r.reset_transform();
    r.set_fade(1.0);
    r.end_paint();
    drop(cfg_ref);

    st.content_h = content_h;
    st.scroll = st.scroll.min(max_scroll(st));
    let entering = st.animations_enabled && now < st.page_enter + Duration::from_millis(ENTER_MS);
    st.needs_frames = tw_animating
        || entering
        || (continuous && st.animations_enabled)
        || st.focus.is_some()
        || st.open_anim.is_some()
        || st.theme_from.is_some()
        || st.capture.is_some()
        || matches!(update.phase, crate::UpdatePhase::Checking | crate::UpdatePhase::Downloading(..))
        || (st.animations_enabled && matches!(update.phase, crate::UpdatePhase::Found));
}

/// Barra lateral: 52 px de iconos que se despliega a 208 por encima del contenido al
/// pasar el ratón, con las etiquetas entrando escalonadas y un indicador que salta
/// con rebote a la página activa.
fn draw_rail(ui: &mut Ui, page: Page, update: &UpdateInfo, _w: f32, h: f32, hover: Hit) {
    let pal = ui.pal;
    let r = ui.r;
    let open = matches!(hover, Hit::Nav(_) | Hit::EditConfig | Hit::Rail);
    let t = ui.tween(TK::RailOpen, open as u8 as f32, 260, Curve::Out);
    let rw = RAIL_W + (RAIL_OPEN_W - RAIL_W) * t;
    let rail = Rect::new(0.0, TITLEBAR_H, rw, h);
    if t > 0.01 {
        // Sombra hacia la derecha (`box-shadow: 14px 0 34px`).
        for k in 0..10 {
            let a = 0.45 * t * (1.0 - k as f32 / 10.0).powi(2) * 0.22;
            r.fill(Rect::new(rw + 3.0 * k as f32, TITLEBAR_H, rw + 3.0 * (k + 1) as f32, h), pal.shadow.faded(a / pal.shadow.3.max(0.01)));
        }
    }
    r.fill(rail, pal.chrome);
    r.fill(Rect::new(rw - 1.0, TITLEBAR_H, rw, h), pal.line);
    ui.hits.push((rail, Hit::Rail));
    r.push_clip(Rect::new(0.0, TITLEBAR_H, rw - 1.0, h));

    let active = page.rail_index();
    let icons = [icon::APARIENCIA, icon::VENTANA, icon::TECLADO, icon::CARPETA, icon::RAYO, icon::ACTUALIZAR, icon::INFO, icon::AYUDA];
    let top0 = TITLEBAR_H + 10.0;
    let ind = ui.tween(TK::RailInd, active as f32, 380, Curve::Spring);
    for (i, p) in Page::RAIL.iter().enumerate() {
        let hit = Hit::Nav(*p);
        let by = top0 + 38.0 * i as f32;
        let br = Rect::new(6.0, by, rw - 7.0, by + 36.0);
        nav_item(ui, br, hit, icons[i], p.name(), i == active, i, open);
        if *p == Page::Actualizaciones && matches!(update.phase, crate::UpdatePhase::Found | crate::UpdatePhase::Downloading(..)) {
            let (dx, dy) = (br.left + 25.0 + 3.5, br.top + 8.0 + 3.5);
            let p = if ui.anim() { (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0) % 1500) as f32 / 1500.0 } else { 0.0 };
            r.fill_circle(dx, dy, 3.5 + 2.0 + 7.0 * p, pal.accent.faded(0.6 * (1.0 - p)));
            r.fill_circle(dx, dy, 5.5, pal.chrome);
            r.fill_circle(dx, dy, 3.5, pal.accent);
        }
    }
    // Indicador activo.
    let iy = top0 + 38.0 * ind + 10.0;
    r.fill_round(Rect::new(0.0, iy, 3.0, iy + 16.0), 1.5, pal.accent);
    // "Editar config.toml" abajo del todo.
    let by = h - 10.0 - 36.0;
    if by > top0 + 38.0 * Page::RAIL.len() as f32 {
        nav_item(ui, Rect::new(6.0, by, rw - 7.0, by + 36.0), Hit::EditConfig, icon::ARCHIVO, "Editar config.toml", false, 8, open);
    }
    r.pop_clip();
}

#[allow(clippy::too_many_arguments)]
fn nav_item(ui: &mut Ui, br: Rect, hit: Hit, d: &str, label: &str, active: bool, i: usize, open: bool) {
    let pal = ui.pal;
    let hv = ui.hover_t(hit, 150);
    let bg = if active { pal.accent.faded(0.14) } else { pal.hover.faded(hv) };
    ui.r.fill_round(br, 6.0, bg);
    let fg = if active { pal.accent } else { pal.text_2.mix(pal.text, hv) };
    let wig = ui.hover_spring(hit, 300);
    let press = ui.press_t(hit, 100);
    let c = Vector2 { X: br.left + 11.0 + 9.0, Y: br.top + 18.0 };
    let s = (1.0 + 0.14 * wig) * (1.0 - 0.08 * press);
    ui.push_xf(Matrix3x2::rotation_around(-4.0 * wig, c) * Matrix3x2::scale_around(s, s, c));
    ui.icon(d, c.X, c.Y, 18.0, 1.6, fg);
    ui.pop_xf();
    let delay = if open { 60 + 20 * i.min(7) as u64 } else { 0 };
    let lt = ui.tw.to_delayed(TK::RailLabel(i as u8), open as u8 as f32, if open { 200 } else { 140 }, delay, Curve::Out, ui.now);
    if lt > 0.01 {
        ui.push_fade(lt);
        let lx = br.left + 11.0 + 18.0 + 14.0 - 6.0 * (1.0 - lt);
        ui.text(label, 13.0, false, Rect::new(lx, br.top, lx + 160.0, br.bottom), fg);
        ui.pop_fade();
    }
    ui.hits.push((br, hit));
}

/// Capa de captura de atajo: velo sobre la ventana y tarjeta con el nombre de la
/// acción, el teclado (combinación actual en azul, teclas pulsadas en verde) y
/// Guardar/Cancelar/Restablecer. Entra con rebote (`cardIn`) y sale en 140 ms.
#[allow(clippy::too_many_arguments)]
fn draw_capture(
    r: &Renderer,
    pal: &theme::Palette,
    fmts: &FormatCache,
    cap: &Capture,
    cfg: &Config,
    w: f32,
    h: f32,
    hover: Hit,
    hits: &mut Vec<(Rect, Hit)>,
    animations_enabled: bool,
) {
    let now = Instant::now();
    let (t, pos) = match cap.closing {
        Some(t0) => {
            let k = 1.0 - crate::Anim::new_maybe(t0, Duration::from_millis(CAPTURE_OUT_MS), animations_enabled).value(now, 0.0, 1.0);
            (k, k)
        }
        None => {
            let p = crate::Anim::new_maybe(cap.opened, Duration::from_millis(CAPTURE_IN_MS), animations_enabled).progress(now);
            (Curve::Linear.apply(p / 0.75), Curve::Spring.apply(p))
        }
    };
    if t <= 0.0 {
        return;
    }

    let area = Rect::new(0.0, TITLEBAR_H, w, h);
    r.fill(area, Rgba(0.0, 0.0, 0.0, 0.45 * t.min(1.0)));
    if cap.closing.is_none() {
        hits.push((area, Hit::CapBackdrop));
    }

    const PAD: f32 = 20.0;
    const TITLE_H: f32 = 26.0;
    const LINE_H: f32 = 18.0;
    const MSG_H: f32 = 36.0;
    const BTN_H: f32 = 32.0;
    let fixed_h = PAD + TITLE_H + 4.0 + LINE_H + 14.0 + 14.0 + MSG_H + 14.0 + BTN_H + PAD;
    let avail_w = (w - 32.0).min(600.0);
    let avail_h = area.height() - 24.0;
    let u = ((avail_w - PAD * 2.0) / keyboard_widget::UNITS_W).min((avail_h - fixed_h) / keyboard_widget::UNITS_H).clamp(10.0, 30.0);
    let card_w = (u * keyboard_widget::UNITS_W + PAD * 2.0).max(avail_w.min(380.0));
    let card_h = fixed_h + u * keyboard_widget::UNITS_H;
    let cx = area.left + (area.width() - card_w) / 2.0;
    let cy = area.top + ((area.height() - card_h) / 2.0).max(8.0);
    let card = Rect::new(cx, cy, cx + card_w, cy + card_h);

    let (scale, dy) = if cap.closing.is_some() { (0.96 + 0.04 * pos, 0.0) } else { (0.94 + 0.06 * pos, 8.0 * (1.0 - pos)) };
    r.set_transform(
        Matrix3x2::scale_around(scale, scale, Vector2 { X: card.left + card.width() / 2.0, Y: card.top + card.height() / 2.0 })
            * Matrix3x2::translation(0.0, dy),
    );
    r.set_fade(t.min(1.0));

    r.draw_popup_shadow(card, 10.0, 1.0, pal.shadow);
    r.fill_round(card, 10.0, pal.chrome_hi);
    r.stroke_round_rect(card, 10.0, 1.0, pal.shadow_ring);
    if cap.closing.is_none() {
        hits.push((card, Hit::CapCard));
    }

    let f18 = fmts.get(r, None, 18.0, true);
    let f125 = fmts.get(r, None, 12.5, false);
    let f115 = fmts.get(r, None, 11.5, false);
    let f12s = fmts.get(r, None, 12.0, true);
    let f12 = fmts.get(r, None, 12.0, false);
    let inner_l = card.left + PAD;
    let inner_r = card.right - PAD;
    let mut y = card.top + PAD;
    let title = ellipsize(r, cap.cmd.title(), &f18, inner_r - inner_l);
    r.text(&title, &f18, Rect::new(inner_l, y, inner_r, y + TITLE_H), pal.text);
    y += TITLE_H + 4.0;

    let current = notty_input::binding(cfg, cap.cmd);
    let confirm = if cap.captured.is_some() {
        crate::Anim::new_maybe(cap.captured_at, Duration::from_millis(CONFIRM_MS), animations_enabled).value(now, 0.0, 1.0)
    } else {
        0.0
    };
    let (line, line_c) = match cap.captured {
        Some((vk, m)) => (format!("Nueva combinación: {}", notty_input::format_key_spec(vk, m).unwrap_or_default()), pal.text.mix(GREEN, confirm)),
        None => {
            let now_s = notty_input::binding_spec(cfg, cap.cmd);
            let now_s = if now_s.is_empty() { "sin atajo".to_string() } else { now_s };
            (format!("Pulsa la nueva combinación · ahora: {now_s}"), pal.text_2)
        }
    };
    let line = ellipsize(r, &line, &f125, inner_r - inner_l);
    r.text(&line, &f125, Rect::new(inner_l, y, inner_r, y + LINE_H), line_c);
    y += LINE_H + 14.0;

    let shown = cap.captured.or(current);
    let highlighted = shown.map(|(vk, m)| keyboard_widget::combo_vks(vk, m)).unwrap_or_default();
    let pressed = if cap.closing.is_none() { keyboard_widget::held_keys() } else { Vec::new() };
    let pulse = if cap.captured.is_some() || !animations_enabled {
        1.0
    } else {
        let s = now.saturating_duration_since(cap.opened).as_secs_f32();
        0.5 + 0.5 * (s * std::f32::consts::TAU * 0.9).sin()
    };
    let kb_w = u * keyboard_widget::UNITS_W;
    let kb_left = card.left + (card.width() - kb_w) / 2.0;
    keyboard_widget::draw(r, pal, kb_left, y, u, &highlighted, &pressed, pulse, confirm);
    y += u * keyboard_widget::UNITS_H + 14.0;

    let conflict = cap.captured.and_then(|k| notty_input::conflict(cfg, cap.cmd, k));
    let (msg, msg_c) = if let Some(e) = cap.error {
        (e.to_string(), pal.warn)
    } else if let Some(other) = conflict {
        (format!("Ya lo usa «{}». Si lo guardas, esa acción se quedará sin atajo.", other.title()), pal.warn)
    } else if let Some(name) = cap.captured.and_then(|(vk, m)| editor_action_name(vk, m)) {
        (format!("En el editor también es «{name}»; el atajo nuevo tendrá prioridad."), pal.text_3)
    } else {
        ("Esc cancela · Enter guarda".to_string(), pal.text_3)
    };
    r.push_clip(Rect::new(inner_l, y, inner_r, y + MSG_H));
    r.text_wrapped(&msg, &f115, Rect::new(inner_l, y, inner_r, y + MSG_H), msg_c, Some(11.5 * 1.45));
    r.pop_clip();
    y += MSG_H + 14.0;

    let can_save = cap.captured.is_some();
    let save_label = if conflict.is_some() { "Reemplazar" } else { "Guardar" };
    let save_w = r.measure(save_label, &f12s) + 28.0;
    let save_r = Rect::new(inner_r - save_w, y, inner_r, y + BTN_H);
    let save_bg = if !can_save {
        pal.accent.faded(0.4)
    } else if hover == Hit::CapSave {
        pal.accent.mix(pal.text, 0.15)
    } else {
        pal.accent
    };
    r.fill_round(save_r, 6.0, save_bg);
    r.text_center(save_label, &f12s, save_r, if can_save { pal.on_accent } else { pal.on_accent.faded(0.6) });
    if can_save && cap.closing.is_none() {
        hits.push((save_r, Hit::CapSave));
    }

    let cancel_w = r.measure("Cancelar", &f12) + 28.0;
    let cancel_r = Rect::new(save_r.left - 8.0 - cancel_w, y, save_r.left - 8.0, y + BTN_H);
    r.fill_round(cancel_r, 6.0, if hover == Hit::CapCancel { pal.line } else { pal.chrome });
    r.text_center("Cancelar", &f12, cancel_r, pal.text);
    if cap.closing.is_none() {
        hits.push((cancel_r, Hit::CapCancel));
    }

    let default = notty_input::parse_key_spec(cap.cmd.default_spec());
    if shown != default {
        let label = format!("Restablecer ({})", cap.cmd.default_spec());
        let label = ellipsize(r, &label, &f12, (cancel_r.left - 12.0 - inner_l).max(0.0));
        let lw = r.measure(&label, &f12);
        let lr = Rect::new(inner_l, y, inner_l + lw, y + BTN_H);
        r.text(&label, &f12, lr, pal.accent);
        if hover == Hit::CapReset {
            let uy = lr.top + lr.height() / 2.0 + 9.0;
            r.stroke_line(lr.left, uy, lr.right, uy, 1.0, pal.accent);
        }
        if cap.closing.is_none() {
            hits.push((lr, Hit::CapReset));
        }
    }

    r.reset_transform();
    r.set_fade(1.0);
}
