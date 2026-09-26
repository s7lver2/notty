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
//!
//! Corrección de QA visual (ver memoria del proyecto, "Pending visual QA:
//! installer/tutorial"): las transiciones de paso ahora siguen el mismo modelo de dos
//! fases que `notty-setup::ui` (salida 140ms + entrada 180ms/12px con `ease_out_cubic`,
//! ver `STEP_OUT_MS`/`STEP_IN_MS` y `Transition`) en vez de un único `Anim` compartido
//! entre saliente y entrante, y la ventana tiene su propia animación de entrada
//! (`entrance`) para el primer pintado. El paso Estilo gana una vista previa en vivo
//! con fundido cruzado + escala (`draw_style_preview`, mismos 250ms/300ms que
//! `notty-setup::ui::{SCENE_FADE,SCENE_SCALE}`). El paso Teclado anima su etiqueta
//! `.demo` de Vim con el mismo revelado `steps(14)/1.4s` que las escenas del
//! instalador. El "✓" final reutiza la geometría de `notty-setup::draw_check_stroke`
//! (más limpia que el trazo de dos segmentos ad-hoc que había antes) y arranca 200ms
//! después de que termine la transición de entrada, no en paralelo con ella.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{InvalidateRect, MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, GWLP_USERDATA, GetWindowLongPtrW, GetWindowRect, HTCAPTION,
    HTCLIENT, IDC_ARROW, KillTimer, LoadCursorW, PostMessageW, RegisterClassExW, SW_SHOW, SWP_FRAMECHANGED,
    SWP_NOZORDER, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_CLOSE, WM_DESTROY, WM_DPICHANGED,
    WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCALCSIZE, WM_NCHITTEST, WM_PAINT,
    WM_SIZE, WM_TIMER, WNDCLASSEXW, WS_CAPTION, WS_CLIPSIBLINGS, WS_MAXIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
    WS_THICKFRAME,
};
use windows::core::{PCWSTR, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

use notty_config::{Config, Preset, Theme};

use crate::layout::Rect;
use crate::step_rail::{STEP_ROW_H, StepRail};
use crate::theme;
use crate::{Renderer, Rgba, is_dark};

use adaptive::text_fit;

const TITLEBAR_H: f32 = 32.0; // .wt{height:32px}
// Más ancha que la maqueta (430x380) para que quepan las tres cajas de Estilo.
const WIN_W: f32 = 540.0;
const WIN_H: f32 = 400.0;
/// Tamaño mínimo al redimensionar (sin pasar nunca del área de trabajo del monitor).
const MIN_W: f32 = 380.0;
const MIN_H: f32 = 350.0;
const STYLE_PRESETS: [Preset; 3] = [Preset::Moderna, Preset::Clasica, Preset::Zen];
const RAIL_W: f32 = 118.0; // .rail{width:118px}
/// Por debajo de este ancho de ventana el carril solo enseña los puntos.
const RAIL_COMPACT_BELOW: f32 = 470.0;
const RAIL_W_COMPACT: f32 = 36.0;
/// Ancho máximo del panel de contenido: con la ventana maximizada las tarjetas no se
/// estiran de lado a lado de la pantalla.
const PANE_MAX_W: f32 = 660.0;
/// Ancho mínimo de cada caja de Estilo en tres columnas; si no caben, se apilan.
const STYLE_CARD_MIN_W: f32 = 112.0;
const FOOTER_H: f32 = 52.0; // .wf{height:52px}
const CLOSE_W: f32 = 46.0; // .wt span:last-child{width:46px}

const STEP_LABELS: [&str; 6] = ["Hola", "Estilo", "Tema", "Teclado", "Privacidad", "Listo"];
// Convención del proyecto (mismos valores que `notty-setup::ui`): salida 140ms
// (fundido) y entrada 180ms (fundido+desplazamiento de 12px, `ease_out_cubic`).
const STEP_OUT_MS: u64 = 140;
const STEP_IN_MS: u64 = 180;
const CHECKMARK_DELAY_MS: u64 = 200;
const CHECKMARK_MS: u64 = 600;
/// Duración del "pop" del punto de radio recién marcado (`.rad::after{animation:rp
/// .2s}` de la maqueta).
const CHOICE_POP_MS: u64 = 200;

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

/// Paso desde el que se sale y cuándo empezó la transición (salida 140ms + entrada
/// 180ms, ver módulo). Igual que `notty_setup::ui::Transition`.
#[derive(Debug, Clone, Copy)]
struct Transition {
    from: usize,
    start: Instant,
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
    /// Cuándo se creó la ventana: da la animación de entrada del primer paso (igual
    /// que `notty_setup::ui::State::entrance`), para que "Hola" también entre con
    /// fundido+desplazamiento en vez de aparecer de golpe.
    entrance: Instant,
    /// Transición de paso en curso (fase de salida del `from`, luego entrada del
    /// paso ya activo en `rail.current`), o `None` en reposo.
    transition: Option<Transition>,
    rail_from: f32,
    rail_to: f32,
    checkmark_anim: Option<crate::Anim>,
    /// Cuánto estaba encendida cada caja de Estilo (0..1) al empezar la última
    /// animación, y cuándo empezó: el valor mostrado va de ahí a su destino.
    card_from: [f32; 3],
    style_at: Instant,
    /// Cuándo se marcó la tarjeta actualmente seleccionada del paso activo: anima el
    /// punto de radio (`CHOICE_POP_MS`) y sirve de ancla para la demo de Vim.
    choice_at: Option<Instant>,
    /// Cambio de tema en curso: de qué tema se viene (`true` = oscuro) y cuándo.
    theme_from: Option<(bool, Instant)>,
    anim_timer_running: bool,
}

const THEME_MS: u64 = 350; // .app{transition:background .35s,color .35s}
const STYLE_CARD_MS: u64 = 280;
/// El 👋 saluda al entrar y luego cada `WAVE_EVERY` segundos mientras sigas en "Hola".
const WAVE_S: f32 = 1.1;
const WAVE_EVERY: f32 = 3.2;

fn wave_angle(elapsed: f32) -> f32 {
    let c = elapsed % WAVE_EVERY;
    if c >= WAVE_S {
        return 0.0;
    }
    let p = c / WAVE_S;
    20.0 * (p * std::f32::consts::TAU * 2.0).sin() * (1.0 - p)
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

/// Progreso `0.0..=1.0` de una animación de reloj (a diferencia de `crate::Anim`, que
/// vive en la propia animación): usado para las transiciones que solo necesitan un
/// `Instant` de inicio suelto (rail, escenas, pop). Igual que `notty_setup::ui::eased`.
fn eased(now: Instant, start: Instant, dur: Duration, enabled: bool) -> f32 {
    if !enabled || dur.is_zero() {
        return 1.0;
    }
    let p = (now.saturating_duration_since(start).as_secs_f32() / dur.as_secs_f32()).clamp(0.0, 1.0);
    crate::ease_out_cubic(p)
}

fn anim_done(now: Instant, start: Instant, dur: Duration) -> bool {
    now.saturating_duration_since(start) >= dur
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

        // Centrada sobre `owner`, pero recortada al área de trabajo de su monitor y
        // metida dentro entera (pantallas pequeñas o escalados altos).
        let monitor = MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST);
        let dpi = adaptive::monitor_dpi(monitor);
        let mut owner_rect = RECT::default();
        let _ = GetWindowRect(owner, &mut owner_rect);
        let center = ((owner_rect.left + owner_rect.right) / 2, (owner_rect.top + owner_rect.bottom) / 2);
        let (x, y, w_px, h_px) = adaptive::initial_rect(WIN_W, WIN_H, dpi, adaptive::work_area(monitor), center);

        let title = to_wide("Bienvenido a notty");
        // Barra de título nativa (que `WM_NCCALCSIZE` quita) en vez de `WS_POPUP`: así
        // Windows le da su animación de apertura. `owner` como dueño: flota encima y se
        // cierra con él, pero sigue activo (no modal, ver la nota del módulo).
        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MAXIMIZEBOX | WS_CLIPSIBLINGS,
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
        let now = Instant::now();
        let state = Box::new(State {
            cfg,
            on_change,
            on_start_tour,
            renderer,
            rail: StepRail::new(STEP_LABELS.to_vec()),
            hover: Hit::None,
            hits: Vec::new(),
            animations_enabled,
            entrance: now,
            transition: None,
            rail_from: 0.0,
            rail_to: 0.0,
            checkmark_anim: None,
            card_from: [0.0; 3],
            style_at: now - Duration::from_secs(1),
            choice_at: None,
            theme_from: None,
            anim_timer_running: false,
        });
        let ptr = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
        // Arranca el temporizador ya aquí para que el primer paso ("Hola") también
        // entre animado en vez de aparecer de golpe (antes solo `go_to` lo arrancaba).
        if let Some(st) = ptr.as_mut() {
            ensure_anim_timer(st, hwnd);
        }

        let _ = SetWindowPos(hwnd, None, x, y, w_px, h_px, SWP_NOZORDER | SWP_FRAMECHANGED);
        let _ = ShowWindow(hwnd, SW_SHOW);
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
            WM_NCCALCSIZE if wparam.0 != 0 => adaptive::nc_calc_size(hwnd, lparam),
            WM_GETMINMAXINFO => {
                adaptive::min_max_info(hwnd, lparam, MIN_W, MIN_H);
                LRESULT(0)
            }
            WM_DPICHANGED => {
                // El DPI va antes que el tamaño: el `WM_SIZE` que dispara `SetWindowPos`
                // ya redimensiona el render target con la escala nueva.
                if let Some(st) = ptr.as_mut() {
                    st.renderer.set_dpi(adaptive::dpi_from_wparam(wparam));
                }
                adaptive::apply_suggested_rect(hwnd, lparam);
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_SIZE => {
                // El renderer se crea antes de quitar la barra nativa (área cliente más
                // baja): sin esto D2D estira el dibujo y los clics caen desplazados.
                if let Some(st) = ptr.as_mut() {
                    st.renderer.resize((lparam.0 as u32) & 0xFFFF, ((lparam.0 as u32) >> 16) & 0xFFFF);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let def = DefWindowProcW(hwnd, msg, wparam, lparam);
                if def.0 as u32 != HTCLIENT {
                    return def;
                }
                if let Some(edge) = adaptive::edge_hit(hwnd, lparam) {
                    return edge;
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
                        let total_step = Duration::from_millis(STEP_OUT_MS + STEP_IN_MS);
                        if let Some(t) = st.transition {
                            if !st.animations_enabled || anim_done(now, t.start, total_step) {
                                st.transition = None;
                            }
                        }
                        if st.checkmark_anim.is_some_and(|a| a.is_done(now)) {
                            // Se deja en `Some` (terminada): `draw_checkmark` la sigue
                            // usando para saber que debe dibujarse llena, no a medias.
                        }
                        // La demo de Vim (paso Teclado) teclea en bucle mientras está
                        // seleccionada y visible: sin esto el temporizador se pararía a
                        // mitad de la animación en cuanto el resto termine.
                        let vim_looping = st.animations_enabled
                            && st.rail.current == 3
                            && selected_choice(&st.cfg.borrow(), 3) == Some(1);
                        let waving = st.animations_enabled && st.rail.current == 0;
                        let entering = !anim_done(now, st.entrance, Duration::from_millis(STEP_IN_MS + 40));
                        if st.theme_from.is_some_and(|(_, t0)| anim_done(now, t0, Duration::from_millis(THEME_MS))) {
                            st.theme_from = None;
                        }
                        let still_animating = entering
                            || waving
                            || st.theme_from.is_some()
                            || st.transition.is_some()
                            || st.checkmark_anim.is_some_and(|a| !a.is_done(now))
                            || !anim_done(now, st.style_at, Duration::from_millis(STYLE_CARD_MS))
                            || st.choice_at.is_some_and(|t0| !anim_done(now, t0, Duration::from_millis(CHOICE_POP_MS)))
                            || vim_looping;
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
    let now = Instant::now();
    st.rail_from = st.rail.progress_fraction();
    st.transition = Some(Transition { from: st.rail.current, start: now });
    st.rail.current = target;
    st.rail_to = st.rail.progress_fraction();
    st.choice_at = None;
    if target == STEP_LABELS.len() - 1 {
        // Convención: el "✓" espera a que termine de entrar el paso (140+180ms) y
        // luego un respiro de 200ms antes de trazarse en 600ms (mismo tratamiento que
        // `notty-setup::State::go_to`), en vez de arrancar en paralelo con la entrada.
        let start = now + Duration::from_millis(STEP_OUT_MS + STEP_IN_MS + CHECKMARK_DELAY_MS);
        st.checkmark_anim = Some(crate::Anim::new_maybe(start, Duration::from_millis(CHECKMARK_MS), st.animations_enabled));
    } else {
        st.checkmark_anim = None;
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
            apply_choice(st, hwnd, opt);
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
/// y guarda+notifica, para que `owner` repinte en vivo. También arranca las
/// animaciones de la tarjeta recién marcada (pop del punto, y el fundido cruzado de la
/// vista previa de Estilo si tocaba).
fn apply_choice(st: &mut State, hwnd: HWND, opt: usize) {
    let now = Instant::now();
    {
        let mut cfg = st.cfg.borrow_mut();
        match st.rail.current {
            1 => {
                let preset = STYLE_PRESETS[opt.min(STYLE_PRESETS.len() - 1)];
                if cfg.ui.preset != preset {
                    // Cada caja parte de lo que enseña ahora mismo, aunque la animación
                    // anterior no hubiera terminado: así cambiar rápido no da saltos.
                    let t = eased(now, st.style_at, Duration::from_millis(STYLE_CARD_MS), st.animations_enabled);
                    let old = STYLE_PRESETS.iter().position(|p| *p == cfg.ui.preset);
                    for (i, from) in st.card_from.iter_mut().enumerate() {
                        let target = if old == Some(i) { 1.0 } else { 0.0 };
                        *from += (target - *from) * t;
                    }
                    st.style_at = now;
                }
                notty_config::apply_preset(&mut cfg.ui, preset);
            }
            2 => {
                let system_dark = crate::window::system_uses_dark_mode();
                let was_dark = is_dark(cfg.ui.theme, system_dark);
                cfg.ui.theme = match opt {
                    0 => Theme::System,
                    1 => Theme::Light,
                    _ => Theme::Dark,
                };
                if is_dark(cfg.ui.theme, system_dark) != was_dark {
                    st.theme_from = Some((was_dark, now));
                }
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
    st.choice_at = Some(now);
    let _ = notty_config::save(&st.cfg.borrow(), &notty_config::default_path());
    (st.on_change)();
    ensure_anim_timer(st, hwnd);
}

/// Qué opción está seleccionada en el paso activo (para pintar el radio marcado),
/// `None` en pasos sin tarjetas (Hola, Listo).
fn selected_choice(cfg: &Config, step: usize) -> Option<usize> {
    match step {
        1 => STYLE_PRESETS.iter().position(|p| *p == cfg.ui.preset),
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
    let now = Instant::now();
    let anim = st.animations_enabled;
    // Fundido de tema: la paleta entera se interpola, así todo (fondo, texto,
    // tarjetas) cambia a la vez y suave, como `.app{transition:background .35s}`.
    let theme_t = st.theme_from.map(|(_, t0)| eased(now, t0, Duration::from_millis(THEME_MS), anim));
    let dark_k = match (st.theme_from, theme_t) {
        (Some((from_dark, _)), Some(t)) => {
            let (a, b) = (if from_dark { 1.0 } else { 0.0 }, if dark { 1.0 } else { 0.0 });
            a + (b - a) * t
        }
        _ => if dark { 1.0 } else { 0.0 },
    };
    let pal_mixed = theme::LIGHT.mix(&theme::DARK, dark_k);
    let pal = &pal_mixed;
    let (w, h) = st.renderer.size_dips();
    let r = &st.renderer;
    let compact_rail = w < RAIL_COMPACT_BELOW;
    let rail_w = if compact_rail { RAIL_W_COMPACT } else { RAIL_W };

    r.begin_paint(pal.chrome);

    // Barra de título: "Bienvenido a notty" + ✕.
    let titlebar = Rect::new(0.0, 0.0, w, TITLEBAR_H);
    text_fit(r, "Bienvenido a notty", &r.fonts().ui_11_5, Rect::new(12.0, 0.0, w - CLOSE_W - 6.0, TITLEBAR_H), pal.text_2);
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

    // Carril de pasos (izquierda): se rellena durante la misma ventana de tiempo que
    // el contenido (salida+entrada), no en 180ms sueltos.
    let rail_area = Rect::new(0.0, TITLEBAR_H, rail_w, h - FOOTER_H);
    // Carril compacto: el borde derecho pegado al punto deja las etiquetas sin ancho y
    // `step_rail::draw` no las dibuja.
    let rail_right = if compact_rail { rail_area.left + 14.0 + 8.0 } else { rail_area.right };
    let rail_target = Rect::new(rail_area.left + 14.0, rail_area.top + 10.0, rail_right, rail_area.bottom);
    let total_step = Duration::from_millis(STEP_OUT_MS + STEP_IN_MS);
    let rail_t = match st.transition {
        Some(t) => st.rail_from + (st.rail_to - st.rail_from) * eased(now, t.start, total_step, anim),
        None => st.rail.progress_fraction(),
    };
    crate::step_rail::draw(r, pal, rail_target, &st.rail, rail_t);
    for i in 0..STEP_LABELS.len() {
        let row_top = rail_target.top + STEP_ROW_H * i as f32;
        st.hits.push((Rect::new(rail_area.left, row_top, rail_area.right, row_top + STEP_ROW_H), Hit::Nav(i)));
    }

    // Panel del paso activo, con el fundido+desplazamiento de entrada/salida en dos
    // fases (ver el módulo): primero se dibuja el paso saliente si toca (140ms), y
    // luego siempre el paso activo entrando (180ms), con la animación de entrada de
    // la ventana (`st.entrance`) como línea de base cuando no hay transición en curso
    // (igual que `notty_setup::ui::draw`).
    let pane = Rect::new(rail_w, TITLEBAR_H, w.min(rail_w + PANE_MAX_W), h - FOOTER_H);
    // Nada del paso se sale por encima del pie aunque la ventana sea muy baja (margen
    // a la izquierda para el halo de las cajas y el desplazamiento de salida).
    r.push_clip(Rect::new(pane.left - 8.0, pane.top, w, pane.bottom));
    let out_d = Duration::from_millis(STEP_OUT_MS);
    let in_d = Duration::from_millis(STEP_IN_MS);
    let in_start = match st.transition {
        Some(t) if anim && now < t.start + out_d => {
            let lin = (now.saturating_duration_since(t.start).as_secs_f32() / out_d.as_secs_f32()).clamp(0.0, 1.0);
            let dx = -12.0 * lin;
            let fade = 1.0 - lin;
            draw_step_content(r, pal, offset(pane, dx), t.from, fade, 0.0, None, now);
            None
        }
        Some(t) => Some(t.start + out_d),
        None => Some(st.entrance),
    };
    if let Some(start) = in_start {
        let v = eased(now, start, in_d, anim);
        let dx_in = 12.0 * (1.0 - v);
        let checkmark = st.checkmark_anim;
        let since_in = now.saturating_duration_since(start).as_secs_f32() - 0.2;
        let wave = if anim && st.rail.current == 0 && since_in > 0.0 { wave_angle(since_in) } else { 0.0 };
        let content_top = draw_step_content(r, pal, offset(pane, dx_in), st.rail.current, v, wave, checkmark, now);
        if st.rail.current != 0 && st.rail.current != STEP_LABELS.len() - 1 {
            let selected = selected_choice(&st.cfg.borrow(), st.rail.current);
            let opts = choices_for(st.rail.current);
            let pop_t = st
                .choice_at
                .map(|t0| eased(now, t0, Duration::from_millis(CHOICE_POP_MS), anim))
                .unwrap_or(1.0);
            let demo_elapsed = (st.rail.current == 3 && anim).then(|| {
                let anchor = st.choice_at.unwrap_or(st.entrance);
                now.saturating_duration_since(anchor).as_secs_f32()
            });
            let rows = if st.rail.current == 1 {
                draw_style_cards(r, pal, offset(pane, dx_in), content_top, selected, &st.card_from, st.style_at, st.hover, v, now, anim)
            } else {
                draw_choices(r, pal, offset(pane, dx_in), content_top, opts, selected, st.hover, v, pop_t, demo_elapsed)
            };
            for (i, rr) in rows.into_iter().enumerate() {
                st.hits.push((rr, Hit::Choice(i)));
            }
        }
        if st.rail.current == STEP_LABELS.len() - 1 {
            let tourgo_r = draw_tour_go(r, pal, offset(pane, dx_in), content_top, st.hover, v);
            st.hits.push((tourgo_r, Hit::TourGo));
        }
    }
    r.pop_clip();

    // Pie (`.wf{height:52px;padding:0 16px;background:#1b1c1e;border-top:1px solid #2c2d31}`):
    // "Saltar" a la izquierda, Atrás (`.btn2`) + primario (`.btn`) a la derecha.
    let footer = Rect::new(0.0, h - FOOTER_H, w, h);
    let foot_bg = pal.chrome.mix(Rgba(0x1b as f32 / 255.0, 0x1c as f32 / 255.0, 0x1e as f32 / 255.0, 1.0), dark_k);
    r.fill(footer, foot_bg);
    r.stroke_line(footer.left, footer.top + 0.5, footer.right, footer.top + 0.5, 1.0, pal.line);
    let skip_w = r.measure("Saltar", &r.fonts().ui_12);
    let skip_r = Rect::new(16.0, footer.top, 16.0 + skip_w, footer.bottom);
    r.text("Saltar", &r.fonts().ui_12, skip_r, if st.hover == Hit::Skip { pal.text } else { pal.text_3 });
    st.hits.push((skip_r, Hit::Skip));

    let next_label = if st.rail.current == 0 {
        "Empezar"
    } else if st.rail.current == STEP_LABELS.len() - 1 {
        "Terminar"
    } else {
        "Siguiente"
    };
    let cy = footer.top + footer.height() / 2.0;
    let bold = &r.fonts().ui_12_semibold;
    let next_w = 1.0 + 16.0 + r.measure(next_label, bold) + 16.0 + 1.0;
    let next_r = Rect::new(w - 16.0 - next_w, cy - 15.0, w - 16.0, cy + 15.0);
    let accent_hi = Rgba(0x8c as f32 / 255.0, 0xc4 as f32 / 255.0, 0xfb as f32 / 255.0, 1.0);
    r.fill_round(next_r, 4.0, if st.hover == Hit::Next { accent_hi } else { pal.accent });
    r.stroke_round_rect(next_r, 4.0, 1.0, accent_hi);
    r.text_center(next_label, bold, next_r, pal.on_accent);
    st.hits.push((next_r, Hit::Next));

    if st.rail.current > 0 {
        let back_w = 1.0 + 14.0 + r.measure("Atrás", &r.fonts().ui_12) + 14.0 + 1.0;
        let back_r = Rect::new(next_r.left - 6.0 - back_w, cy - 15.0, next_r.left - 6.0, cy + 15.0);
        r.fill_round(back_r, 4.0, if st.hover == Hit::Back { pal.hover } else { pal.surface_2 });
        r.stroke_round_rect(back_r, 4.0, 1.0, pal.line);
        r.text_center("Atrás", &r.fonts().ui_12, back_r, pal.text);
        st.hits.push((back_r, Hit::Back));
    }

    r.end_paint();
}

fn offset(r: Rect, dx: f32) -> Rect {
    Rect::new(r.left + dx, r.top, r.right + dx, r.bottom)
}

/// Título + subtítulo de cada paso (`h4`/`.sub` de la maqueta), más el caso especial
/// del paso Listo (marca de verificación animada + el aviso de "puedes repetirlo").
/// Devuelve dónde acaba el subtítulo (`.sub{margin-bottom:14px}` incluido), para que
/// las tarjetas del paso empiecen justo debajo aunque el texto ocupe varias líneas.
fn draw_step_content(
    r: &Renderer,
    pal: &theme::Palette,
    pane: Rect,
    step: usize,
    fade: f32,
    wave: f32,
    checkmark: Option<crate::Anim>,
    now: Instant,
) -> f32 {
    let (title, sub): (&str, &str) = match step {
        0 => ("Hola","Cuatro preguntas rápidas y listo. Todo se aplica al momento en la ventana de detrás, y puedes cambiarlo luego en Ajustes."),
        1 => ("Estilo", "¿Cómo quieres la ventana?"),
        2 => ("Tema", "Claro, oscuro o el que use Windows."),
        3 => ("Teclado", "Si no sabes qué es vim, deja \"Normal\"."),
        4 => ("Actualizaciones", "notty no se conecta a nada sin tu permiso. Si lo activas, solo consulta una vez al día si hay versión nueva en GitHub: sin datos, sin identificadores."),
        _ => ("Todo listo", "¿Te enseño en 20 segundos dónde está cada cosa?"),
    };
    // `.pane{padding:10px 18px 0 6px}`, `h4{font-size:18px;margin:0 0 3px}`,
    // `.sub{line-height:1.45}` sobre el `font-size:12.5px` de la ventana. El título
    // puede llevar emoji (p.ej. "Hola 👋"): `text_color_font` para que salga a color
    // (Segoe UI Emoji) en vez del glifo monocromo por defecto de DirectWrite.
    let left = pane.left + 6.0;
    let right = pane.right - 18.0;
    let title_r = Rect::new(left, pane.top + 10.0, right, pane.top + 10.0 + 24.0);
    text_fit(r, title, &r.fonts().ui_18_semibold, title_r, pal.text.faded(fade));
    if step == 0 {
        // El 👋 va aparte para poder girarlo sobre su muñeca (abajo a la derecha).
        let ex = left + r.measure(title, &r.fonts().ui_18_semibold) + 7.0;
        let er = Rect::new(ex, title_r.top, ex + 30.0, title_r.bottom);
        let pivot = Vector2 { X: ex + 17.0, Y: title_r.bottom - 4.0 };
        r.set_transform(Matrix3x2::rotation_around(wave, pivot));
        let prev_fade = r.fade();
        r.set_fade(prev_fade * fade);
        r.text_color_font("👋", &r.fonts().ui_18_semibold, er, pal.text.faded(fade));
        r.set_fade(prev_fade);
        r.reset_transform();
    }
    let line_h = 12.5 * 1.45;
    let sub_top = title_r.bottom + 3.0;
    let sub_h = r.measure_wrapped(sub, &r.fonts().ui_12_5, right - left, Some(line_h));
    r.text_wrapped(sub, &r.fonts().ui_12_5, Rect::new(left, sub_top, right, sub_top + sub_h), pal.text_2.faded(fade), Some(line_h));
    let bottom = sub_top + sub_h + 14.0;

    if step == STEP_LABELS.len() - 1 {
        draw_checkmark(r, pal, Rect::new(left, bottom, left + 56.0, bottom + 56.0), checkmark, now);
    }
    bottom
}

/// Marca de verificación (mismo trazo que `notty_setup::ui::draw_check_stroke`, viewBox
/// `20x16`, dos segmentos `(2,8.5)->(7.5,13.5)->(18,2.5)`, reescalado a esta caja): un
/// icono más limpio que el original de esta ventana (dos `stroke_line` sueltos, sin
/// puntas ni uniones redondeadas), con un punto relleno en cada extremo/unión para
/// imitar `stroke-linecap:round`/`stroke-linejoin:round` sin geometría D2D dedicada.
/// Empieza en `0` y se revela en 600ms; `anim.start` ya incluye el retraso de 200ms
/// tras la entrada del paso (ver `go_to`), así que aquí solo hace falta leer `value`.
fn draw_checkmark(r: &Renderer, pal: &theme::Palette, box_r: Rect, anim: Option<crate::Anim>, now: Instant) {
    let cx = (box_r.left + box_r.right) / 2.0;
    let cy = (box_r.top + box_r.bottom) / 2.0;
    let radius = box_r.width() / 2.0;
    let t = anim.map(|a| a.value(now, 0.0, 1.0)).unwrap_or(1.0).clamp(0.0, 1.0);
    r.stroke_circle(cx, cy, radius, 2.0, pal.accent.faded(0.35 + 0.65 * t));
    if t <= 0.0 {
        return;
    }

    let k = radius / 10.0; // geometría original pensada para un círculo de radio ~10
    let p0 = (cx - 8.0 * k, cy + 0.5 * k);
    let p1 = (cx - 2.5 * k, cy + 5.5 * k);
    let p2 = (cx + 8.0 * k, cy - 5.5 * k);
    let width = 2.5;
    let len1 = dist(p0, p1);
    let len2 = dist(p1, p2);
    let total = len1 + len2;
    let target = total * t;

    r.fill_circle(p0.0, p0.1, width / 2.0, pal.accent);
    if target <= len1 {
        let f = target / len1;
        let p = lerp(p0, p1, f);
        r.stroke_line(p0.0, p0.1, p.0, p.1, width, pal.accent);
        r.fill_circle(p.0, p.1, width / 2.0, pal.accent);
    } else {
        r.stroke_line(p0.0, p0.1, p1.0, p1.1, width, pal.accent);
        r.fill_circle(p1.0, p1.1, width / 2.0, pal.accent);
        let f = ((target - len1) / len2).clamp(0.0, 1.0);
        let p = lerp(p1, p2, f);
        r.stroke_line(p1.0, p1.1, p.0, p.1, width, pal.accent);
        r.fill_circle(p.0, p.1, width / 2.0, pal.accent);
    }
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn lerp(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// Igual que `.typed{animation:ty 1.4s steps(14) infinite alternate}` (mismo cálculo
/// que `notty_setup::ui::typed`, pero duplicado aquí porque vive en otro crate y no se
/// comparte): revela `s` carácter a carácter en 1.4s y luego lo retira en otro 1.4s,
/// en bucle, mientras la tarjeta de Vim está seleccionada.
fn typed(s: &str, elapsed: f32) -> &str {
    let total = s.chars().count().max(1);
    let cycle = elapsed % 2.8;
    let p = if cycle < 1.4 { cycle / 1.4 } else { (2.8 - cycle) / 1.4 };
    let n = ((p * total as f32).floor() as usize).min(total);
    let end = s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len());
    &s[..end]
}

/// Tarjetas `.choice` (punto de radio + etiqueta + descripción opcional). Devuelve el
/// rectángulo de cada una para el hit-testing. `pop_t` es el progreso (`0..=1`) del
/// "pop" del punto recién marcado; `demo_elapsed`, si viene, teclea la etiqueta
/// `.demo` de la tarjeta seleccionada en vez de mostrarla fija (paso Teclado/Vim).
#[allow(clippy::too_many_arguments)]
fn draw_choices(
    r: &Renderer,
    pal: &theme::Palette,
    pane: Rect,
    top: f32,
    opts: &[ChoiceOpt],
    selected: Option<usize>,
    hover: Hit,
    fade: f32,
    pop_t: f32,
    demo_elapsed: Option<f32>,
) -> Vec<Rect> {
    let mut out = Vec::with_capacity(opts.len());
    let mut y = top;
    for (i, opt) in opts.iter().enumerate() {
        let has_sub = !opt.sub.is_empty();
        let row_h = if has_sub { 44.0 } else { 34.0 };
        let rr = Rect::new(pane.left + 6.0, y, pane.right - 18.0, y + row_h);
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
            // `.rad::after{animation:rp .2s cubic-bezier(.3,0,.2,1)}` de la maqueta:
            // el punto marcado aparece creciendo desde el centro, no de golpe.
            let dot_r = 3.5 * pop_t.clamp(0.0, 1.0);
            if dot_r > 0.0 {
                r.fill_circle(rad_cx, rad_cy, dot_r, pal.accent.faded(fade));
            }
        }
        let text_left = rad_cx + 7.0 + 10.0;
        // Punto 3 del informe de QA: sin subtítulo (p.ej. el paso Tema) la etiqueta
        // debe ocupar toda la fila para que `text` (centrado vertical) la centre de
        // verdad, en vez de solo los 18px superiores.
        let label_top = if has_sub { rr.top + 8.0 } else { rr.top };
        let label_bottom = if has_sub { label_top + 18.0 } else { rr.bottom };
        // Ancho fijo con el texto completo (evita que la caja `.demo` cambie de tamaño
        // mientras se teclea la versión animada). Si no deja sitio a la etiqueta, se
        // omite en vez de pisarla.
        let demo_w = if opt.demo.is_empty() { 0.0 } else { r.measure(opt.demo, &r.fonts().mono_11) + 10.0 };
        let show_demo = demo_w > 0.0 && rr.right - 12.0 - demo_w - 8.0 >= text_left + r.measure(opt.label, &r.fonts().ui_13);
        let label_right = if show_demo { rr.right - 12.0 - demo_w - 8.0 } else { rr.right - 12.0 };
        text_fit(r, opt.label, &r.fonts().ui_13, Rect::new(text_left, label_top, label_right, label_bottom), pal.text.faded(fade));
        if has_sub {
            text_fit(r, opt.sub, &r.fonts().ui_11_5, Rect::new(text_left, label_top + 17.0, rr.right - 12.0, label_top + 33.0), pal.text_3.faded(fade));
        }
        if show_demo {
            let dw = demo_w;
            let dr = Rect::new(rr.right - 12.0 - dw, rr.top + 6.0, rr.right - 12.0, rr.top + 6.0 + 18.0);
            r.fill_round(dr, 3.0, pal.hover.faded(fade));
            let shown = match demo_elapsed {
                Some(elapsed) if on => typed(opt.demo, elapsed),
                _ => opt.demo,
            };
            r.text(shown, &r.fonts().mono_11, Rect::new(dr.left + 5.0, dr.top, dr.right - 5.0, dr.bottom), pal.text_2.faded(fade));
        }
        out.push(rr);
        y += row_h + 6.0;
    }
    out
}

/// Paso Estilo: tres cajas lado a lado (apiladas si la ventana es estrecha), cada una
/// con una miniatura de la app en ese estilo. La elegida se eleva, se le enciende el
/// borde, su miniatura se acerca un poco y le aparece un ✓ con rebote; la que se deja
/// hace lo contrario, a la vez.
#[allow(clippy::too_many_arguments)]
fn draw_style_cards(
    r: &Renderer,
    pal: &theme::Palette,
    pane: Rect,
    top: f32,
    selected: Option<usize>,
    card_from: &[f32; 3],
    style_at: Instant,
    hover: Hit,
    fade: f32,
    now: Instant,
    anim: bool,
) -> Vec<Rect> {
    const LABELS: [(&str, &str); 3] =
        [("Moderna", "Pestañas en el título"), ("Clásica", "Con barra de menús"), ("Zen", "Solo el texto")];
    let left = pane.left + 6.0;
    let right = pane.right - 18.0;
    let gap = 10.0;
    let n = LABELS.len() as f32;
    let avail_h = pane.bottom - top - 14.0;
    // Tres columnas mientras cada caja tenga un ancho legible; si no, se apilan en
    // filas horizontales (miniatura a la izquierda, texto a la derecha).
    let stacked = right - left < STYLE_CARD_MIN_W * n + gap * (n - 1.0);
    let (cw, ch) = if stacked {
        (right - left, ((avail_h - gap * (n - 1.0)) / n).clamp(52.0, 84.0))
    } else {
        let cw = (right - left - gap * (n - 1.0)) / n;
        (cw, avail_h.min(cw * 1.45).max(96.0))
    };
    let k_in = eased(now, style_at, Duration::from_millis(STYLE_CARD_MS), anim);

    let mut out = Vec::with_capacity(LABELS.len());
    for (i, &(label, sub)) in LABELS.iter().enumerate() {
        let preset = STYLE_PRESETS[i];
        let target = if selected == Some(i) { 1.0 } else { 0.0 };
        let k = card_from[i] + (target - card_from[i]) * k_in;
        let hov = if hover == Hit::Choice(i) { 1.0 } else { 0.0 };
        let lift = 3.0 * k + 1.0 * hov * (1.0 - k);
        let (x, y) = if stacked { (left, top + i as f32 * (ch + gap)) } else { (left + i as f32 * (cw + gap), top) };
        let card = Rect::new(x, y - lift, x + cw, y + ch - lift);

        // Halo suave alrededor de la elegida.
        for (grow, a) in [(7.0, 0.04), (4.0, 0.07), (2.0, 0.10)] {
            if k > 0.0 {
                let g = Rect::new(card.left - grow, card.top - grow, card.right + grow, card.bottom + grow);
                r.fill_round(g, 10.0 + grow, pal.accent.faded(a * k * fade));
            }
        }
        r.fill_round(card, 10.0, pal.surface_2.mix(pal.chrome_hi, 0.6 * hov).faded(fade));
        r.fill_round(card, 10.0, pal.accent_soft.faded(k * fade));
        r.stroke_round_rect(card, 10.0, 1.0 + 0.5 * k, pal.line.mix(pal.accent, k).faded(fade));

        // Miniatura: recortada a su caja; dentro, la escena se acerca un 4% al elegirla.
        let pv = if stacked {
            let pw = ((ch - 14.0) * 1.6).max(110.0).min(cw * 0.45);
            Rect::new(card.left + 7.0, card.top + 7.0, card.left + 7.0 + pw, card.bottom - 7.0)
        } else {
            Rect::new(card.left + 9.0, card.top + 9.0, card.right - 9.0, card.bottom - 50.0)
        };
        r.fill_round(pv, 6.0, pal.surface.faded(fade));
        r.push_clip(pv);
        let s = 1.0 + 0.04 * k;
        let center = Vector2 { X: (pv.left + pv.right) / 2.0, Y: (pv.top + pv.bottom) / 2.0 };
        r.set_transform(Matrix3x2::scale_around(s, s, center));
        r.set_fade(fade * (0.55 + 0.45 * k.max(hov)));
        draw_style_scene(r, pal, pv, preset);
        r.set_fade(1.0);
        r.reset_transform();
        r.pop_clip();
        r.stroke_round_rect(pv, 6.0, 1.0, pal.line.faded(fade));

        // El texto deja sitio al ✓ de la derecha para no pisarlo al truncarse.
        let label_r = if stacked {
            let cy = card.top + ch / 2.0;
            Rect::new(pv.right + 12.0, cy - 17.0, card.right - 30.0, cy + 1.0)
        } else {
            Rect::new(card.left + 12.0, pv.bottom + 8.0, card.right - 28.0, pv.bottom + 26.0)
        };
        text_fit(r, label, &r.fonts().ui_13_semibold, label_r, pal.text.faded(fade));
        let sub_r = Rect::new(label_r.left, label_r.bottom, if stacked { label_r.right } else { card.right - 12.0 }, label_r.bottom + 16.0);
        text_fit(r, sub, &r.fonts().ui_11, sub_r, pal.text_3.faded(fade));

        // ✓ en la esquina, creciendo con un pequeño rebote.
        if k > 0.0 {
            let pop = (k + 0.35 * (k * std::f32::consts::PI).sin()).max(0.0);
            let (bx, by) = if stacked { (card.right - 16.0, card.top + ch / 2.0) } else { (card.right - 16.0, label_r.top + 9.0) };
            r.fill_circle(bx, by, 8.0 * pop, pal.accent.faded(fade));
            if pop > 0.6 {
                let c = pal.on_accent.faded(fade * ((pop - 0.6) / 0.4).min(1.0));
                r.stroke_polyline(&[(bx - 3.5, by + 0.2), (bx - 1.0, by + 2.8), (bx + 3.8, by - 2.6)], 1.6, c);
            }
        }
        out.push(card);
    }
    out
}

/// Miniatura de la app para una caja de Estilo: barra de título con pestaña
/// (Moderna) o con título + fila de menú (Clásica), unas líneas de texto y la barra
/// de estado, como `.app`/`.app.clasica .mb` de la maqueta.
fn draw_style_scene(r: &Renderer, pal: &theme::Palette, rr: Rect, preset: Preset) {
    let tb_h = 18.0;
    r.fill(Rect::new(rr.left, rr.top, rr.right, rr.top + tb_h), pal.chrome);
    // Botones de ventana esquemáticos.
    for j in 0..3 {
        let cx = rr.right - 8.0 - j as f32 * 11.0;
        r.fill_circle(cx, rr.top + tb_h / 2.0, 2.0, pal.text_3);
    }
    let body_top = match preset {
        Preset::Zen => {
            // Zen: ni pestañas ni menús, solo el nombre del archivo, tenue.
            r.text("notas.txt", &r.fonts().ui_9, Rect::new(rr.left + 7.0, rr.top, rr.right - 40.0, rr.top + tb_h), pal.text_3);
            rr.top + tb_h
        }
        Preset::Clasica => {
            r.text("notas.txt", &r.fonts().ui_9, Rect::new(rr.left + 7.0, rr.top, rr.right - 40.0, rr.top + tb_h), pal.text_2);
            let mb = Rect::new(rr.left, rr.top + tb_h, rr.right, rr.top + tb_h + 15.0);
            r.fill(mb, pal.chrome_hi);
            r.text("Archivo  Editar  Ver", &r.fonts().ui_9, Rect::new(mb.left + 7.0, mb.top, mb.right, mb.bottom), pal.text_2);
            r.stroke_line(mb.left, mb.bottom - 0.5, mb.right, mb.bottom - 0.5, 1.0, pal.line);
            mb.bottom
        }
        _ => {
            let tab = Rect::new(rr.left + 6.0, rr.top + 4.0, rr.left + 60.0, rr.top + tb_h);
            r.fill_round(tab, 3.0, pal.surface);
            r.text("notas.txt", &r.fonts().ui_9, Rect::new(tab.left + 6.0, tab.top, tab.right - 2.0, tab.bottom), pal.text);
            r.text("+", &r.fonts().ui_9, Rect::new(tab.right + 5.0, tab.top, tab.right + 16.0, tab.bottom), pal.text_3);
            rr.top + tb_h
        }
    };
    let sb_h = 10.0;
    let body = Rect::new(rr.left, body_top, rr.right, rr.bottom - sb_h);
    r.fill(body, pal.surface);
    // Líneas de "texto" en vez de letras: a este tamaño se leen mejor como forma.
    let widths = [0.62, 0.84, 0.48, 0.72, 0.30];
    let mut y = body.top + 8.0;
    for wf in widths {
        if y + 3.0 > body.bottom - 4.0 {
            break;
        }
        let lw = (body.width() - 16.0) * wf;
        r.fill_round(Rect::new(body.left + 8.0, y, body.left + 8.0 + lw, y + 3.0), 1.5, pal.text_3);
        y += 9.0;
    }
    let sb = Rect::new(rr.left, rr.bottom - sb_h, rr.right, rr.bottom);
    r.fill(sb, pal.chrome);
    r.stroke_line(sb.left, sb.top + 0.5, sb.right, sb.top + 0.5, 1.0, pal.line);
}

/// Paso Listo: la tarjeta destacada "Enséñame dónde está todo →" (`#tourGo` en la
/// maqueta) más la nota de que se puede repetir desde Ajustes → Ayuda.
fn draw_tour_go(r: &Renderer, pal: &theme::Palette, pane: Rect, top: f32, hover: Hit, fade: f32) -> Rect {
    let y = top + 56.0 + 12.0;
    let rr = Rect::new(pane.left + 6.0, y, pane.right - 18.0, y + 34.0);
    let bg = if hover == Hit::TourGo { pal.accent_soft } else { pal.surface_2 };
    r.fill_round(rr, 6.0, bg.faded(fade));
    r.stroke_round_rect(rr, 6.0, 1.0, pal.accent.faded(fade));
    text_fit(
        r,
        "Enséñame dónde está todo →",
        &r.fonts().ui_13,
        Rect::new(rr.left + 12.0, rr.top, rr.right - 12.0, rr.bottom),
        pal.accent.faded(fade),
    );
    let note = "Puedes repetirlo desde Ajustes → Ayuda.";
    let note_w = pane.right - 18.0 - (pane.left + 6.0);
    let note_h = r.measure_wrapped(note, &r.fonts().ui_11_5, note_w, None).max(18.0);
    let note_r = Rect::new(pane.left + 6.0, rr.bottom + 10.0, pane.right - 18.0, rr.bottom + 10.0 + note_h);
    r.text_wrapped(note, &r.fonts().ui_11_5, note_r, pal.text_3.faded(fade), None);
    rr
}

/// Piezas comunes a las ventanas con barra de título propia que se pueden
/// redimensionar (esta y las de `notty-setup`): tamaño inicial ajustado al monitor,
/// bordes de redimensionado, maximizado sin salirse del área de trabajo, tamaño mínimo,
/// cambio de DPI entre monitores y texto truncado con «…».
pub mod adaptive {
    use std::borrow::Cow;

    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, MonitorFromRect,
        MonitorFromWindow,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
        HTTOPRIGHT, IsZoomed, MINMAXINFO, NCCALCSIZE_PARAMS, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos,
    };

    use crate::layout::Rect;
    use crate::{Renderer, Rgba};

    /// Franja (en DIPs) junto a cada borde en la que el ratón redimensiona.
    const EDGE_DIP: f32 = 6.0;
    /// Las esquinas se agarran un poco más lejos, como en las ventanas nativas.
    const CORNER_DIP: f32 = 12.0;

    pub fn work_area(monitor: HMONITOR) -> RECT {
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        unsafe {
            let _ = GetMonitorInfoW(monitor, &mut info);
        }
        info.rcWork
    }

    pub fn monitor_dpi(monitor: HMONITOR) -> u32 {
        let (mut x, mut y) = (96u32, 96u32);
        unsafe {
            let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y);
        }
        x.max(96)
    }

    /// Monitor bajo el ratón: donde el usuario acaba de lanzar el programa.
    pub fn monitor_under_cursor() -> HMONITOR {
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
            MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST)
        }
    }

    /// `(x, y, ancho, alto)` en píxeles de una ventana de `w_dip`×`h_dip` a `dpi`:
    /// nunca mayor que `work`, centrada en `center` y metida entera dentro de `work`.
    pub fn initial_rect(w_dip: f32, h_dip: f32, dpi: u32, work: RECT, center: (i32, i32)) -> (i32, i32, i32, i32) {
        let scale = dpi as f32 / 96.0;
        let (work_w, work_h) = (work.right - work.left, work.bottom - work.top);
        let w = ((w_dip * scale).round() as i32).min(work_w).max(1);
        let h = ((h_dip * scale).round() as i32).min(work_h).max(1);
        let x = (center.0 - w / 2).clamp(work.left, (work.right - w).max(work.left));
        let y = (center.1 - h / 2).clamp(work.top, (work.bottom - h).max(work.top));
        (x, y, w, h)
    }

    /// `WM_NCHITTEST` para los bordes: con `WM_NCCALCSIZE` devolviendo 0 toda la
    /// ventana es cliente y Windows ya no los detecta por su cuenta. `None` fuera de
    /// ellos o con la ventana maximizada.
    pub fn edge_hit(hwnd: HWND, lparam: LPARAM) -> Option<LRESULT> {
        unsafe {
            if IsZoomed(hwnd).as_bool() {
                return None;
            }
            let (x, y) = ((lparam.0 as i16) as i32, ((lparam.0 >> 16) as i16) as i32);
            let mut wr = RECT::default();
            GetWindowRect(hwnd, &mut wr).ok()?;
            let scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
            let edge = (EDGE_DIP * scale).round() as i32;
            let corner = (CORNER_DIP * scale).round() as i32;
            let (l, r, t, b) = (x - wr.left, wr.right - x, y - wr.top, wr.bottom - y);
            let near = |d: i32, lim: i32| (0..lim).contains(&d);
            let hit = if (near(t, edge) && near(l, corner)) || (near(l, edge) && near(t, corner)) {
                HTTOPLEFT
            } else if (near(t, edge) && near(r, corner)) || (near(r, edge) && near(t, corner)) {
                HTTOPRIGHT
            } else if (near(b, edge) && near(l, corner)) || (near(l, edge) && near(b, corner)) {
                HTBOTTOMLEFT
            } else if (near(b, edge) && near(r, corner)) || (near(r, edge) && near(b, corner)) {
                HTBOTTOMRIGHT
            } else if near(t, edge) {
                HTTOP
            } else if near(b, edge) {
                HTBOTTOM
            } else if near(l, edge) {
                HTLEFT
            } else if near(r, edge) {
                HTRIGHT
            } else {
                return None;
            };
            Some(LRESULT(hit as isize))
        }
    }

    /// `WM_NCCALCSIZE` con `wparam != 0`: toda la ventana es cliente, salvo al
    /// maximizar, donde Windows la agranda más allá del monitor el grosor del marco
    /// (que aquí no existe) y se cortaría el contenido; se ajusta al área de trabajo.
    pub fn nc_calc_size(hwnd: HWND, lparam: LPARAM) -> LRESULT {
        unsafe {
            if IsZoomed(hwnd).as_bool() {
                if let Some(params) = (lparam.0 as *mut NCCALCSIZE_PARAMS).as_mut() {
                    let monitor = MonitorFromRect(&params.rgrc[0], MONITOR_DEFAULTTONEAREST);
                    params.rgrc[0] = work_area(monitor);
                }
            }
        }
        LRESULT(0)
    }

    /// `WM_GETMINMAXINFO`: mínimo de `min_w`×`min_h` DIPs, pero nunca mayor que el
    /// área de trabajo del monitor (en pantallas pequeñas manda la pantalla).
    pub fn min_max_info(hwnd: HWND, lparam: LPARAM, min_w: f32, min_h: f32) {
        unsafe {
            let Some(mmi) = (lparam.0 as *mut MINMAXINFO).as_mut() else { return };
            let scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
            let work = work_area(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST));
            mmi.ptMinTrackSize.x = ((min_w * scale).round() as i32).min(work.right - work.left);
            mmi.ptMinTrackSize.y = ((min_h * scale).round() as i32).min(work.bottom - work.top);
        }
    }

    /// DPI nuevo de un `WM_DPICHANGED` (`LOWORD(wparam)`).
    pub fn dpi_from_wparam(wparam: WPARAM) -> u32 {
        ((wparam.0 & 0xFFFF) as u32).max(96)
    }

    /// Aplica el rectángulo que Windows sugiere en `WM_DPICHANGED` (`lparam`).
    pub fn apply_suggested_rect(hwnd: HWND, lparam: LPARAM) {
        unsafe {
            if let Some(r) = (lparam.0 as *const RECT).as_ref() {
                let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            }
        }
    }

    /// `s` tal cual si cabe en `max_w`; si no, el prefijo más largo que quepa seguido
    /// de «…» (los formatos de `Fonts` no parten líneas y casi ninguno recorta solo).
    pub fn ellipsize<'a>(r: &Renderer, s: &'a str, fmt: &IDWriteTextFormat, max_w: f32) -> Cow<'a, str> {
        if r.measure(s, fmt) <= max_w {
            return Cow::Borrowed(s);
        }
        // `ends[n - 1]`: byte donde acaba el prefijo de `n` caracteres.
        let ends: Vec<usize> = s.char_indices().map(|(i, _)| i).skip(1).chain(std::iter::once(s.len())).collect();
        let candidate = |n: usize| -> String {
            let end = if n == 0 { 0 } else { ends[n - 1] };
            format!("{}…", s[..end].trim_end())
        };
        let (mut lo, mut hi) = (0usize, ends.len().saturating_sub(1));
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if r.measure(&candidate(mid), fmt) <= max_w {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        if lo == 0 && r.measure("…", fmt) > max_w {
            return Cow::Borrowed("");
        }
        Cow::Owned(candidate(lo))
    }

    /// `Renderer::text` con «…» si `s` no cabe en el ancho de `rect`.
    pub fn text_fit(r: &Renderer, s: &str, fmt: &IDWriteTextFormat, rect: Rect, c: Rgba) {
        r.text(&ellipsize(r, s, fmt, rect.width()), fmt, rect, c);
    }
}
