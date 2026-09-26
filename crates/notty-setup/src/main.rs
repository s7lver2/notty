//! Punto de entrada de `notty-setup`: crea la ventana del asistente (o, con
//! `--update`, la pantalla de actualización de Task 7) y dirige la instalación real a
//! través de `msi_driver` en un hilo aparte.
use std::path::{Path, PathBuf};
use std::time::Instant;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW, GetWindowLongPtrW,
    HTCAPTION, HTCLIENT, IDC_ARROW, LoadCursorW, MSG, PostMessageW, PostQuitMessage, RegisterClassExW, SW_SHOW,
    SWP_NOZORDER, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, WM_APP, WM_DESTROY,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCHITTEST, WM_PAINT, WM_TIMER, WNDCLASSEXW,
    WS_CLIPSIBLINGS, WS_POPUP, WS_VISIBLE,
};
use windows::core::{PCWSTR, Result, w};

use notty_setup::features::{self, FeatureToggles};
use notty_setup::msi_driver::{self, InstallEvent};
use notty_setup::ui::{self, Hit, State, Step, WIN_H, WIN_W};

const ID_ANIM_TIMER: usize = 1;
/// Un `InstallEvent` recién llegado del hilo de instalación (puntero a un
/// `Box<InstallEvent>`, reconstruido en `wndproc`).
const WM_INSTALL_EVENT: u32 = WM_APP + 1;
/// El hilo de instalación terminó con un código de error de MSI (`wparam` = código).
const WM_INSTALL_FAILED: u32 = WM_APP + 2;

static MSI_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/notty.msi"));

/// Los handles de ventana de Win32 son números de proceso, no punteros con afinidad
/// de hilo: `PostMessageW` hacia un HWND desde cualquier hilo es el mecanismo estándar
/// para avisar al hilo de la UI. `HWND` envuelve un `*mut c_void` y por tanto no es
/// `Send` por defecto; este envoltorio se lo concede explícitamente para ese único uso.
#[derive(Clone, Copy)]
struct SendHwnd(HWND);
unsafe impl Send for SendHwnd {}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--update") {
        let relaunch = args.iter().any(|a| a == "--relaunch");
        let from = arg_value(&args, "--from").unwrap_or_else(|| "?".into());
        let to = arg_value(&args, "--to").unwrap_or_else(|| "?".into());
        let _ = run_update_screen(&from, &to, relaunch);
        return;
    }
    let _ = run_wizard();
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

/// Extrae el MSI embebido (Task 6 Paso 6) a `%TEMP%\notty-install\notty.msi`.
/// `build.rs` deja un fichero vacío en compilaciones de desarrollo sin
/// `installer/notty.msi` todavía construido -- en ese caso esto falla más adelante,
/// dentro de `MsiInstallProduct`, con un error claro en vez de silenciosamente.
fn extract_embedded_msi() -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join("notty-install");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("notty.msi");
    std::fs::write(&path, MSI_BYTES)?;
    Ok(path)
}

fn launch_unelevated(exe: &Path) -> windows::core::Result<()> {
    let mut cmdline = to_wide(&format!("\"{}\"", exe.display()));
    let mut si = STARTUPINFOW::default();
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut pi = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            None,
            Some(windows::core::PWSTR(cmdline.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_UNICODE_ENVIRONMENT,
            None,
            None,
            &si,
            &mut pi,
        )?;
        let _ = windows::Win32::Foundation::CloseHandle(pi.hProcess);
        let _ = windows::Win32::Foundation::CloseHandle(pi.hThread);
    }
    Ok(())
}

fn open_default_apps() {
    unsafe {
        let verb = to_wide("open");
        let file = to_wide("ms-settings:defaultapps");
        let _ = ShellExecuteW(None, PCWSTR(verb.as_ptr()), PCWSTR(file.as_ptr()), None, None, windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL);
    }
}

