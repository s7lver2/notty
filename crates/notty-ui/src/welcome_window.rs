//! Ventana de bienvenida (primer arranque): seis pasos guiados que se aplican en vivo
//! sobre la ventana principal (`docs/mockups/setup/tutorial.html` → `.wiz`). Se
//! construye igual que `settings_window.rs` (mismo `Renderer`, misma técnica de barra
//! de título propia), pero **no es modal**: se crea como ventana propia (`hWndParent`
//! = `owner`, sin `WS_EX_TOPMOST` ni deshabilitar al dueño) y no corre su propio bucle
//! de mensajes — los suyos los recoge el bucle de la ventana principal
//! (`GetMessageW(None, ...)` ya recoge mensajes de cualquier ventana de este hilo).
//!
//! Desviación del plan: la firma original pedía `show(owner, config: &mut Config,
//! on_change: impl FnMut(&Config))`, pero una ventana no modal vive más allá de la
//! llamada que la crea, así que no puede quedarse con un `&mut Config` prestado. Se
//! usa el mismo patrón que `settings_window::open` (`Rc<RefCell<Config>>` +
//! `Box<dyn Fn()>`), que ya resuelve justo este problema en esta base de código.
//!
//! Desviación del plan (Task 3 Step 2): la maqueta usa tarjetas `.choice` (punto de
//! radio + descripción) para Estilo/Tema/Teclado/Privacidad, no el control `.seg`
//! segmentado de Ajustes — así que aquí se dibujan tarjetas propias en vez de
//! reutilizar `settings_model::Row::Seg`, seleccionable con el mismo aspecto que la
//! maqueta de este plan en concreto.
//!
//! Desviación (Task 3 Step 4): no existe ningún modo de teclado "nano" en el resto de
//! la app (solo `UiConfig::vim_always: bool`); el paso Teclado solo ofrece
//! Normal/Vim, sin la tercera opción de la maqueta.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, GWLP_USERDATA, GetWindowLongPtrW, GetWindowRect, HTCAPTION,
    HTCLIENT, IDC_ARROW, KillTimer, LoadCursorW, PostMessageW, RegisterClassExW, SW_SHOW, SWP_NOZORDER, SetTimer,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_CLOSE, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_NCHITTEST, WM_PAINT, WM_TIMER, WNDCLASSEXW, WS_CLIPSIBLINGS, WS_POPUP, WS_VISIBLE,
};
use windows::core::{PCWSTR, Result, w};

use notty_config::{Config, Preset, Theme};

use crate::layout::Rect;
use crate::step_rail::{STEP_ROW_H, StepRail};
use crate::theme;
use crate::{Renderer, is_dark};

const TITLEBAR_H: f32 = 32.0; // .wt{height:32px}
const WIN_W: f32 = 430.0; // .wiz{width:430px}
const WIN_H: f32 = 380.0; // .wiz{height:380px}
const RAIL_W: f32 = 118.0; // .rail{width:118px}
const FOOTER_H: f32 = 52.0; // .wf{height:52px}
const CLOSE_W: f32 = 46.0; // .wt span:last-child{width:46px}

const STEP_LABELS: [&str; 6] = ["Hola", "Estilo", "Tema", "Teclado", "Privacidad", "Listo"];
const STEP_TRANSITION_MS: u64 = 180; // plan: "180ms in / 12px" (fundido+desplazamiento compartido)
const CHECKMARK_MS: u64 = 600;

const ID_ANIM_TIMER: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Hit {
    #[default]
    None,
    Caption,
    Close,
    Skip,
    Back,
    Next,
    Nav(usize),
    /// Tarjeta `opt` de las opciones del paso actual.
    Choice(usize),
    /// El paso Listo: la tarjeta "Enséñame dónde está todo →".
    TourGo,
}

