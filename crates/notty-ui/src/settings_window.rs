//! Ventana de Ajustes, dibujada con Direct2D igual que la principal (`docs/mockups/
//! notty-ui.html` → «Ajustes»): columna de navegación + panel con las filas de
//! `settings_model::sections`. Reutiliza `Renderer` (helpers de dibujo, formatos de
//! texto) y la misma técnica de barra de título propia que `window.rs`. Se puede
//! redimensionar: navegación y filas se recolocan a partir de `size_dips()` y el
//! panel se desplaza (rueda o barra) cuando su contenido no cabe.

use std::cell::RefCell;
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow, ScreenToClient,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetClientRect, GetMessageW,
    GetWindowLongPtrW, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT,
    HTTOP, HTTOPLEFT, HTTOPRIGHT, IDC_ARROW, IsIconic, IsZoomed, KillTimer, LoadCursorW, MINMAXINFO, MSG,
    NCCALCSIZE_PARAMS, PostMessageW, RegisterClassExW, SC_KEYMENU, SM_CXFRAME, SM_CXPADDEDBORDER, SW_RESTORE,
    SW_SHOW, SWP_FRAMECHANGED, SWP_NOZORDER, SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, TranslateMessage, WM_CLOSE, WM_DESTROY,
    WM_DPICHANGED, WM_GETMINMAXINFO, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCHITTEST, WM_PAINT, WM_SIZE, WM_SYSCHAR, WM_SYSCOMMAND, WM_SYSKEYDOWN,
    WM_SYSKEYUP, WM_TIMER, WNDCLASSEXW, WS_CLIPSIBLINGS, WS_POPUP, WS_THICKFRAME,
};
use windows::core::{PCWSTR, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

use notty_config::Config;
use notty_input::Command;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::keyboard_widget::{self, GREEN};
use crate::layout::Rect;
use crate::settings_model::{Row, SettingKey, SettingValue};
use crate::theme::{self, Rgba};

/// Geometría de un `Select` abierto, para dibujar su desplegable al final de `paint`
/// (igual que `pending_dropdown` en `render.rs`): fila, caja y sus opciones.
/// `(fila, caja, opciones, índice de la opción actualmente seleccionada)`.
type OpenSelectGeom = (usize, Rect, &'static [(&'static str, SettingValue)], usize);
use crate::{Renderer, is_dark};

const TITLEBAR_H: f32 = 36.0;
/// Tamaño inicial (se recorta al área de trabajo del monitor) y mínimo, en DIPs.
const WIN_W: f32 = 820.0;
const WIN_H: f32 = 620.0;
const MIN_W: f32 = 560.0;
const MIN_H: f32 = 420.0;
const NAV_W: f32 = 200.0;
const NAV_W_NARROW: f32 = 160.0;
const CLOSE_W: f32 = 46.0;
/// Grosor de la franja de redimensionar en los bordes (`WM_NCHITTEST`).
const RESIZE_BORDER: f32 = 6.0;
/// Ancho máximo de las filas: en ventanas muy anchas no se estiran hasta el borde.
const ROW_MAX_W: f32 = 760.0;
/// Por debajo de este ancho de texto, el control de la fila baja a su propia línea.
const MIN_TEXT_W: f32 = 150.0;
const WHEEL_STEP: f32 = 48.0;

/// Id del `SetTimer` de animación de esta ventana (interruptores + fundido/escala de
/// apertura), igual que `ID_ANIM_TIMER` en `window.rs` pero local a esta ventana.
const ID_ANIM_TIMER: usize = 1;

// Convención del proyecto (mismos valores que `welcome_window`/`notty-setup::ui`):
// salida 140ms (fundido) y entrada 180ms (fundido), al cambiar de sección en el
// panel derecho (Task 4 del plan de animaciones).
const SECTION_OUT_MS: u64 = 140;
const SECTION_IN_MS: u64 = 180;
/// Captura de atajo: misma entrada/salida que el resto de transiciones.
const CAPTURE_IN_MS: u64 = 180;
const CAPTURE_OUT_MS: u64 = 140;
/// Paso de azul a verde de las teclas al capturar una combinación.
const CONFIRM_MS: u64 = 220;

/// Sección desde la que se sale y cuándo empezó la transición, igual que
/// `welcome_window::Transition`: primero se funde fuera `from` (`SECTION_OUT_MS`),
/// luego entra la sección ya activa (`SECTION_IN_MS`).
#[derive(Debug, Clone, Copy)]
struct SectionTransition {
    from: usize,
    from_scroll: f32,
    start: Instant,
}

/// Capa de "pulsa la nueva combinación" para un `Row::Binding`.
#[derive(Debug, Clone, Copy)]
struct Capture {
    cmd: Command,
    opened: Instant,
    /// `Some` mientras se funde fuera; al terminar se suelta la captura.
    closing: Option<Instant>,
    captured: Option<(u32, notty_input::Modifiers)>,
    captured_at: Instant,
    /// Aviso de tecla no válida ("combínala con Ctrl o Alt"...).
    error: Option<&'static str>,
}

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
    /// Clic en un `Row::Link` de la sección activa (`row` = su índice): qué acción
    /// dispara se decide en `handle_click` según el `action` de esa fila.
    Link(usize),
    /// Fila `row` (índice dentro de la sección activa), control `Toggle`.
    Toggle(usize),
    /// Fila `row`, opción `opt` de un `Seg`.
    Seg(usize, usize),
    /// Abre/cierra el desplegable de la fila `row` (`Select`).
    SelectBox(usize),
    /// Opción `opt` del desplegable abierto.
    SelectOption(usize),
    /// Fila `row`, atajo reasignable: abre la captura.
    Binding(usize),
    ScrollThumb,
    ScrollTrack,
    CapSave,
    CapCancel,
    CapReset,
    /// La tarjeta de la captura (se traga el clic).
    CapCard,
    /// Fuera de la tarjeta: cancela.
    CapBackdrop,
}