/// Arranca el hilo de instalación real: extrae el MSI embebido y llama a
/// `msi_driver::install`, reenviando cada `InstallEvent` a la ventana vía
/// `PostMessageW(WM_INSTALL_EVENT)` (Task 6 Paso 4). Si `install` devuelve `Err`, se
/// avisa por separado con `WM_INSTALL_FAILED` para que `wndproc` distinga 1602/1618
/// del resto (ver `ui::State::on_install_failed`).
fn spawn_install(hwnd: HWND, toggles: FeatureToggles, install_folder: PathBuf) {
    let send_hwnd = SendHwnd(hwnd);
    std::thread::spawn(move || {
        // `let send_hwnd = send_hwnd;` fuerza a que la clausura capture la variable
        // entera (que sí es `Send`, por el `unsafe impl` de arriba) en vez de que la
        // captura "disjunta" de Rust 2021 capture solo el campo `.0` (un `HWND`, que
        // no lo es) al ver `send_hwnd.0` más abajo.
        let send_hwnd = send_hwnd;
        let hwnd = send_hwnd.0;
        let msi_path = match extract_embedded_msi() {
            Ok(p) => p,
            Err(e) => {
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_INSTALL_EVENT, WPARAM(0), LPARAM(Box::into_raw(Box::new(InstallEvent::Error(e.to_string()))) as isize));
                }
                return;
            }
        };
        let addlocal = features::to_addlocal(&toggles);
        let on_event = move |event: InstallEvent| unsafe {
            let send_hwnd = send_hwnd;
            let hwnd = send_hwnd.0;
            let ptr = Box::into_raw(Box::new(event));
            let _ = PostMessageW(Some(hwnd), WM_INSTALL_EVENT, WPARAM(0), LPARAM(ptr as isize));
        };
        if let Err(code) = msi_driver::install(&msi_path, &addlocal, &install_folder, on_event) {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_INSTALL_FAILED, WPARAM(code as usize), LPARAM(0));
            }
        }
    });
}

fn ensure_anim_timer(st: &mut State, hwnd: HWND) {
    if !st.anim_timer_running {
        unsafe {
            let _ = SetTimer(Some(hwnd), ID_ANIM_TIMER, 16, None);
        }
        st.anim_timer_running = true;
    }
}