struct State {
    cfg: Rc<RefCell<Config>>,
    on_change: Box<dyn Fn()>,
    on_start_tour: Box<dyn Fn()>,
    renderer: Renderer,
    rail: StepRail,
    hover: Hit,
    hits: Vec<(Rect, Hit)>,
    animations_enabled: bool,
    /// Paso desde el que se está saliendo (para dibujar su fundido de salida) y la
    /// animación compartida por la entrada del nuevo paso y el relleno del carril.
    prev_step: Option<usize>,
    rail_from: f32,
    rail_to: f32,
    step_anim: Option<crate::Anim>,
    checkmark_anim: Option<crate::Anim>,
    anim_timer_running: bool,
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn ensure_anim_timer(st: &mut State, hwnd: HWND) {
    if !st.anim_timer_running {
        unsafe {
            let _ = SetTimer(Some(hwnd), ID_ANIM_TIMER, 16, None);
        }
        st.anim_timer_running = true;
    }
}

/// Abre la ventana de bienvenida, flotando (no modal) sobre `owner`. `on_change` se
/// llama tras cada elección aplicada y guardada (para que `owner` repinte con su
/// animación de tema si tocaba); `on_start_tour` cuando el paso Listo pide arrancar el
/// recorrido.
pub fn show(owner: HWND, cfg: Rc<RefCell<Config>>, on_change: Box<dyn Fn()>, on_start_tour: Box<dyn Fn()>) -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("NottyWelcomeClass");
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

        let dpi = GetDpiForWindow(owner).max(96);
        let scale = dpi as f32 / 96.0;
        let (w_px, h_px) = ((WIN_W * scale) as i32, (WIN_H * scale) as i32);

        let mut owner_rect = RECT::default();
        let _ = GetWindowRect(owner, &mut owner_rect);
        let x = owner_rect.left + ((owner_rect.right - owner_rect.left) - w_px) / 2;
        let y = owner_rect.top + ((owner_rect.bottom - owner_rect.top) - h_px) / 2;