struct State {
    cfg: Rc<RefCell<Config>>,
    on_change: Box<dyn Fn()>,
    on_open_path: Box<dyn Fn(std::path::PathBuf)>,
    /// "Buscar ahora" en Acerca de (Task 4, Step 4 del plan del actualizador):
    /// dispara el mismo chequeo que el automático, pero en caliente y siempre con
    /// un mensaje (nunca en silencio, a diferencia del chequeo de arranque).
    on_check_updates: Box<dyn Fn()>,
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
    /// Transición en curso al cambiar de sección en el nav (Task 4 del plan de
    /// animaciones): fundido cruzado del panel derecho, mismo tratamiento (fases y
    /// duraciones) que `welcome_window` entre pasos. `None` en reposo.
    section_transition: Option<SectionTransition>,
    /// Cambio de tema en curso: de qué tema se viene (`true` = oscuro) y cuándo.
    theme_from: Option<(bool, Instant)>,
    /// Si el `SetTimer` de animación (`ID_ANIM_TIMER`) está corriendo.
    anim_timer_running: bool,
    /// Desplazamiento vertical del panel de la sección activa, en DIPs.
    scroll: f32,
    /// Alto total del contenido de la sección activa en el último `paint`.
    content_h: f32,
    /// Alto visible del panel en el último `paint`.
    panel_h: f32,
    /// Arrastre de la barra: `(y del ratón al empezar, scroll al empezar)`.
    scroll_drag: Option<(f32, f32)>,
    capture: Option<Capture>,
}

const THEME_MS: u64 = 350;

/// Arranca el temporizador de animación (60Hz) de esta ventana si no estaba corriendo.
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
/// `start_section` es el `id` de la sección con la que se abre (`""` = la primera).
#[allow(clippy::too_many_arguments)]
pub fn open(
    parent: HWND,
    cfg: Rc<RefCell<Config>>,
    on_change: Box<dyn Fn()>,
    on_open_path: Box<dyn Fn(std::path::PathBuf)>,
    on_check_updates: Box<dyn Fn()>,
    on_repeat_tutorial: Box<dyn Fn()>,
    start_section: &str,
    on_created: impl Fn(HWND),
) -> Result<()> {
    let start_index =
        crate::settings_model::sections(&cfg.borrow()).iter().position(|s| s.id == start_section).unwrap_or(0);
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
        // `WS_THICKFRAME` para que Windows haga el redimensionado de verdad a partir de
        // los `HT*` de `WM_NCHITTEST`; `WM_NCCALCSIZE` quita el marco que añadiría.
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
        let state = Box::new(State {
            cfg,
            on_change,
            on_open_path,
            on_check_updates,
            on_repeat_tutorial,
            renderer,
            active_section: start_index,
            hover: Hit::None,
            open_select: None,
            hits: Vec::new(),
            animations_enabled,
            open_anim: Some(crate::Anim::new_maybe(Instant::now(), Duration::from_millis(150), animations_enabled)),
            toggle_anims: HashMap::new(),
            select_open_anim: None,
            section_transition: None,
            theme_from: None,
            anim_timer_running: false,
            scroll: 0.0,
            content_h: 0.0,
            panel_h: 0.0,
            scroll_drag: None,
            capture: None,
        });
        let ptr = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
        if let Some(st) = ptr.as_mut() {
            ensure_anim_timer(st, hwnd);
        }

        let _ = SetWindowPos(hwnd, None, x, y, w_px, h_px, SWP_NOZORDER | SWP_FRAMECHANGED);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetWindowPos(hwnd, None, x, y, w_px, h_px, SWP_NOZORDER);
        // El renderer nació con el área cliente que aún tenía marco: se ajusta ya.
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
    }
    Ok(())
}