fn run_wizard() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("NottySetupClass");
        let icon = notty_ui::window::app_icon(instance.into());
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

        // Ventana temporal para conocer el DPI del monitor antes de crear la real,
        // igual que hace `settings_window::open` con `parent`; aquí no hay ventana
        // "padre" (notty-setup es su propio proceso), así que se usa el escritorio.
        let desktop = windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow();
        let dpi = GetDpiForWindow(desktop).max(96);
        let scale = dpi as f32 / 96.0;
        let (w_px, h_px) = ((WIN_W * scale) as i32, (WIN_H * scale) as i32);

        let mut work_area = RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::SystemParametersInfoW(
            windows::Win32::UI::WindowsAndMessaging::SPI_GETWORKAREA,
            0,
            Some(&mut work_area as *mut _ as *mut _),
            Default::default(),
        );
        let x = work_area.left + ((work_area.right - work_area.left) - w_px) / 2;
        let y = work_area.top + ((work_area.bottom - work_area.top) - h_px) / 2;

        let title = to_wide("Instalar notty");
        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_CLIPSIBLINGS | WS_VISIBLE,
            x,
            y,
            w_px,
            h_px,
            None,
            None,
            Some(instance.into()),
            None,
        )?;

        let prefer_round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &prefer_round as *const _ as *const _, std::mem::size_of_val(&prefer_round) as u32);
        let margins = windows::Win32::UI::Controls::MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 1, cyBottomHeight: 0 };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        // Las maquetas del instalador solo definen el tema oscuro (ver
        // `notty_setup::palette`): se fuerza oscuro en vez de seguir
        // `system_uses_dark_mode()` hasta que exista una maqueta clara.
        notty_ui::window::apply_dark_mode(hwnd, true);

        let renderer = notty_ui::Renderer::new(hwnd, dpi)?;
        let animations_enabled = notty_ui::window::system_animations_enabled();
        let state = Box::new(State::new(renderer, animations_enabled));
        let ptr = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
        if let Some(st) = ptr.as_mut() {
            ensure_anim_timer(st, hwnd);
        }

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetWindowPos(hwnd, None, x, y, w_px, h_px, SWP_NOZORDER);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
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
                    let still_animating = ui::paint(st);
                    if still_animating {
                        ensure_anim_timer(st, hwnd);
                    }
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
                    let hit = ui::hit_test(st, pt.x as f32 / scale, pt.y as f32 / scale);
                    if hit == Hit::Caption {
                        return LRESULT(HTCAPTION as isize);
                    }
                }
                LRESULT(HTCLIENT as isize)
            }
            WM_MOUSEMOVE => {
                if let Some(st) = ptr.as_mut() {
                    let (x, y) = point_from_lparam(lparam);
                    let scale = st.renderer.scale();
                    let hit = ui::hit_test(st, x / scale, y / scale);
                    if hit != st.hover {
                        st.hover = hit;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                if let Some(st) = ptr.as_mut() {
                    let (x, y) = point_from_lparam(lparam);
                    let scale = st.renderer.scale();
                    handle_click(hwnd, st, x / scale, y / scale);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => LRESULT(0),
            WM_KEYDOWN => {
                if let Some(st) = ptr.as_mut() {
                    match wparam.0 as u32 {
                        0x0D => activate_primary(hwnd, st), // Enter
                        0x1B => activate_secondary(hwnd, st), // Esc
                        _ => {}
                    }
                }
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == ID_ANIM_TIMER {
                    if let Some(st) = ptr.as_mut() {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        if !st.any_animating(Instant::now()) {
                            let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(Some(hwnd), ID_ANIM_TIMER);
                            st.anim_timer_running = false;
                        }
                    }
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_INSTALL_EVENT => {
                if let Some(st) = ptr.as_mut() {
                    let event = *Box::from_raw(lparam.0 as *mut InstallEvent);
                    st.on_install_event(event);
                    ensure_anim_timer(st, hwnd);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_INSTALL_FAILED => {
                if let Some(st) = ptr.as_mut() {
                    st.on_install_failed(wparam.0 as i32);
                    ensure_anim_timer(st, hwnd);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(Some(hwnd), ID_ANIM_TIMER);
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

fn close_window(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(Some(hwnd), windows::Win32::UI::WindowsAndMessaging::WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

fn activate_primary(hwnd: HWND, st: &mut State) {
    match st.step {
        Step::Welcome => st.go_to(Step::Options),
        Step::Options => start_install(hwnd, st),
        Step::Installing => {}
        Step::Done => open_notty_and_exit(hwnd, st),
    }
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn activate_secondary(hwnd: HWND, st: &mut State) {
    match st.step {
        Step::Options => st.go_to(Step::Welcome),
        Step::Welcome => close_window(hwnd),
        _ => {}
    }
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn start_install(hwnd: HWND, st: &mut State) {
    st.go_to(Step::Installing);
    spawn_install(hwnd, st.toggles, st.install_folder.clone());
    ensure_anim_timer(st, hwnd);
}

fn open_notty_and_exit(hwnd: HWND, st: &mut State) {
    let exe = st.install_folder.join("notty.exe");
    let _ = launch_unelevated(&exe);
    close_window(hwnd);
}

fn handle_click(hwnd: HWND, st: &mut State, x: f32, y: f32) {
    let hit = ui::hit_test(st, x, y);
    match hit {
        Hit::Close => close_window(hwnd),
        Hit::Back => activate_secondary(hwnd, st),
        Hit::Next => activate_primary(hwnd, st),
        Hit::GroupHeader(i) => {
            st.toggle_group(i);
            ensure_anim_timer(st, hwnd);
        }
        Hit::Toggle(i) => {
            if st.step == Step::Options {
                st.toggle_feature(i);
            }
        }
        Hit::OpenDefaultApps => open_default_apps(),
        Hit::OpenNotty => open_notty_and_exit(hwnd, st),
        Hit::Retry => start_install(hwnd, st),
        Hit::None | Hit::Caption => {}
    }
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

// --- Modo `--update --relaunch` (Task 7) -----------------------------------------

/// Pantalla única de actualización (Task 7): sin carril de pasos, muestra
/// "<from> → <to> · firma verificada ✓" y los botones "Más tarde"/"Actualizar". Usa
/// su propio bucle de pintado reducido en vez de `ui::paint` porque su maqueta
/// (última tarjeta de `instalador-flujo.html`) no tiene ni carril ni vista previa.
fn run_update_screen(from: &str, to: &str, relaunch: bool) -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("NottyUpdateClass");
        let icon = notty_ui::window::app_icon(instance.into());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DROPSHADOW,
            lpfnWndProc: Some(update_wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hIcon: icon.unwrap_or_default(),
            hIconSm: icon.unwrap_or_default(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let desktop = windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow();
        let dpi = GetDpiForWindow(desktop).max(96);
        let scale = dpi as f32 / 96.0;
        let (w_px, h_px) = ((520.0 * scale) as i32, (300.0 * scale) as i32);
        let mut work_area = RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::SystemParametersInfoW(
            windows::Win32::UI::WindowsAndMessaging::SPI_GETWORKAREA,
            0,
            Some(&mut work_area as *mut _ as *mut _),
            Default::default(),
        );
        let x = work_area.left + ((work_area.right - work_area.left) - w_px) / 2;
        let y = work_area.top + ((work_area.bottom - work_area.top) - h_px) / 2;

        let title = to_wide("Actualizar notty");
        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_CLIPSIBLINGS | WS_VISIBLE,
            x,
            y,
            w_px,
            h_px,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let prefer_round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &prefer_round as *const _ as *const _, std::mem::size_of_val(&prefer_round) as u32);
        let margins = windows::Win32::UI::Controls::MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 1, cyBottomHeight: 0 };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        notty_ui::window::apply_dark_mode(hwnd, true);

        let renderer = notty_ui::Renderer::new(hwnd, dpi)?;
        let state = Box::new(UpdateState {
            renderer,
            from: from.to_string(),
            to: to.to_string(),
            relaunch,
            hover: Hit::None,
            hits: Vec::new(),
            install: ui::InstallPhase::Running { progress: 0, progress_anim: None, action_text: String::new(), action_anim: None },
            installing: false,
        });
        let ptr = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetWindowPos(hwnd, None, x, y, w_px, h_px, SWP_NOZORDER);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

struct UpdateState {
    renderer: notty_ui::Renderer,
    from: String,
    to: String,
    relaunch: bool,
    hover: Hit,
    hits: Vec<(notty_ui::layout::Rect, Hit)>,
    install: ui::InstallPhase,
    installing: bool,
}

fn paint_update(st: &mut UpdateState) {
    use notty_ui::layout::{Rect, TITLEBAR_H};
    let pal = notty_setup::palette::DARK;
    let (w, h) = st.renderer.size_dips();
    let r = &st.renderer;
    st.hits.clear();

    r.begin_paint(pal.win);
    let titlebar = Rect::new(0.0, 0.0, w, TITLEBAR_H);
    r.text("Actualizar notty", &r.fonts().ui_12, Rect::new(12.0, 0.0, w - 46.0, TITLEBAR_H), pal.titlebar_text);
    let close_r = Rect::new(w - 46.0, 0.0, w, TITLEBAR_H);
    let ccx = close_r.left + close_r.width() / 2.0;
    let ccy = close_r.top + close_r.height() / 2.0;
    r.stroke_line(ccx - 5.0, ccy - 5.0, ccx + 5.0, ccy + 5.0, 1.0, pal.text_3);
    r.stroke_line(ccx + 5.0, ccy - 5.0, ccx - 5.0, ccy + 5.0, 1.0, pal.text_3);
    st.hits.push((titlebar, Hit::Caption));
    st.hits.push((close_r, Hit::Close));

    let x = 32.0;
    let mut y = TITLEBAR_H + 24.0;
    r.text("Actualización disponible", &r.fonts().ui_20_semibold, Rect::new(x, y, w - 32.0, y + 28.0), pal.text);
    y += 34.0;
    r.text(&format!("{} → {} · firma verificada ✓", st.from, st.to), &r.fonts().mono_12, Rect::new(x, y, w - 32.0, y + 18.0), pal.text_2);
    y += 30.0;
    let notes = Rect::new(x, y, w - 32.0, y + 60.0);
    r.fill_round(notes, 6.0, pal.group);
    r.text(
        "· Barra de título más compacta\n· Sombras suaves en los desplegables\n· Cursor de bloque en vim Normal",
        &r.fonts().ui_12,
        Rect::new(notes.left + 10.0, notes.top + 8.0, notes.right - 10.0, notes.bottom - 8.0),
        pal.text_2,
    );
    y += 70.0;
    r.text("Tus pestañas y borradores se restaurarán al terminar.", &r.fonts().ui_11, Rect::new(x, y, w - 32.0, y + 16.0), pal.text_3);

    let foot = Rect::new(0.0, h - 56.0, w, h);
    r.fill(foot, pal.foot);
    r.stroke_line(0.0, foot.top, w, foot.top, 1.0, pal.foot_border);
    if !st.installing {
        let later = Rect::new(foot.left + 18.0, foot.top + 10.0, foot.left + 18.0 + 90.0, foot.bottom - 10.0);
        r.fill_round(later, 4.0, pal.btn2);
        r.stroke_round_rect(later, 4.0, 1.0, pal.btn2_border);
        r.text("Más tarde", &r.fonts().ui_12_5, later, pal.text);
        st.hits.push((later, Hit::Back));

        let label = "Actualizar";
        let bw = r.measure(label, &r.fonts().ui_12_5) + 52.0;
        let update_btn = Rect::new(foot.right - 18.0 - bw, foot.top + 10.0, foot.right - 18.0, foot.bottom - 10.0);
        r.fill_round(update_btn, 4.0, pal.accent);
        r.stroke_round_rect(update_btn, 4.0, 1.0, pal.accent_border);
        r.fill_round(Rect::new(update_btn.left + 16.0, update_btn.top + 8.0, update_btn.left + 24.0, update_btn.top + 20.0), 2.0, pal.on_accent);
        r.text(label, &r.fonts().ui_12_5, Rect::new(update_btn.left + 34.0, update_btn.top, update_btn.right - 14.0, update_btn.bottom), pal.on_accent);
        st.hits.push((update_btn, Hit::Next));
    } else if let ui::InstallPhase::Running { progress, .. } = &st.install {
        r.text(&format!("Actualizando… {progress} %"), &r.fonts().ui_12_5, Rect::new(foot.left + 18.0, foot.top, foot.right - 18.0, foot.bottom), pal.text_2);
    }

    r.end_paint();
}

fn update_hit_test(st: &UpdateState, x: f32, y: f32) -> Hit {
    for &(r, h) in st.hits.iter().rev() {
        if r.contains(x, y) {
            return h;
        }
    }
    if y < notty_ui::layout::TITLEBAR_H { Hit::Caption } else { Hit::None }
}

extern "system" fn update_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut UpdateState;
        match msg {
            WM_PAINT => {
                if let Some(st) = ptr.as_mut() {
                    paint_update(st);
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
                    if update_hit_test(st, pt.x as f32 / scale, pt.y as f32 / scale) == Hit::Caption {
                        return LRESULT(HTCAPTION as isize);
                    }
                }
                LRESULT(HTCLIENT as isize)
            }
            WM_MOUSEMOVE => {
                if let Some(st) = ptr.as_mut() {
                    let (x, y) = point_from_lparam(lparam);
                    let scale = st.renderer.scale();
                    let hit = update_hit_test(st, x / scale, y / scale);
                    if hit != st.hover {
                        st.hover = hit;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                if let Some(st) = ptr.as_mut() {
                    let (x, y) = point_from_lparam(lparam);
                    let scale = st.renderer.scale();
                    let hit = update_hit_test(st, x / scale, y / scale);
                    match hit {
                        Hit::Close | Hit::Back => close_window(hwnd),
                        Hit::Next => {
                            st.installing = true;
                            let install_folder = PathBuf::from(r"C:\Program Files\notty");
                            spawn_install(hwnd, FeatureToggles::default(), install_folder);
                        }
                        _ => {}
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_INSTALL_EVENT => {
                if let Some(st) = ptr.as_mut() {
                    let event = *Box::from_raw(lparam.0 as *mut InstallEvent);
                    let relaunch = st.relaunch;
                    let done = matches!(event, InstallEvent::Done);
                    if let ui::InstallPhase::Running { progress, .. } = &mut st.install {
                        if let InstallEvent::Progress(p) = event {
                            *progress = p;
                        }
                    }
                    if done {
                        if relaunch {
                            let _ = launch_unelevated(Path::new(r"C:\Program Files\notty\notty.exe"));
                        }
                        close_window(hwnd);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
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