        let title = to_wide("Bienvenido a notty");
        // `WS_POPUP` propio (mismo aspecto que `settings_window`) con `owner` como
        // dueño: flota por encima de `owner` y se minimiza/cierra con él, pero
        // `owner` sigue activo y respondiendo a la vez (ventana no modal a propósito,
        // ver la nota del módulo).
        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_CLIPSIBLINGS | WS_VISIBLE,
            x,
            y,
            w_px,
            h_px,
            Some(owner),
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
            on_start_tour,
            renderer,
            rail: StepRail::new(STEP_LABELS.to_vec()),
            hover: Hit::None,
            hits: Vec::new(),
            animations_enabled,
            prev_step: None,
            rail_from: 0.0,
            rail_to: 0.0,
            step_anim: None,
            checkmark_anim: None,
            anim_timer_running: false,
        });
        let ptr = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetWindowPos(hwnd, None, x, y, w_px, h_px, SWP_NOZORDER);
        // Sin bucle de mensajes propio: no es modal, así que el bucle de la ventana
        // principal (`GetMessageW(None, ...)`) recoge también los mensajes de esta.
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
                    // Esc: cierra y marca first_run_done, igual que "Saltar".
                    if let Some(st) = ptr.as_mut() {
                        finish(st, false);
                    }
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == ID_ANIM_TIMER {
                    if let Some(st) = ptr.as_mut() {
                        let now = Instant::now();
                        if st.step_anim.is_some_and(|a| a.is_done(now)) {
                            st.step_anim = None;
                            st.prev_step = None;
                        }
                        if st.checkmark_anim.is_some_and(|a| a.is_done(now)) {
                            // Se deja en `Some` (terminada): `draw_checkmark` la sigue
                            // usando para saber que debe dibujarse llena, no a medias.
                        }
                        let still_animating = st.step_anim.is_some();
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
                // A propósito: SIN `PostQuitMessage` — esta ventana comparte el bucle
                // de mensajes de `owner`; salir de él aquí cerraría toda la app.
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

/// Marca `first_run_done` y guarda. `to_tour` arranca el recorrido justo después.
fn finish(st: &State, to_tour: bool) {
    st.cfg.borrow_mut().first_run_done = true;
    let _ = notty_config::save(&st.cfg.borrow(), &notty_config::default_path());
    (st.on_change)();
    if to_tour {
        (st.on_start_tour)();
    }
}

fn go_to(st: &mut State, hwnd: HWND, target: usize) {
    let target = target.min(STEP_LABELS.len() - 1);
    if target == st.rail.current {
        return;
    }
    st.prev_step = Some(st.rail.current);
    st.rail_from = st.rail.progress_fraction();
    st.rail.current = target;
    st.rail_to = st.rail.progress_fraction();
    st.step_anim =
        Some(crate::Anim::new_maybe(Instant::now(), Duration::from_millis(STEP_TRANSITION_MS), st.animations_enabled));
    if target == STEP_LABELS.len() - 1 {
        st.checkmark_anim =
            Some(crate::Anim::new_maybe(Instant::now(), Duration::from_millis(CHECKMARK_MS), st.animations_enabled));
    }
    ensure_anim_timer(st, hwnd);
}

fn handle_click(hwnd: HWND, st: &mut State, x: f32, y: f32) {
    let hit = hit_test(st, x, y);
    match hit {
        Hit::Close => {
            finish(st, false);
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        Hit::Skip => {
            finish(st, false);
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        Hit::Nav(i) => go_to(st, hwnd, i),
        Hit::Back => {
            if st.rail.current > 0 {
                let target = st.rail.current - 1;
                go_to(st, hwnd, target);
            }
        }
        Hit::Next => {
            if st.rail.current == STEP_LABELS.len() - 1 {
                finish(st, false);
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            } else {
                let target = st.rail.current + 1;
                go_to(st, hwnd, target);
            }
        }
        Hit::Choice(opt) => {
            apply_choice(st, opt);
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
        Hit::TourGo => {
            finish(st, true);
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        Hit::None | Hit::Caption => {}
    }
}

/// Aplica la opción `opt` (índice dentro de las tarjetas del paso activo) al `Config`
/// y guarda+notifica, para que `owner` repinte en vivo.
fn apply_choice(st: &mut State, opt: usize) {
    {
        let mut cfg = st.cfg.borrow_mut();
        match st.rail.current {
            1 => {
                let preset = if opt == 0 { Preset::Moderna } else { Preset::Clasica };
                notty_config::apply_preset(&mut cfg.ui, preset);
            }
            2 => {
                cfg.ui.theme = match opt {
                    0 => Theme::System,
                    1 => Theme::Light,
                    _ => Theme::Dark,
                };
            }
            3 => {
                cfg.ui.vim_always = opt == 1;
            }
            4 => {
                cfg.updates.check = opt == 0;
            }
            _ => return,
        }
    }
    let _ = notty_config::save(&st.cfg.borrow(), &notty_config::default_path());
    (st.on_change)();
}

/// Qué opción está seleccionada en el paso activo (para pintar el radio marcado),
/// `None` en pasos sin tarjetas (Hola, Listo).
fn selected_choice(cfg: &Config, step: usize) -> Option<usize> {
    match step {
        1 => Some(if cfg.ui.preset == Preset::Clasica { 1 } else { 0 }),
        2 => Some(match cfg.ui.theme {
            Theme::System => 0,
            Theme::Light => 1,
            Theme::Dark => 2,
        }),
        3 => Some(if cfg.ui.vim_always { 1 } else { 0 }),
        4 => Some(if cfg.updates.check { 0 } else { 1 }),
        _ => None,
    }
}

struct ChoiceOpt {
    label: &'static str,
    sub: &'static str,
    demo: &'static str,
}

fn choices_for(step: usize) -> &'static [ChoiceOpt] {
    const ESTILO: &[ChoiceOpt] = &[
        ChoiceOpt { label: "Moderna", sub: "Pestañas en la barra de título, sin menús", demo: "" },
        ChoiceOpt { label: "Clásica", sub: "Menú Archivo · Editar · Ver, como siempre", demo: "" },
    ];
    const TEMA: &[ChoiceOpt] = &[
        ChoiceOpt { label: "Usar el de Windows", sub: "", demo: "" },
        ChoiceOpt { label: "Claro", sub: "", demo: "" },
        ChoiceOpt { label: "Oscuro", sub: "", demo: "" },
    ];
    const TECLADO: &[ChoiceOpt] = &[
        ChoiceOpt { label: "Normal", sub: "Como cualquier editor de Windows", demo: "" },
        ChoiceOpt { label: "Vim", sub: "Modos Normal / Insertar / Visual", demo: "hjkl · dd · :w" },
    ];
    const PRIVACIDAD: &[ChoiceOpt] = &[
        ChoiceOpt { label: "Avisarme de nuevas versiones", sub: "", demo: "" },
        ChoiceOpt { label: "No, lo miraré yo", sub: "Ajustes → Acerca de → Buscar ahora", demo: "" },
    ];
    match step {
        1 => ESTILO,
        2 => TEMA,
        3 => TECLADO,
        4 => PRIVACIDAD,
        _ => &[],
    }
}

// --- Dibujo -------------------------------------------------------------------------

fn paint(st: &mut State) {
    st.hits.clear();
    let dark = is_dark(st.cfg.borrow().ui.theme, crate::window::system_uses_dark_mode());
    let pal = theme::palette(dark);
    let (w, h) = st.renderer.size_dips();
    let r = &st.renderer;

    r.begin_paint(pal.chrome);

    // Barra de título: "Bienvenido a notty" + ✕.
    let titlebar = Rect::new(0.0, 0.0, w, TITLEBAR_H);
    r.text("Bienvenido a notty", &r.fonts().ui_11_5, Rect::new(12.0, 0.0, w - CLOSE_W, TITLEBAR_H), pal.text_2);
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

    // Carril de pasos (izquierda).
    let rail_area = Rect::new(0.0, TITLEBAR_H, RAIL_W, h - FOOTER_H);
    let rail_target = Rect::new(rail_area.left + 14.0, rail_area.top + 10.0, rail_area.right, rail_area.bottom);
    let now = Instant::now();
    let rail_t = st.step_anim.map(|a| a.value(now, st.rail_from, st.rail_to)).unwrap_or(st.rail.progress_fraction());
    crate::step_rail::draw(r, pal, rail_target, &st.rail, rail_t);
    for i in 0..STEP_LABELS.len() {
        let row_top = rail_target.top + STEP_ROW_H * i as f32;
        st.hits.push((Rect::new(rail_area.left, row_top, rail_area.right, row_top + STEP_ROW_H), Hit::Nav(i)));
    }

    // Panel del paso activo, con el fundido+desplazamiento de entrada/salida.
    let pane = Rect::new(RAIL_W, TITLEBAR_H, w, h - FOOTER_H);
    let t = st.step_anim.map(|a| a.progress(now)).unwrap_or(1.0);
    let eased = crate::ease_out_cubic(t);
    if let Some(prev) = st.prev_step {
        let dx = -12.0 * t;
        let fade = 1.0 - t;
        draw_step_content(r, pal, offset(pane, dx), prev, fade, None);
    }
    let dx_in = (1.0 - eased) * 14.0;
    let checkmark = st.checkmark_anim;
    draw_step_content(r, pal, offset(pane, dx_in), st.rail.current, eased, checkmark);
    if st.rail.current != 0 && st.rail.current != STEP_LABELS.len() - 1 {
        let selected = selected_choice(&st.cfg.borrow(), st.rail.current);
        let opts = choices_for(st.rail.current);
        let rows = draw_choices(r, pal, offset(pane, dx_in), opts, selected, st.hover, eased);
        for (i, rr) in rows.into_iter().enumerate() {
            st.hits.push((rr, Hit::Choice(i)));
        }
    }
    if st.rail.current == STEP_LABELS.len() - 1 {
        let tourgo_r = draw_tour_go(r, pal, offset(pane, dx_in), st.hover, eased);
        st.hits.push((tourgo_r, Hit::TourGo));
    }

    // Pie: "Saltar" + Atrás/Siguiente.
    let footer = Rect::new(0.0, h - FOOTER_H, w, h);
    r.fill(footer, pal.chrome);
    r.stroke_line(footer.left, footer.top, footer.right, footer.top, 1.0, pal.line);
    let skip_r = Rect::new(16.0, footer.top, 90.0, footer.bottom);
    r.text("Saltar", &r.fonts().ui_12, skip_r, if st.hover == Hit::Skip { pal.text } else { pal.text_2 });
    st.hits.push((skip_r, Hit::Skip));

    let next_label = if st.rail.current == 0 {
        "Empezar"
    } else if st.rail.current == STEP_LABELS.len() - 1 {
        "Terminar"
    } else {
        "Siguiente"
    };
    let next_w = r.measure(next_label, &r.fonts().ui_12) + 32.0;
    let next_r = Rect::new(w - 16.0 - next_w, footer.top + 10.0, w - 16.0, footer.bottom - 10.0);
    r.fill_round(next_r, 4.0, pal.accent);
    r.text(next_label, &r.fonts().ui_12, next_r, pal.on_accent);
    st.hits.push((next_r, Hit::Next));

    if st.rail.current > 0 {
        let back_w = r.measure("Atrás", &r.fonts().ui_12) + 28.0;
        let back_r = Rect::new(next_r.left - 6.0 - back_w, footer.top + 10.0, next_r.left - 6.0, footer.bottom - 10.0);
        r.fill_round(back_r, 4.0, pal.hover);
        r.text("Atrás", &r.fonts().ui_12, back_r, pal.text);
        st.hits.push((back_r, Hit::Back));
    }

    r.end_paint();
}

fn offset(r: Rect, dx: f32) -> Rect {
    Rect::new(r.left + dx, r.top, r.right + dx, r.bottom)
}

/// Título + subtítulo de cada paso (`h4`/`.sub` de la maqueta), más el caso especial
/// del paso Listo (marca de verificación animada + el aviso de "puedes repetirlo").
fn draw_step_content(r: &Renderer, pal: &theme::Palette, pane: Rect, step: usize, fade: f32, checkmark: Option<crate::Anim>) {
    let (title, sub): (&str, &str) = match step {
        0 => ("Hola 👋", "Cuatro preguntas rápidas y listo. Todo se aplica al momento en la ventana de detrás, y puedes cambiarlo luego en Ajustes."),
        1 => ("Estilo", "¿Cómo quieres la ventana?"),
        2 => ("Tema", "Claro, oscuro o el que use Windows."),
        3 => ("Teclado", "Si no sabes qué es vim, deja \"Normal\"."),
        4 => ("Actualizaciones", "notty no se conecta a nada sin tu permiso. Si lo activas, solo consulta una vez al día si hay versión nueva en GitHub: sin datos, sin identificadores."),
        _ => ("Todo listo", "¿Te enseño en 20 segundos dónde está cada cosa?"),
    };
    let title_r = Rect::new(pane.left + 12.0, pane.top, pane.right - 12.0, pane.top + 24.0);
    r.text(title, &r.fonts().ui_20_semibold, title_r, pal.text.faded(fade));
    let sub_r = Rect::new(pane.left + 12.0, title_r.bottom + 3.0, pane.right - 12.0, title_r.bottom + 3.0 + 48.0);
    r.text(sub, &r.fonts().ui_11_5, sub_r, pal.text_2.faded(fade));

    if step == STEP_LABELS.len() - 1 {
        draw_checkmark(r, pal, Rect::new(pane.left + 12.0, sub_r.bottom + 8.0, pane.left + 12.0 + 56.0, sub_r.bottom + 8.0 + 56.0), checkmark);
    }
}

/// Marca de verificación trazada con dos segmentos, revelados progresivamente en
/// 600ms (mismo tratamiento animado que el paso Listo del instalador: un trazo que se
/// dibuja, no un icono que aparece de golpe). No hay ningún helper de geometría D2D
/// compartido en este árbol de trabajo (el del instalador vive en otro, invisible
/// aquí), así que se traza a mano con dos `stroke_line` parciales.
fn draw_checkmark(r: &Renderer, pal: &theme::Palette, box_r: Rect, anim: Option<crate::Anim>) {
    let t = anim.map(|a| a.value(Instant::now(), 0.0, 1.0)).unwrap_or(1.0);
    r.stroke_circle(
        (box_r.left + box_r.right) / 2.0,
        (box_r.top + box_r.bottom) / 2.0,
        box_r.width() / 2.0,
        2.0,
        pal.accent.faded(0.35 + 0.65 * t),
    );
    // Puntos del trazo (proporción de la caja, como el `path` del check de la maqueta):
    // (0.22,0.52) -> (0.42,0.72) -> (0.80,0.32), en dos tramos.
    let p0 = (box_r.left + box_r.width() * 0.22, box_r.top + box_r.height() * 0.52);
    let p1 = (box_r.left + box_r.width() * 0.42, box_r.top + box_r.height() * 0.72);
    let p2 = (box_r.left + box_r.width() * 0.80, box_r.top + box_r.height() * 0.32);
    let seg1_len = 0.4; // fracción de `t` dedicada al primer tramo
    let t1 = (t / seg1_len).clamp(0.0, 1.0);
    let t2 = ((t - seg1_len) / (1.0 - seg1_len)).clamp(0.0, 1.0);
    if t1 > 0.0 {
        let ex = p0.0 + (p1.0 - p0.0) * t1;
        let ey = p0.1 + (p1.1 - p0.1) * t1;
        r.stroke_line(p0.0, p0.1, ex, ey, 2.5, pal.accent);
    }
    if t2 > 0.0 {
        let ex = p1.0 + (p2.0 - p1.0) * t2;
        let ey = p1.1 + (p2.1 - p1.1) * t2;
        r.stroke_line(p1.0, p1.1, ex, ey, 2.5, pal.accent);
    }
}

/// Tarjetas `.choice` (punto de radio + etiqueta + descripción opcional). Devuelve el
/// rectángulo de cada una para el hit-testing.
fn draw_choices(
    r: &Renderer,
    pal: &theme::Palette,
    pane: Rect,
    opts: &[ChoiceOpt],
    selected: Option<usize>,
    hover: Hit,
    fade: f32,
) -> Vec<Rect> {
    let mut out = Vec::with_capacity(opts.len());
    let mut y = pane.top + 56.0;
    for (i, opt) in opts.iter().enumerate() {
        let has_sub = !opt.sub.is_empty();
        let row_h = if has_sub { 44.0 } else { 34.0 };
        let rr = Rect::new(pane.left + 12.0, y, pane.right - 12.0, y + row_h);
        let on = selected == Some(i);
        let bg = if on { pal.accent_soft } else if hover == Hit::Choice(i) { pal.hover } else { pal.surface_2 };
        r.fill_round(rr, 6.0, bg.faded(fade));
        if on {
            r.stroke_round_rect(rr, 6.0, 1.0, pal.accent.faded(fade));
        }
        let rad_cx = rr.left + 12.0 + 7.0;
        let rad_cy = rr.top + rr.height() / 2.0;
        r.stroke_circle(rad_cx, rad_cy, 7.0, 1.5, (if on { pal.accent } else { pal.text_2 }).faded(fade));
        if on {
            r.fill_circle(rad_cx, rad_cy, 3.5, pal.accent.faded(fade));
        }
        let text_left = rad_cx + 7.0 + 10.0;
        let label_top = if has_sub { rr.top + 8.0 } else { rr.top };
        r.text(opt.label, &r.fonts().ui_13, Rect::new(text_left, label_top, rr.right - 12.0, label_top + 18.0), pal.text.faded(fade));
        if has_sub {
            r.text(opt.sub, &r.fonts().ui_11_5, Rect::new(text_left, label_top + 17.0, rr.right - 12.0, label_top + 33.0), pal.text_3.faded(fade));
        }
        if !opt.demo.is_empty() {
            let dw = r.measure(opt.demo, &r.fonts().mono_11) + 10.0;
            let dr = Rect::new(rr.right - 12.0 - dw, rr.top + 6.0, rr.right - 12.0, rr.top + 6.0 + 18.0);
            r.fill_round(dr, 3.0, pal.hover.faded(fade));
            r.text(opt.demo, &r.fonts().mono_11, Rect::new(dr.left + 5.0, dr.top, dr.right - 5.0, dr.bottom), pal.text_2.faded(fade));
        }
        out.push(rr);
        y += row_h + 6.0;
    }
    out
}

/// Paso Listo: la tarjeta destacada "Enséñame dónde está todo →" (`#tourGo` en la
/// maqueta) más la nota de que se puede repetir desde Ajustes → Ayuda.
fn draw_tour_go(r: &Renderer, pal: &theme::Palette, pane: Rect, hover: Hit, fade: f32) -> Rect {
    let y = pane.top + 8.0 + 56.0 + 16.0;
    let rr = Rect::new(pane.left + 12.0, y, pane.right - 12.0, y + 34.0);
    let bg = if hover == Hit::TourGo { pal.accent_soft } else { pal.surface_2 };
    r.fill_round(rr, 6.0, bg.faded(fade));
    r.stroke_round_rect(rr, 6.0, 1.0, pal.accent.faded(fade));
    r.text(
        "Enséñame dónde está todo →",
        &r.fonts().ui_13,
        Rect::new(rr.left + 12.0, rr.top, rr.right - 12.0, rr.bottom),
        pal.accent.faded(fade),
    );
    let note_r = Rect::new(pane.left + 12.0, rr.bottom + 10.0, pane.right - 12.0, rr.bottom + 10.0 + 32.0);
    r.text("Puedes repetirlo desde Ajustes → Ayuda.", &r.fonts().ui_11_5, note_r, pal.text_3.faded(fade));
    rr
}