/// Tamaño por defecto recortado al área de trabajo del monitor de `parent`, centrado
/// sobre `parent` y metido dentro de ese área si se sale por algún lado.
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
    let x = (lparam.0 as i16) as f32;
    let y = ((lparam.0 >> 16) as i16) as f32;
    (x, y)
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
                }
                let _ = windows::Win32::Graphics::Gdi::ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }
            WM_NCCALCSIZE if wparam.0 != 0 => {
                // Sin marco: el área cliente es la ventana entera. Maximizada (p.ej. por
                // Aero Snap), Windows la saca el grosor del marco por cada lado.
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
                    // La ✕ de la esquina gana a la esquina de redimensionar.
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
                    let (x, y) = (x / scale, y / scale);
                    handle_click(hwnd, st, x, y);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if let Some(st) = ptr.as_mut() {
                    let (x, y) = point_from_lparam(lparam);
                    let scale = st.renderer.scale();
                    let (x, y) = (x / scale, y / scale);
                    if let Some((y0, s0)) = st.scroll_drag {
                        let (_, h) = st.renderer.size_dips();
                        let (track, thumb) = scrollbar_geom(st, h);
                        let free = (track.height() - thumb.height()).max(1.0);
                        let max = max_scroll(st);
                        st.scroll = (s0 + (y - y0) * max / free).clamp(0.0, max);
                        invalidate(hwnd);
                        return LRESULT(0);
                    }
                    let hit = hit_test(st, x, y);
                    if hit != st.hover {
                        st.hover = hit;
                        invalidate(hwnd);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if let Some(st) = ptr.as_mut() {
                    if st.scroll_drag.take().is_some() {
                        invalidate(hwnd);
                    }
                }
                let _ = ReleaseCapture();
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                if let Some(st) = ptr.as_mut() {
                    if st.capture.is_none() {
                        let delta = ((wparam.0 >> 16) as i16) as f32 / 120.0;
                        scroll_by(st, -delta * WHEEL_STEP);
                        st.open_select = None;
                        invalidate(hwnd);
                    }
                }
                LRESULT(0)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if let Some(st) = ptr.as_mut() {
                    if capture_active(st) {
                        let repeat = (lparam.0 >> 30) & 1 == 1;
                        capture_key(st, hwnd, wparam.0 as u32, repeat);
                        return LRESULT(0);
                    }
                    if msg == WM_KEYDOWN {
                        handle_nav_key(st, hwnd, wparam.0 as u32);
                        return LRESULT(0);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KEYUP | WM_SYSKEYUP | WM_SYSCHAR => {
                // Durante la captura, soltar Alt no debe abrir el menú de sistema ni
                // un `WM_SYSCHAR` pitar: solo se repinta (teclas en verde en vivo).
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
                        st.toggle_anims.retain(|_, (a, _)| !a.is_done(now));
                        if st.select_open_anim.is_some_and(|a| a.is_done(now)) {
                            st.select_open_anim = None;
                        }
                        let section_total = Duration::from_millis(SECTION_OUT_MS.max(SECTION_IN_MS));
                        if st.section_transition.is_some_and(|t| !st.animations_enabled || now.saturating_duration_since(t.start) >= section_total) {
                            st.section_transition = None;
                        }
                        if st.theme_from.is_some_and(|(_, t0)| now.saturating_duration_since(t0) >= Duration::from_millis(THEME_MS)) {
                            st.theme_from = None;
                        }
                        let out_d = Duration::from_millis(CAPTURE_OUT_MS);
                        if st.capture.is_some_and(|c| {
                            c.closing.is_some_and(|t0| !st.animations_enabled || now.saturating_duration_since(t0) >= out_d)
                        }) {
                            st.capture = None;
                        }
                        let still_animating = st.open_anim.is_some()
                            || st.theme_from.is_some()
                            || !st.toggle_anims.is_empty()
                            || st.select_open_anim.is_some()
                            || st.section_transition.is_some()
                            || st.capture.is_some();
                        if !still_animating {
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
                // null y dejaría el WM_QUIT en la cola para el bucle principal, que
                // cerraría toda la app.
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// Teclas fuera de la captura: Esc cierra el desplegable o Ajustes; RePág/AvPág,
/// Inicio/Fin desplazan el panel.
fn handle_nav_key(st: &mut State, hwnd: HWND, vk: u32) {
    let page = (st.panel_h - 40.0).max(WHEEL_STEP);
    match vk {
        0x1B => {
            if st.open_select.take().is_some() {
                invalidate(hwnd);
            } else {
                post_close(hwnd);
            }
        }
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

fn max_scroll(st: &State) -> f32 {
    (st.content_h - st.panel_h).max(0.0)
}

fn scroll_by(st: &mut State, dy: f32) {
    st.scroll = (st.scroll + dy).clamp(0.0, max_scroll(st));
}

/// Pista y pulgar de la barra de desplazamiento del panel (el pulgar vacío si no
/// hace falta barra).
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

fn goto_section(st: &mut State, hwnd: HWND, i: usize) {
    if i != st.active_section {
        st.section_transition =
            Some(SectionTransition { from: st.active_section, from_scroll: st.scroll, start: Instant::now() });
        st.active_section = i;
        st.scroll = 0.0;
        st.open_select = None;
        st.toggle_anims.clear();
        ensure_anim_timer(st, hwnd);
    }
    invalidate(hwnd);
}

/// Ventana de Ajustes ya abierta: en vez de crear otra encima (Task del pedido de
/// usuario "no debería dejarte abrir varias pestañas de ajustes a la vez"),
/// `open_settings_at` llama aquí para llevar el foco a la que ya existe y, si se pidió
/// una sección concreta, cambiar a ella. `hwnd` viene de `on_created` (ver `open`).
pub fn focus_existing(hwnd: HWND, section: &str) {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
        if section.is_empty() {
            return;
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if let Some(st) = ptr.as_mut() {
            let idx = crate::settings_model::sections(&st.cfg.borrow()).iter().position(|s| s.id == section);
            if let Some(i) = idx {
                goto_section(st, hwnd, i);
            }
        }
    }
}

fn handle_click(hwnd: HWND, st: &mut State, x: f32, y: f32) {
    let hit = hit_test(st, x, y);
    match hit {
        Hit::Close => post_close(hwnd),
        Hit::Nav(i) => {
            goto_section(st, hwnd, i);
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
                Some(crate::settings_model::LinkAction::CheckUpdatesNow) => (st.on_check_updates)(),
                Some(crate::settings_model::LinkAction::RepeatTutorial) => {
                    (st.on_repeat_tutorial)();
                    post_close(hwnd);
                }
                None => {}
            }
        }
        Hit::Toggle(row) => {
            toggle_row(st, row, hwnd);
            invalidate(hwnd);
        }
        Hit::Seg(row, opt) => {
            set_row_option(st, hwnd, row, opt);
            invalidate(hwnd);
        }
        Hit::SelectBox(row) => {
            st.open_select = if st.open_select == Some(row) { None } else { Some(row) };
            if st.open_select.is_some() {
                st.select_open_anim =
                    Some(crate::Anim::new_maybe(Instant::now(), Duration::from_millis(120), st.animations_enabled));
                ensure_anim_timer(st, hwnd);
            }
            invalidate(hwnd);
        }
        Hit::SelectOption(opt) => {
            if let Some(row) = st.open_select.take() {
                set_row_option(st, hwnd, row, opt);
            }
            invalidate(hwnd);
        }
        Hit::Binding(row) => {
            let cmd = crate::settings_model::sections(&st.cfg.borrow())[st.active_section].rows.get(row).and_then(|r| {
                match r {
                    Row::Binding { cmd } => Some(*cmd),
                    _ => None,
                }
            });
            if let Some(cmd) = cmd {
                let now = Instant::now();
                st.open_select = None;
                st.capture = Some(Capture { cmd, opened: now, closing: None, captured: None, captured_at: now, error: None });
                unsafe {
                    let _ = SetFocus(Some(hwnd));
                }
                ensure_anim_timer(st, hwnd);
            }
            invalidate(hwnd);
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
            invalidate(hwnd);
        }
        Hit::CapSave => capture_save(st, hwnd),
        Hit::CapCancel | Hit::CapBackdrop => close_capture(st, hwnd),
        Hit::CapReset => {
            if let Some(cap) = st.capture.as_mut() {
                cap.captured = notty_input::parse_key_spec(cap.cmd.default_spec());
                cap.captured_at = Instant::now();
                cap.error = None;
            }
            invalidate(hwnd);
        }
        Hit::CapCard => {}
        Hit::None | Hit::Caption => {
            if st.open_select.take().is_some() {
                invalidate(hwnd);
            }
        }
    }
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
        SettingKey::Autosave => cfg.files.autosave,
        SettingKey::StartWithWindows => cfg.hotkey.start_with_windows,
        SettingKey::UpdatesCheck => cfg.updates.check,
        _ => false,
    }
}

/// Fija la fila `row` (de la sección activa) a su opción `opt` (usado por `Seg`,
/// `Select` y su desplegable): aplica el `SettingValue` de esa opción con
/// `settings_model::apply`.
fn set_row_option(st: &mut State, hwnd: HWND, row: usize, opt: usize) {
    let system_dark = crate::window::system_uses_dark_mode();
    let was_dark = is_dark(st.cfg.borrow().ui.theme, system_dark);
    set_row_option_inner(st, row, opt);
    let now_dark = is_dark(st.cfg.borrow().ui.theme, system_dark);
    if now_dark != was_dark {
        st.theme_from = Some((was_dark, Instant::now()));
        ensure_anim_timer(st, hwnd);
        // Sin esto, el marco que sigue dibujando DWM (los ~4px de borde nativo) se queda
        // con el tema viejo: se ve un contorno claro alrededor de una ventana ya oscura.
        unsafe { crate::window::apply_dark_mode(hwnd, now_dark) };
    }
}

fn set_row_option_inner(st: &mut State, row: usize, opt: usize) {
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
        SettingKey::TempMode => SettingValue::TempMode(cfg.files.temp_mode),
        SettingKey::HotkeyMechanism => SettingValue::HotkeyMechanism(cfg.hotkey.mechanism),
        _ => return None,
    };
    options.iter().position(|(_, v)| *v == current)
}

// --- Dibujo ---------------------------------------------------------------------------

/// Recorta `s` con «…» para que quepa en `max_w` con `fmt` (búsqueda binaria sobre el
/// número de caracteres; las cadenas de Ajustes son cortas).
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

fn nav_width(w: f32) -> f32 {
    if w < 680.0 { NAV_W_NARROW } else { NAV_W }
}

fn paint(st: &mut State) {
    st.hits.clear();
    let dark = is_dark(st.cfg.borrow().ui.theme, crate::window::system_uses_dark_mode());
    let pal_mixed;
    let pal = match st.theme_from {
        Some((from_dark, t0)) if st.animations_enabled => {
            let p = (Instant::now().saturating_duration_since(t0).as_secs_f32() / (THEME_MS as f32 / 1000.0)).clamp(0.0, 1.0);
            pal_mixed = theme::palette(from_dark).mix(theme::palette(dark), crate::ease_out_cubic(p));
            &pal_mixed
        }
        _ => theme::palette(dark),
    };
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
    let nav_w = nav_width(w);
    let nav = Rect::new(0.0, TITLEBAR_H, nav_w, h);
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
        let name = ellipsize(r, sec.name, &r.fonts().ui_13, br.width() - 20.0);
        r.text(&name, &r.fonts().ui_13, Rect::new(br.left + 12.0, br.top, br.right - 8.0, br.bottom), tc);
        st.hits.push((br, Hit::Nav(i)));
        ny += 36.0;
    }
    // Pie: "Todo se guarda en config.toml." + "Editar el archivo". Si no cabe en una
    // línea (navegación estrecha), "config.toml." baja a la siguiente.
    let prefix = "Todo se guarda en ";
    let prefix_w = r.measure(prefix, &r.fonts().ui_11_5);
    let bold_w = r.measure("config.toml", &r.fonts().ui_11_5_semibold);
    let dot_w = r.measure(".", &r.fonts().ui_11_5);
    let foot_left = nav.left + 10.0;
    let foot_right = nav.right - 10.0;
    let one_line = prefix_w + bold_w + dot_w <= foot_right - foot_left;
    let foot_h = if one_line { 44.0 } else { 62.0 };
    let foot_top = nav.bottom - foot_h;
    if foot_top >= ny + 4.0 {
        let (bx, by) = if one_line {
            r.text(prefix, &r.fonts().ui_11_5, Rect::new(foot_left, foot_top, foot_left + prefix_w, foot_top + 18.0), pal.text_3);
            (foot_left + prefix_w, foot_top)
        } else {
            r.text(prefix.trim_end(), &r.fonts().ui_11_5, Rect::new(foot_left, foot_top, foot_right, foot_top + 18.0), pal.text_3);
            (foot_left, foot_top + 18.0)
        };
        r.text("config.toml", &r.fonts().ui_11_5_semibold, Rect::new(bx, by, bx + bold_w, by + 18.0), pal.text_3);
        r.text(".", &r.fonts().ui_11_5, Rect::new(bx + bold_w, by, foot_right.max(bx + bold_w + dot_w), by + 18.0), pal.text_3);
        let link_r = Rect::new(foot_left, by + 18.0, foot_right, by + 36.0);
        r.text("Editar el archivo", &r.fonts().ui_11_5, link_r, pal.accent);
        if st.hover == Hit::EditConfig {
            // La maqueta subraya estos enlaces al pasar el ratón (`.snav .foot a:hover`),
            // en vez de dejar el color de acento como único estado (Task 7 del plan).
            let lw = r.measure("Editar el archivo", &r.fonts().ui_11_5);
            let uy = link_r.bottom - 3.0;
            r.stroke_line(link_r.left, uy, link_r.left + lw, uy, 1.0, pal.accent);
        }
        st.hits.push((link_r, Hit::EditConfig));
    }

    // Panel de la sección activa: fundido cruzado al cambiar de sección (Task 4 del
    // plan de animaciones), mismas dos fases/duraciones que `welcome_window` entre
    // pasos — ver `SectionTransition`. Sin una transición en curso (`None`, el caso
    // de siempre) se pinta en una sola pasada, exactamente igual que antes de esto.
    // El contenido se dibuja desplazado `scroll` y recortado al panel.
    let panel = Rect::new(nav_w, TITLEBAR_H, w, h);
    r.fill(panel, pal.surface);
    st.panel_h = panel.height();
    st.scroll = st.scroll.clamp(0.0, max_scroll(st));
    let shifted = |s: f32| Rect::new(panel.left, panel.top - s, panel.right, panel.bottom - s);
    r.push_clip(panel);

    let now = Instant::now();
    let out_d = Duration::from_millis(SECTION_OUT_MS);
    let in_d = Duration::from_millis(SECTION_IN_MS);
    // Fundido cruzado: la sección de la que se viene se va (140ms) mientras la nueva
    // entra a la vez (180ms). Antes la nueva se pintaba opaca durante la salida y luego
    // volvía a 0 para entrar: ese aparecer-desaparecer era el parpadeo.
    let in_start = match st.section_transition {
        Some(t) => {
            if st.animations_enabled && now < t.start + out_d {
                // Fantasma: sin hits, sin toggles animados ni desplegable.
                let fade = 1.0 - crate::Anim::new_maybe(t.start, out_d, st.animations_enabled).value(now, 0.0, 1.0);
                let base = r.fade();
                r.set_fade(base * fade);
                draw_section_panel(r, pal, shifted(t.from_scroll), &st.cfg.borrow(), &sections[t.from], st.hover, &HashMap::new(), None, None);
                r.set_fade(base);
            }
            Some(t.start)
        }
        None => None,
    };
    // Entrada de la sección ya activa (siempre se dibuja): 180ms fundiéndose desde 0
    // si viene justo detrás de una salida, o a fundido 1.0 (sin coste extra) en reposo.
    let fade_in = match in_start {
        Some(start) => crate::Anim::new_maybe(start, in_d, st.animations_enabled).value(now, 0.0, 1.0),
        None => 1.0,
    };
    let section = &sections[st.active_section];
    let base_fade = r.fade();
    if fade_in < 1.0 {
        r.set_fade(base_fade * fade_in);
    }
    let mut section_hits = Vec::new();
    let slide = 10.0 * (1.0 - fade_in);
    let (open_select_geom, content_h) = draw_section_panel(
        r,
        pal,
        Rect::new(panel.left + slide, panel.top - st.scroll, panel.right + slide, panel.bottom - st.scroll),
        &st.cfg.borrow(),
        section,
        st.hover,
        &st.toggle_anims,
        st.open_select,
        Some(&mut section_hits),
    );
    if fade_in < 1.0 {
        r.set_fade(base_fade);
    }
    r.pop_clip();
    for (rc, hit) in section_hits {
        let c = Rect::new(rc.left, rc.top.max(panel.top), rc.right, rc.bottom.min(panel.bottom));
        if !c.is_empty() {
            st.hits.push((c, hit));
        }
    }
    st.content_h = content_h;
    st.scroll = st.scroll.min(max_scroll(st));

    // Barra de desplazamiento fina, solo si el contenido no cabe.
    if max_scroll(st) > 0.0 {
        let (track, thumb) = scrollbar_geom(st, h);
        let active = st.scroll_drag.is_some() || st.hover == Hit::ScrollThumb;
        let c = if active { pal.text_2 } else { pal.text_3 };
        r.fill_round(thumb, thumb.width() / 2.0, c);
        st.hits.push((track, Hit::ScrollTrack));
        st.hits.push((Rect::new(thumb.left - 2.0, thumb.top, thumb.right + 2.0, thumb.bottom), Hit::ScrollThumb));
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
        // Si no cabe debajo de la caja, se abre hacia arriba.
        let below = Rect::new(box_r.left, box_r.bottom + 2.0, box_r.right, box_r.bottom + 2.0 + dd_h);
        let dd = if below.bottom > h - 6.0 && box_r.top - 2.0 - dd_h >= TITLEBAR_H {
            Rect::new(box_r.left, box_r.top - 2.0 - dd_h, box_r.right, box_r.top - 2.0)
        } else {
            below
        };
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

    if let Some(cap) = st.capture {
        draw_capture(r, pal, &cap, &st.cfg.borrow(), w, h, st.hover, &mut st.hits, st.animations_enabled);
    }

    r.end_paint();
}

/// Capa de captura de atajo: velo sobre el panel y tarjeta con el nombre de la
/// acción, el teclado (combinación actual en azul, teclas pulsadas en verde) y
/// Guardar/Cancelar/Restablecer. Entra con fundido + escala (180ms) y sale en 140ms.
#[allow(clippy::too_many_arguments)]
fn draw_capture(
    r: &Renderer,
    pal: &theme::Palette,
    cap: &Capture,
    cfg: &Config,
    w: f32,
    h: f32,
    hover: Hit,
    hits: &mut Vec<(Rect, Hit)>,
    animations_enabled: bool,
) {
    let now = Instant::now();
    let t = match cap.closing {
        Some(t0) => 1.0 - crate::Anim::new_maybe(t0, Duration::from_millis(CAPTURE_OUT_MS), animations_enabled).value(now, 0.0, 1.0),
        None => crate::Anim::new_maybe(cap.opened, Duration::from_millis(CAPTURE_IN_MS), animations_enabled).value(now, 0.0, 1.0),
    };
    if t <= 0.0 {
        return;
    }

    let area = Rect::new(0.0, TITLEBAR_H, w, h);
    r.fill(area, Rgba(0.0, 0.0, 0.0, 0.45 * t));
    if cap.closing.is_none() {
        hits.push((area, Hit::CapBackdrop));
    }

    const PAD: f32 = 20.0;
    const TITLE_H: f32 = 26.0;
    const LINE_H: f32 = 18.0;
    const MSG_H: f32 = 36.0;
    const BTN_H: f32 = 30.0;
    let fixed_h = PAD + TITLE_H + 4.0 + LINE_H + 14.0 + 14.0 + MSG_H + 14.0 + BTN_H + PAD;
    let avail_w = (w - 32.0).min(600.0);
    let avail_h = area.height() - 24.0;
    let u = ((avail_w - PAD * 2.0) / keyboard_widget::UNITS_W)
        .min((avail_h - fixed_h) / keyboard_widget::UNITS_H)
        .clamp(10.0, 26.0);
    let card_w = (u * keyboard_widget::UNITS_W + PAD * 2.0).max(avail_w.min(380.0));
    let card_h = fixed_h + u * keyboard_widget::UNITS_H;
    let cx = area.left + (area.width() - card_w) / 2.0;
    let cy = area.top + ((area.height() - card_h) / 2.0).max(8.0);
    let card = Rect::new(cx, cy, cx + card_w, cy + card_h);

    let scale = 0.96 + 0.04 * t;
    r.set_transform(Matrix3x2::scale_around(
        scale,
        scale,
        Vector2 { X: card.left + card.width() / 2.0, Y: card.top + card.height() / 2.0 },
    ));
    r.set_fade(t);

    r.draw_popup_shadow(card, 10.0, 1.0, pal.shadow);
    r.fill_round(card, 10.0, pal.chrome_hi);
    r.stroke_round_rect(card, 10.0, 1.0, pal.shadow_ring);
    if cap.closing.is_none() {
        hits.push((card, Hit::CapCard));
    }

    let inner_l = card.left + PAD;
    let inner_r = card.right - PAD;
    let mut y = card.top + PAD;
    let title = ellipsize(r, cap.cmd.title(), &r.fonts().ui_18_semibold, inner_r - inner_l);
    r.text(&title, &r.fonts().ui_18_semibold, Rect::new(inner_l, y, inner_r, y + TITLE_H), pal.text);
    y += TITLE_H + 4.0;

    let current = notty_input::binding(cfg, cap.cmd);
    let confirm = if cap.captured.is_some() {
        crate::Anim::new_maybe(cap.captured_at, Duration::from_millis(CONFIRM_MS), animations_enabled).value(now, 0.0, 1.0)
    } else {
        0.0
    };
    let (line, line_c) = match cap.captured {
        Some((vk, m)) => (
            format!("Nueva combinación: {}", notty_input::format_key_spec(vk, m).unwrap_or_default()),
            pal.text.mix(GREEN, confirm),
        ),
        None => {
            let now_s = notty_input::binding_spec(cfg, cap.cmd);
            let now_s = if now_s.is_empty() { "sin atajo".to_string() } else { now_s };
            (format!("Pulsa la nueva combinación · ahora: {now_s}"), pal.text_2)
        }
    };
    let line = ellipsize(r, &line, &r.fonts().ui_12_5, inner_r - inner_l);
    r.text(&line, &r.fonts().ui_12_5, Rect::new(inner_l, y, inner_r, y + LINE_H), line_c);
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

    // Aviso: tecla no válida, choque con otro comando, o con una acción del editor.
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
    r.text_wrapped(&msg, &r.fonts().ui_11_5, Rect::new(inner_l, y, inner_r, y + MSG_H), msg_c, Some(11.5 * 1.45));
    r.pop_clip();
    y += MSG_H + 14.0;

    // Botones: Restablecer (enlace, a la izquierda), Cancelar y Guardar/Reemplazar.
    let can_save = cap.captured.is_some();
    let save_label = if conflict.is_some() { "Reemplazar" } else { "Guardar" };
    let save_w = r.measure(save_label, &r.fonts().ui_12_5_semibold) + 28.0;
    let save_r = Rect::new(inner_r - save_w, y, inner_r, y + BTN_H);
    let save_bg = if !can_save {
        pal.accent.faded(0.35)
    } else if hover == Hit::CapSave {
        pal.accent.mix(pal.on_accent, 0.12)
    } else {
        pal.accent
    };
    r.fill_round(save_r, 6.0, save_bg);
    r.text_center(save_label, &r.fonts().ui_12_5_semibold, save_r, if can_save { pal.on_accent } else { pal.on_accent.faded(0.6) });
    if can_save && cap.closing.is_none() {
        hits.push((save_r, Hit::CapSave));
    }

    let cancel_w = r.measure("Cancelar", &r.fonts().ui_12_5) + 28.0;
    let cancel_r = Rect::new(save_r.left - 8.0 - cancel_w, y, save_r.left - 8.0, y + BTN_H);
    r.fill_round(cancel_r, 6.0, if hover == Hit::CapCancel { pal.press } else { pal.hover });
    r.text_center("Cancelar", &r.fonts().ui_12_5, cancel_r, pal.text);
    if cap.closing.is_none() {
        hits.push((cancel_r, Hit::CapCancel));
    }

    let default = notty_input::parse_key_spec(cap.cmd.default_spec());
    if shown != default {
        let label = format!("Restablecer ({})", cap.cmd.default_spec());
        let label = ellipsize(r, &label, &r.fonts().ui_12_5, (cancel_r.left - 12.0 - inner_l).max(0.0));
        let lw = r.measure(&label, &r.fonts().ui_12_5);
        let lr = Rect::new(inner_l, y, inner_l + lw, y + BTN_H);
        r.text(&label, &r.fonts().ui_12_5, lr, pal.accent);
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

/// Dónde va una fila: su fondo, el ancho que le toca al texto y el rect del control.
/// Si al lado del control no quedan `MIN_TEXT_W` para el texto, el control baja a su
/// propia línea bajo el título/descripción.
struct RowLayout {
    rr: Rect,
    text_w: f32,
    ctrl: Rect,
}

/// Interlineado de la descripción (`line-height` de la maqueta sobre su
/// `font-size:11.5px`), igual proporción que usa `welcome_window::draw_step_content`
/// para su subtítulo.
fn row_desc_line_h() -> f32 {
    11.5 * 1.45
}

/// Alto del título (17) y, si hay descripción, del hueco de 3 + la descripción ya
/// envuelta a `text_w` (nunca menos de una línea de 16).
fn text_block_h(r: &Renderer, desc: &str, text_w: f32) -> f32 {
    if desc.is_empty() {
        17.0
    } else {
        17.0 + 3.0 + r.measure_wrapped(desc, &r.fonts().ui_11_5, text_w, Some(row_desc_line_h())).max(16.0)
    }
}

/// `.srow`: 14 de relleno arriba y abajo alrededor del texto, control centrado a la
/// derecha (o debajo, ver `RowLayout`).
fn row_layout(r: &Renderer, left: f32, right: f32, top: f32, desc: &str, ctrl_w: f32, ctrl_h: f32) -> RowLayout {
    let inner = (right - left - 24.0).max(0.0);
    let inline_text_w = inner - ctrl_w - 16.0;
    if inline_text_w >= MIN_TEXT_W {
        let h = (28.0 + text_block_h(r, desc, inline_text_w)).max(ctrl_h + 12.0);
        let cy = top + (h - ctrl_h) / 2.0;
        RowLayout {
            rr: Rect::new(left, top, right, top + h),
            text_w: inline_text_w,
            ctrl: Rect::new(right - 12.0 - ctrl_w, cy, right - 12.0, cy + ctrl_h),
        }
    } else {
        let text_h = text_block_h(r, desc, inner);
        let ctrl_top = top + 14.0 + text_h + 8.0;
        let cw = ctrl_w.min(inner);
        RowLayout {
            rr: Rect::new(left, top, right, ctrl_top + ctrl_h + 14.0),
            text_w: inner,
            ctrl: Rect::new(left + 12.0, ctrl_top, left + 12.0 + cw, ctrl_top + ctrl_h),
        }
    }
}

/// Dibuja el título y las filas de `section` dentro de `panel` (extraído de `paint`
/// para poder dibujar dos secciones en la misma pasada durante el fundido cruzado
/// del cambio de sección, Task 4 del plan de animaciones): la que se va (fantasma,
/// `hits: None`) y la que entra (interactiva, `hits: Some`). `panel.top` ya viene
/// desplazado por el scroll. Devuelve la geometría del desplegable abierto, si
/// `open_select` señala una fila `Select` de `section`, y el alto total del contenido.
#[allow(clippy::too_many_arguments)]
fn draw_section_panel(
    r: &Renderer,
    pal: &theme::Palette,
    panel: Rect,
    cfg: &Config,
    section: &crate::settings_model::Section,
    hover: Hit,
    toggle_anims: &HashMap<usize, (crate::Anim, bool)>,
    open_select: Option<usize>,
    mut hits: Option<&mut Vec<(Rect, Hit)>>,
) -> (Option<OpenSelectGeom>, f32) {
    let row_left = panel.left + 22.0;
    let row_right = (panel.right - 22.0).min(row_left + ROW_MAX_W).max(row_left + 40.0);

    let title_r = Rect::new(row_left, panel.top + 28.0, row_right, panel.top + 28.0 + 28.0);
    let custom = section.id == "apariencia" && cfg.ui.preset == notty_config::Preset::Custom;
    let suffix = " · Personalizado";
    let suffix_w = if custom { r.measure(suffix, &r.fonts().ui_12) } else { 0.0 };
    let name = ellipsize(r, section.name, &r.fonts().ui_20_semibold, (title_r.width() - suffix_w).max(20.0));
    let title_w = r.measure(&name, &r.fonts().ui_20_semibold);
    r.text(&name, &r.fonts().ui_20_semibold, title_r, pal.text);
    if custom {
        r.text(suffix, &r.fonts().ui_12, Rect::new(title_r.left + title_w, title_r.top, title_r.right, title_r.bottom), pal.text_3);
    }

    let mut ry = title_r.bottom + 14.0;
    let mut open_select_geom: Option<OpenSelectGeom> = None;

    for (i, row) in section.rows.iter().enumerate() {
        match row {
            Row::Group(label) => {
                let label = ellipsize(r, label, &r.fonts().ui_12, row_right - row_left);
                r.text(&label, &r.fonts().ui_12, Rect::new(row_left, ry, row_right, ry + 20.0), pal.text_2);
                ry += 24.0;
                continue;
            }
            Row::Toggle { title, desc, key } => {
                let lay = row_layout(r, row_left, row_right, ry, desc, 40.0, 20.0);
                r.fill_round(lay.rr, 6.0, pal.surface_2);
                draw_row_text(r, &lay, title, desc, pal);
                let on = current_bool(cfg, *key);
                let anim = toggle_anims.get(&i).copied();
                draw_toggle(r, lay.ctrl, on, pal, anim);
                if let Some(hits) = hits.as_deref_mut() {
                    hits.push((lay.rr, Hit::Toggle(i)));
                }
                ry += lay.rr.height() + 3.0;
            }
            Row::Seg { title, desc, key, options } => {
                let widths: Vec<f32> = options.iter().map(|(label, _)| r.measure(label, &r.fonts().ui_12) + 18.0).collect();
                let total_w: f32 = widths.iter().sum::<f32>() + 4.0;
                let lay = row_layout(r, row_left, row_right, ry, desc, total_w, 28.0);
                r.fill_round(lay.rr, 6.0, pal.surface_2);
                draw_row_text(r, &lay, title, desc, pal);
                let selected = selected_option_index(cfg, *key, options).unwrap_or(usize::MAX);
                let seg_r = draw_seg(r, lay.ctrl, options, &widths, selected, pal, hover, i);
                if let Some(hits) = hits.as_deref_mut() {
                    for (j, opt_r) in seg_r.iter().enumerate() {
                        hits.push((*opt_r, Hit::Seg(i, j)));
                    }
                }
                ry += lay.rr.height() + 3.0;
            }
            Row::Select { title, desc, key, options } => {
                let selected = selected_option_index(cfg, *key, options).unwrap_or(0);
                let label = options.get(selected).map(|(l, _)| *l).unwrap_or("");
                let box_w = 200.0f32.max(r.measure(label, &r.fonts().ui_12_5) + 40.0);
                let lay = row_layout(r, row_left, row_right, ry, desc, box_w, 30.0);
                r.fill_round(lay.rr, 6.0, pal.surface_2);
                draw_row_text(r, &lay, title, desc, pal);
                let box_r = lay.ctrl;
                r.fill(box_r, pal.surface);
                r.stroke_rect(box_r, 1.0, pal.line);
                let shown = ellipsize(r, label, &r.fonts().ui_12_5, (box_r.width() - 28.0).max(0.0));
                r.text(&shown, &r.fonts().ui_12_5, Rect::new(box_r.left + 8.0, box_r.top, box_r.right - 20.0, box_r.bottom), pal.text);
                r.text("˅", &r.fonts().ui_12_5, Rect::new(box_r.right - 20.0, box_r.top, box_r.right - 6.0, box_r.bottom), pal.text_2);
                if let Some(hits) = hits.as_deref_mut() {
                    hits.push((box_r, Hit::SelectBox(i)));
                }
                if open_select == Some(i) {
                    open_select_geom = Some((i, box_r, options, selected));
                }
                ry += lay.rr.height() + 3.0;
            }
            Row::Kbd { title, keys } => {
                let kw = r.measure(keys, &r.fonts().mono_11) + 10.0;
                let lay = row_layout(r, row_left, row_right, ry, "", kw, 20.0);
                r.fill_round(lay.rr, 6.0, pal.surface_2);
                draw_row_text(r, &lay, title, "", pal);
                r.fill_round(lay.ctrl, 4.0, pal.hover);
                // Igual que en el segmentado: `kw` ya lleva 5px de relleno a cada lado.
                let kr = lay.ctrl;
                r.text(keys, &r.fonts().mono_11, Rect::new(kr.left + 5.0, kr.top, kr.right - 5.0, kr.bottom), pal.text_2);
                ry += lay.rr.height() + 3.0;
            }
            Row::Binding { cmd } => {
                let spec = notty_input::binding_spec(cfg, *cmd);
                let (keys, font, kc) = if spec.is_empty() {
                    ("Sin asignar".to_string(), &r.fonts().ui_11_5, pal.text_3)
                } else {
                    (spec, &r.fonts().mono_11, pal.text)
                };
                let kw = r.measure(&keys, font) + 16.0;
                let lay = row_layout(r, row_left, row_right, ry, "", kw, 22.0);
                let hovered = hover == Hit::Binding(i);
                r.fill_round(lay.rr, 6.0, pal.surface_2);
                if hovered {
                    r.fill_round(lay.rr, 6.0, pal.hover);
                }
                draw_row_text(r, &lay, cmd.title(), "", pal);
                let kr = lay.ctrl;
                r.fill_round(kr, 4.0, if hovered { pal.accent_soft } else { pal.hover });
                if hovered {
                    r.stroke_round_rect(kr, 4.0, 1.0, pal.accent);
                }
                r.text_center(&keys, font, kr, if hovered { pal.accent } else { kc });
                if let Some(hits) = hits.as_deref_mut() {
                    hits.push((lay.rr, Hit::Binding(i)));
                }
                ry += lay.rr.height() + 3.0;
            }
            Row::Link { title, desc, label, .. } => {
                let lw = r.measure(label, &r.fonts().ui_13);
                let lay = row_layout(r, row_left, row_right, ry, desc, lw, 20.0);
                r.fill_round(lay.rr, 6.0, pal.surface_2);
                draw_row_text(r, &lay, title, desc, pal);
                let lr = lay.ctrl;
                r.text(label, &r.fonts().ui_13, lr, pal.accent);
                if hover == Hit::Link(i) {
                    // Mismo subrayado al pasar el ratón que el pie de la navegación.
                    let uy = (lr.top + lr.bottom) / 2.0 + 8.0;
                    r.stroke_line(lr.left, uy, lr.right, uy, 1.0, pal.accent);
                }
                if let Some(hits) = hits.as_deref_mut() {
                    hits.push((lay.rr, Hit::Link(i)));
                }
                ry += lay.rr.height() + 3.0;
            }
        }
    }
    (open_select_geom, ry - panel.top + 24.0)
}

fn draw_row_text(r: &Renderer, lay: &RowLayout, title: &str, desc: &str, pal: &theme::Palette) {
    let rr = lay.rr;
    let text_w = lay.text_w;
    let title_top = rr.top + 14.0;
    let title = ellipsize(r, title, &r.fonts().ui_13, text_w);
    r.text(&title, &r.fonts().ui_13, Rect::new(rr.left + 12.0, title_top, rr.left + 12.0 + text_w, title_top + 17.0), pal.text);
    if !desc.is_empty() {
        let desc_top = title_top + 17.0 + 3.0;
        let line_h = row_desc_line_h();
        let desc_h = r.measure_wrapped(desc, &r.fonts().ui_11_5, text_w, Some(line_h)).max(16.0);
        // `text_wrapped` envuelve por palabras y no centra verticalmente: pegada arriba,
        // con exactamente el alto medido (el mismo que ya reservó `row_layout`).
        r.text_wrapped(desc, &r.fonts().ui_11_5, Rect::new(rr.left + 12.0, desc_top, rr.left + 12.0 + text_w, desc_top + desc_h), pal.text_3, Some(line_h));
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

/// `.seg`: contenedor `hover` radio 6 (`seg_r`) con las opciones dentro, de anchos
/// `widths`; devuelve el rectángulo de cada opción (para el hit-testing).
#[allow(clippy::too_many_arguments)]
fn draw_seg(
    r: &Renderer,
    seg_r: Rect,
    options: &[(&str, SettingValue)],
    widths: &[f32],
    selected: usize,
    pal: &theme::Palette,
    hover: Hit,
    row_idx: usize,
) -> Vec<Rect> {
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
        // `w` ya lleva 9px de relleno a cada lado (ver `widths`): dibujar con `opt_r`
        // entero deja el texto pegado al borde izquierdo y todo el relleno amontonado
        // a la derecha, en vez de centrado en la pastilla.
        let label_r = Rect::new(opt_r.left + 9.0, opt_r.top, opt_r.right - 9.0, opt_r.bottom);
        r.text(label, &r.fonts().ui_12, label_r, tc);
        out.push(opt_r);
        x += w;
    }
    out
}
