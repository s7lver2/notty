//! Punto de entrada de `notty-setup`: crea la ventana del asistente (o, con
//! `--update`, la pantalla de actualización de Task 7) y dirige la instalación real a
//! través de `msi_driver` en un hilo aparte.
#![windows_subsystem = "windows"]
use std::path::{Path, PathBuf};
use std::time::Instant;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW, GetWindowLongPtrW,
    HTCAPTION, HTCLIENT, IDC_ARROW, LoadCursorW, MSG, PostMessageW, PostQuitMessage, RegisterClassExW, SW_SHOW,
    SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TranslateMessage, WM_APP, WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCHITTEST, WM_PAINT, WM_SIZE, WM_TIMER, WNDCLASSEXW,
    WS_CAPTION, WS_CLIPSIBLINGS, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
    WS_VISIBLE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, ReleaseCapture, SetCapture, VK_CONTROL};
use windows::core::{PCWSTR, Result, w};

use notty_setup::features::{self, FeatureToggles};
use notty_setup::msi_driver::{self, InstallEvent};
use notty_setup::ui::{self, Hit, MIN_H, MIN_W, State, Step, WIN_H, WIN_W, primary_button, secondary_button};
use notty_ui::welcome_window::adaptive;

const ID_ANIM_TIMER: usize = 1;
/// Un `InstallEvent` recién llegado del hilo de instalación (puntero a un
/// `Box<InstallEvent>`, reconstruido en `wndproc`).
const WM_INSTALL_EVENT: u32 = WM_APP + 1;
/// El hilo de instalación terminó con un código de error de MSI (`wparam` = código).
const WM_INSTALL_FAILED: u32 = WM_APP + 2;
/// Fin de la comprobación de un notty-setup más nuevo: `wparam` 0 = no hay, 1 = no se
/// pudo comprobar, 2 = descargado y verificado (`lparam` = `Box<PathBuf>`).
const WM_SELF_UPDATE: u32 = WM_APP + 3;
/// Lo lleva el notty-setup descargado al relanzarse: si GitHub anunciara una versión
/// que el propio binario no cree tener, no se descargaría a sí mismo en bucle.
const SKIP_SELF_UPDATE: &str = "--skip-self-update";

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
    // Per-monitor v2 (como `notty`): sin esto Windows escala el mapa de bits de la
    // ventana (borroso) y nunca llega `WM_DPICHANGED` al cambiar de monitor.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--update") {
        let relaunch = args.iter().any(|a| a == "--relaunch");
        let from = arg_value(&args, "--from").unwrap_or_else(|| "?".into());
        let to = arg_value(&args, "--to").unwrap_or_else(|| "?".into());
        let _ = run_update_screen(&from, &to, relaunch);
        return;
    }
    let _ = run_wizard(!args.iter().any(|a| a == SKIP_SELF_UPDATE));
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

fn shell_open(target: &str) {
    unsafe {
        let verb = to_wide("open");
        let file = to_wide(target);
        let _ = ShellExecuteW(None, PCWSTR(verb.as_ptr()), PCWSTR(file.as_ptr()), None, None, windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL);
    }
}

/// Selector de carpetas nativo (`IFileOpenDialog` con `FOS_PICKFOLDERS`). Si la carpeta
/// elegida no se llama ya "notty", se instala en una subcarpeta "notty" dentro de ella,
/// como hacen los instaladores de Windows.
fn pick_install_folder(hwnd: HWND, current: &Path, lang: notty_config::Lang) -> Option<PathBuf> {
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
    use windows::Win32::UI::Shell::{FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SHCreateItemFromParsingName, IShellItem, SIGDN_FILESYSPATH};
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = (|| {
            let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            let opts = dialog.GetOptions().ok()?;
            dialog.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
            let title = to_wide(notty_ui::strings::tr(lang, "Carpeta de instalación de notty"));
            let _ = dialog.SetTitle(PCWSTR(title.as_ptr()));
            if let Some(parent) = current.parent() {
                let wide = to_wide(&parent.display().to_string());
                if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(wide.as_ptr()), None) {
                    let _ = dialog.SetFolder(&item);
                }
            }
            dialog.Show(Some(hwnd)).ok()?;
            let item = dialog.GetResult().ok()?;
            let path = PathBuf::from(item.GetDisplayName(SIGDN_FILESYSPATH).ok()?.to_string().ok()?);
            let is_notty = path.file_name().is_some_and(|n| n.eq_ignore_ascii_case("notty"));
            Some(if is_notty { path } else { path.join("notty") })
        })();
        if init.is_ok() {
            CoUninitialize();
        }
        result
    }
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
fn spawn_install(hwnd: HWND, toggles: FeatureToggles, install_folder: PathBuf, lang: notty_config::Lang) {
    let send_hwnd = SendHwnd(hwnd);
    std::thread::spawn(move || {
        // `let send_hwnd = send_hwnd;` fuerza a que la clausura capture la variable
        // entera (que sí es `Send`, por el `unsafe impl` de arriba) en vez de que la
        // captura "disjunta" de Rust 2021 capture solo el campo `.0` (un `HWND`, que
        // no lo es) al ver `send_hwnd.0` más abajo.
        let send_hwnd = send_hwnd;
        let hwnd = send_hwnd.0;
        let fail = |msg: String, code: i32| unsafe {
            let _ = PostMessageW(Some(hwnd), WM_INSTALL_EVENT, WPARAM(0), LPARAM(Box::into_raw(Box::new(InstallEvent::Error(msg))) as isize));
            let _ = PostMessageW(Some(hwnd), WM_INSTALL_FAILED, WPARAM(code as usize), LPARAM(0));
        };
        if MSI_BYTES.is_empty() {
            fail(
                notty_ui::strings::tr(lang, "Este notty-setup se compiló sin el paquete MSI dentro (falta installer\\notty.msi al compilar). Construye el MSI con tools/release.ps1 y vuelve a compilar notty-setup.").into(),
                1620,
            );
            return;
        }
        let msi_path = match extract_embedded_msi() {
            Ok(p) => p,
            Err(e) => {
                fail(format!("{}: {e}", notty_ui::strings::tr(lang, "No se pudo extraer el paquete a la carpeta temporal")), 1619);
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
        match msi_driver::install(&msi_path, &addlocal, &install_folder, on_event) {
            // Tras el MSI se mira el IFEO resultante (no los interruptores): en
            // mantenimiento ADDLOCAL solo añade, así que la feature puede seguir puesta.
            Ok(()) => notty_update::notepad::sync(),
            Err(code) => unsafe {
                let _ = PostMessageW(Some(hwnd), WM_INSTALL_FAILED, WPARAM(code as usize), LPARAM(0));
            },
        }
    });
}

/// `Ok(None)` si ya es la última versión; `Ok(Some(ruta))` con el notty-setup nuevo
/// ya verificado contra `notty_update::PUBKEY`.
fn fetch_newer_setup() -> std::result::Result<Option<PathBuf>, ()> {
    let ua = format!("notty-setup/{}", ui::VERSION);
    let release = notty_update::http::latest_release(notty_update::REPO, &ua).map_err(|_| ())?;
    if !notty_update::is_newer(ui::VERSION, &release.version) {
        return Ok(None);
    }
    let dir = std::env::temp_dir().join("notty-setup-update");
    std::fs::create_dir_all(&dir).map_err(|_| ())?;
    let setup_path = dir.join("notty-setup.exe");
    let sig_path = dir.join("notty-setup.exe.sig");
    let verified = (|| {
        notty_update::http::download(&release.sig_url, &sig_path, |_, _| {}).ok()?;
        notty_update::http::download(&release.setup_url, &setup_path, |_, _| {}).ok()?;
        let sig: [u8; 64] = std::fs::read(&sig_path).ok()?.try_into().ok()?;
        let bytes = std::fs::read(&setup_path).ok()?;
        notty_update::verify(&bytes, &sig, &notty_update::PUBKEY).then_some(())
    })();
    let _ = std::fs::remove_file(&sig_path);
    if verified.is_none() {
        let _ = std::fs::remove_file(&setup_path);
        return Err(());
    }
    Ok(Some(setup_path))
}

fn spawn_self_update_check(hwnd: HWND) {
    let send_hwnd = SendHwnd(hwnd);
    std::thread::spawn(move || {
        let send_hwnd = send_hwnd;
        let (code, payload) = match fetch_newer_setup() {
            Ok(None) => (0, 0),
            Err(()) => (1, 0),
            Ok(Some(path)) => (2, Box::into_raw(Box::new(path)) as isize),
        };
        unsafe {
            let _ = PostMessageW(Some(send_hwnd.0), WM_SELF_UPDATE, WPARAM(code), LPARAM(payload));
        }
    });
}

/// Lanza el notty-setup descargado. `false` si Windows no pudo (p. ej. UAC cancelado).
fn launch_setup(exe: &Path) -> bool {
    unsafe {
        let verb = to_wide("open");
        let file = to_wide(&exe.display().to_string());
        let params = to_wide(SKIP_SELF_UPDATE);
        let h = ShellExecuteW(None, PCWSTR(verb.as_ptr()), PCWSTR(file.as_ptr()), PCWSTR(params.as_ptr()), None, windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL);
        h.0 as isize > 32
    }
}

fn ensure_anim_timer(st: &mut State, hwnd: HWND) {
    if !st.anim_timer_running {
        unsafe {
            let _ = SetTimer(Some(hwnd), ID_ANIM_TIMER, 16, None);
        }
        st.anim_timer_running = true;
    }
}

fn run_wizard(check_for_newer: bool) -> Result<()> {
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

        let (x, y, w_px, h_px, dpi) = centered_on_cursor_monitor(WIN_W, WIN_H);

        let lang = notty_ui::lang::detect_system_lang();
        let title = to_wide(notty_ui::strings::tr(lang, "Instalar notty"));
        // Ventana con barra de título nativa (que `WM_NCCALCSIZE` quita) en vez de
        // `WS_POPUP`: así Windows le da su animación de apertura y de minimizar.
        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_CLIPSIBLINGS,
            x,
            y,
            w_px,
            h_px,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        setup_chrome(hwnd);

        let renderer = notty_ui::Renderer::new(hwnd, dpi)?;
        renderer.set_lang(lang);
        let animations_enabled = notty_ui::window::system_animations_enabled();
        let state = Box::new(State::new(renderer, animations_enabled));
        let ptr = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
        if let Some(st) = ptr.as_mut() {
            ensure_anim_timer(st, hwnd);
            if check_for_newer {
                st.self_update = ui::SelfUpdate::Checking;
                spawn_self_update_check(hwnd);
            }
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

/// `(x, y, ancho, alto, dpi)` de una ventana de `w_dip`×`h_dip` centrada en el monitor
/// bajo el ratón, recortada a su área de trabajo (pantallas pequeñas, escalados altos).
fn centered_on_cursor_monitor(w_dip: f32, h_dip: f32) -> (i32, i32, i32, i32, u32) {
    let monitor = adaptive::monitor_under_cursor();
    let dpi = adaptive::monitor_dpi(monitor);
    let work: RECT = adaptive::work_area(monitor);
    let center = ((work.left + work.right) / 2, (work.top + work.bottom) / 2);
    let (x, y, w, h) = adaptive::initial_rect(w_dip, h_dip, dpi, work, center);
    (x, y, w, h, dpi)
}

/// Mensajes de marco comunes al asistente y a la pantalla de actualización: bordes de
/// redimensionado, maximizado, tamaño mínimo y cambio de DPI. `None` si `msg` no es
/// uno de ellos. `renderer` es `None` mientras la ventana aún no tiene estado.
unsafe fn frame_message(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    renderer: Option<&mut notty_ui::Renderer>,
    min: (f32, f32),
) -> Option<LRESULT> {
    unsafe {
        match msg {
            WM_NCCALCSIZE if wparam.0 != 0 => Some(adaptive::nc_calc_size(hwnd, lparam)),
            WM_GETMINMAXINFO => {
                adaptive::min_max_info(hwnd, lparam, min.0, min.1);
                Some(LRESULT(0))
            }
            WM_DPICHANGED => {
                // El DPI va antes que el tamaño: el `WM_SIZE` que dispara `SetWindowPos`
                // ya redimensiona el render target con la escala nueva.
                if let Some(r) = renderer {
                    r.set_dpi(adaptive::dpi_from_wparam(wparam));
                }
                adaptive::apply_suggested_rect(hwnd, lparam);
                let _ = InvalidateRect(Some(hwnd), None, false);
                Some(LRESULT(0))
            }
            WM_SIZE => {
                if let Some(r) = renderer {
                    r.resize((lparam.0 as u32) & 0xFFFF, ((lparam.0 as u32) >> 16) & 0xFFFF);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}

/// Esquinas redondeadas, sombra, tema oscuro y el borde de 1px `#34353a` de la maqueta.
/// Las maquetas del instalador solo definen el tema oscuro.
unsafe fn setup_chrome(hwnd: HWND) {
    unsafe {
        let prefer_round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &prefer_round as *const _ as *const _, std::mem::size_of_val(&prefer_round) as u32);
        let margins = windows::Win32::UI::Controls::MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 1, cyBottomHeight: 0 };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        notty_ui::window::apply_dark_mode(hwnd, true);
        let border: u32 = 0x003A_3534; // COLORREF 0x00BBGGRR de #34353a
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &border as *const _ as *const _, std::mem::size_of::<u32>() as u32);
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER);
    }
}

fn point_from_lparam(lparam: LPARAM) -> (f32, f32) {
    let x = (lparam.0 as i16) as f32;
    let y = ((lparam.0 >> 16) as i16) as f32;
    (x, y)
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if let Some(res) = frame_message(hwnd, msg, wparam, lparam, ptr.as_mut().map(|st| &mut st.renderer), (MIN_W, MIN_H)) {
            return res;
        }
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
                if let Some(edge) = adaptive::edge_hit(hwnd, lparam) {
                    return edge;
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
                    if st.drag.is_some() {
                        st.drag_to(y / scale);
                        ensure_anim_timer(st, hwnd);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    } else if st.set_hover(ui::hit_test(st, x / scale, y / scale)) {
                        ensure_anim_timer(st, hwnd);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                if let Some(st) = ptr.as_mut() {
                    let (x, y) = point_from_lparam(lparam);
                    let scale = st.renderer.scale();
                    let hit = ui::hit_test(st, x / scale, y / scale);
                    st.pressed = hit;
                    if hit == Hit::ScrollThumb {
                        st.begin_drag(y / scale);
                        SetCapture(hwnd);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if let Some(st) = ptr.as_mut() {
                    let (x, y) = point_from_lparam(lparam);
                    let scale = st.renderer.scale();
                    if st.drag.take().is_some() {
                        let _ = ReleaseCapture();
                    } else if ui::hit_test(st, x / scale, y / scale) == st.pressed {
                        handle_click(hwnd, st, st.pressed);
                    }
                    st.pressed = Hit::None;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                if let Some(st) = ptr.as_mut() {
                    if st.step == Step::Options {
                        let delta = ((wparam.0 >> 16) as i16) as f32;
                        st.scroll_by(-delta / 120.0 * 48.0);
                        ensure_anim_timer(st, hwnd);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                // Con MSI trabajando no se cierra: Windows Installer ya no se puede parar
                // limpiamente desde aquí y la ventana es la que recibe su progreso.
                if let Some(st) = ptr.as_ref() {
                    if st.step == Step::Installing && matches!(st.install, ui::InstallPhase::Running { .. }) {
                        return LRESULT(0);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KEYDOWN => {
                if let Some(st) = ptr.as_mut() {
                    let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                    match wparam.0 as u32 {
                        0x0D => activate_primary(hwnd, st),   // Enter
                        0x1B => activate_secondary(hwnd, st), // Esc
                        0x43 if ctrl => copy_details(hwnd, st), // Ctrl+C
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
            WM_SELF_UPDATE => {
                let newer = (wparam.0 == 2).then(|| *Box::from_raw(lparam.0 as *mut PathBuf));
                if let Some(st) = ptr.as_mut() {
                    st.self_update = ui::SelfUpdate::Idle;
                    match newer {
                        // Ya instalando o instalado: el que corre se queda, sin sorpresas.
                        Some(exe) if matches!(st.step, Step::Welcome | Step::Options) => {
                            if launch_setup(&exe) {
                                close_window(hwnd);
                            } else {
                                st.self_update = ui::SelfUpdate::Failed;
                            }
                        }
                        Some(_) => {}
                        None if wparam.0 == 1 => st.self_update = ui::SelfUpdate::Failed,
                        None => {}
                    }
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

fn failed(st: &State) -> bool {
    st.step == Step::Installing && matches!(st.install, ui::InstallPhase::Error(..) | ui::InstallPhase::Busy)
}

fn activate_primary(hwnd: HWND, st: &mut State) {
    match st.step {
        Step::Welcome => st.go_to(Step::Options),
        Step::Options => start_install(hwnd, st),
        Step::Installing if matches!(st.install, ui::InstallPhase::Busy) => start_install(hwnd, st),
        Step::Installing if failed(st) => back_to_options(st),
        Step::Installing => {}
        Step::Done => open_notty_and_exit(hwnd, st),
    }
    ensure_anim_timer(st, hwnd);
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn activate_secondary(hwnd: HWND, st: &mut State) {
    match st.step {
        Step::Options => st.go_to(Step::Welcome),
        Step::Welcome => close_window(hwnd),
        Step::Installing if failed(st) => back_to_options(st),
        _ => {}
    }
    ensure_anim_timer(st, hwnd);
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn back_to_options(st: &mut State) {
    st.reset_install();
    st.go_to(Step::Options);
}

fn start_install(hwnd: HWND, st: &mut State) {
    st.reset_install();
    st.go_to(Step::Installing);
    spawn_install(hwnd, st.toggles, st.install_folder.clone(), st.renderer.lang());
    ensure_anim_timer(st, hwnd);
}

fn copy_details(hwnd: HWND, st: &mut State) {
    if failed(st) && notty_ui::clipboard::set_clipboard_text(hwnd, &st.error_details()).is_ok() {
        st.copied_at = Some(Instant::now());
        ensure_anim_timer(st, hwnd);
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

fn open_notty_and_exit(hwnd: HWND, st: &mut State) {
    let exe = st.install_folder.join("notty.exe");
    let _ = launch_unelevated(&exe);
    close_window(hwnd);
}

fn handle_click(hwnd: HWND, st: &mut State, hit: Hit) {
    match hit {
        Hit::ChangeFolder => {
            if let Some(folder) = pick_install_folder(hwnd, &st.install_folder, st.renderer.lang()) {
                st.install_folder = folder;
            }
        }
        Hit::OpenLog => shell_open(&msi_driver::log_path().display().to_string()),
        Hit::CopyDetails => copy_details(hwnd, st),
        Hit::FolderRow | Hit::ScrollThumb => {}
        Hit::Close => close_window(hwnd),
        Hit::Minimize => unsafe {
            let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_MINIMIZE);
        },
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
    ensure_anim_timer(st, hwnd);
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

        let (x, y, w_px, h_px, dpi) = centered_on_cursor_monitor(UPDATE_W, UPDATE_H);

        let lang = notty_ui::lang::detect_system_lang();
        let title = to_wide(notty_ui::strings::tr(lang, "Actualizar notty"));
        // `WS_THICKFRAME` para poder redimensionarla; su marco lo quita `WM_NCCALCSIZE`.
        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_THICKFRAME | WS_CLIPSIBLINGS | WS_VISIBLE,
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
        // `.win{border:1px solid #34353a}` (COLORREF es 0x00BBGGRR).
        let border: u32 = 0x003A_3534;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &border as *const _ as *const _, std::mem::size_of::<u32>() as u32);

        let renderer = notty_ui::Renderer::new(hwnd, dpi)?;
        renderer.set_lang(lang);
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
        let _ = SetWindowPos(hwnd, None, x, y, w_px, h_px, SWP_NOZORDER | SWP_FRAMECHANGED);

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

const UPDATE_W: f32 = 520.0;
const UPDATE_H: f32 = 372.0;
const UPDATE_MIN_W: f32 = 380.0;
const UPDATE_MIN_H: f32 = 300.0;
const UPDATE_NOTES: &str = "· Barra de título más compacta\n· Sombras suaves en los desplegables\n· Cursor de bloque en vim Normal";

/// Barra de título de 32px: icono, título y botones "—"/"✕" de 46px (`.tb` de la maqueta).
fn draw_titlebar(r: &notty_ui::Renderer, title: &str, w: f32, hover: Hit, hits: &mut Vec<(notty_ui::layout::Rect, Hit)>) {
    use notty_ui::layout::{Rect, TITLEBAR_H};
    let pal = notty_setup::palette::DARK;
    let cy = TITLEBAR_H / 2.0;
    r.stroke_round_rect(Rect::new(12.6, cy - 6.4, 25.4, cy + 6.4), 3.0, 1.2, pal.titlebar_text);
    r.text(title, &r.fonts().ui_12, Rect::new(34.0, 0.0, w - 92.0, TITLEBAR_H), pal.titlebar_text);
    let min_r = Rect::new(w - 92.0, 0.0, w - 46.0, TITLEBAR_H);
    let close_r = Rect::new(w - 46.0, 0.0, w, TITLEBAR_H);
    if hover == Hit::Minimize {
        r.fill(min_r, pal.caption_hover);
    }
    if hover == Hit::Close {
        r.fill(close_r, pal.close_hover);
    }
    r.text_center("—", &r.fonts().ui_11, min_r, pal.text_3);
    r.text_center("✕", &r.fonts().ui_11, close_r, if hover == Hit::Close { pal.text } else { pal.text_3 });
    hits.push((Rect::new(0.0, 0.0, w, TITLEBAR_H), Hit::Caption));
    hits.push((min_r, Hit::Minimize));
    hits.push((close_r, Hit::Close));
}

fn paint_update(st: &mut UpdateState) {
    use notty_ui::layout::{Rect, TITLEBAR_H};
    let pal = notty_setup::palette::DARK;
    st.renderer.recover_device();
    let (w, h) = st.renderer.size_dips();
    let r = &st.renderer;
    let mut hits = Vec::new();

    r.begin_paint(pal.win);
    draw_titlebar(r, r.tr("Actualizar notty"), w, st.hover, &mut hits);

    // `.body{padding:14px 32px 0}` (menos margen si la ventana es estrecha). Recortado
    // encima del pie para que nada lo pise aunque la ventana sea baja.
    let pad = if w < 460.0 { 20.0 } else { 32.0 };
    let x = pad;
    let right = w - pad;
    let foot_top = h - notty_setup::ui::FOOT_H;
    r.push_clip(Rect::new(0.0, TITLEBAR_H, w, foot_top));
    let mut y = TITLEBAR_H + 14.0;
    adaptive::text_fit(r, r.tr("Actualización disponible"), &r.fonts().ui_20_semibold, Rect::new(x, y, right, y + 27.0), pal.text);
    y += 27.0 + 6.0;

    // `.ver`: "1.2.0 → <b>1.3.0</b> · firma verificada ✓"
    let mono = &r.fonts().mono_12;
    let bold = &r.fonts().mono_12_semibold;
    let head = format!("{} → ", st.from);
    let tail = r.tr(" · firma verificada ✓");
    let head_w = r.measure(&head, mono);
    let to_w = r.measure(&st.to, bold).min((right - x - head_w).max(0.0));
    r.text(&head, mono, Rect::new(x, y, right, y + 18.0), pal.text_2);
    adaptive::text_fit(r, &st.to, bold, Rect::new(x + head_w, y, (x + head_w + to_w).min(right), y + 18.0), pal.text);
    // `mono_12` ya recorta con «…» por sí mismo.
    r.text(tail, mono, Rect::new(x + head_w + to_w, y, right, y + 18.0), pal.text_2);
    y += 18.0 + 12.0;

    // `.notes{padding:10px 12px;line-height:1.65}`
    let line_h = 12.0 * 1.65;
    let text_w = right - x - 24.0;
    let notes = r.tr(UPDATE_NOTES);
    let notes_h = r.measure_wrapped(notes, &r.fonts().ui_12, text_w, Some(line_h)) + 20.0;
    let notes_r = Rect::new(x, y, right, y + notes_h);
    r.fill_round(notes_r, 6.0, pal.field_bg);
    r.stroke_round_rect(notes_r, 6.0, 1.0, pal.foot_border);
    r.text_wrapped(notes, &r.fonts().ui_12, Rect::new(x + 12.0, y + 10.0, right - 12.0, notes_r.bottom), pal.scene_text, Some(line_h));
    y += notes_h + 10.0;
    let hint = r.tr("Tus pestañas y borradores se restaurarán al terminar.");
    let hint_h = r.measure_wrapped(hint, &r.fonts().ui_11_5, right - x, None).max(16.0);
    r.text_wrapped(hint, &r.fonts().ui_11_5, Rect::new(x, y, right, y + hint_h), pal.text_3, None);
    r.pop_clip();

    // `.foot{padding:14px 20px}`
    let foot = Rect::new(0.0, foot_top, w, h);
    let cy = foot.top + foot.height() / 2.0;
    r.fill(foot, pal.foot);
    r.stroke_line(0.0, foot.top + 0.5, w, foot.top + 0.5, 1.0, pal.foot_border);
    if !st.installing {
        let update_btn = primary_button(r, foot.right - 20.0, cy, r.tr("Actualizar"), true, st.hover == Hit::Next);
        let later = secondary_button(r, update_btn.left - 6.0, cy, r.tr("Más tarde"), st.hover == Hit::Back);
        hits.push((later, Hit::Back));
        hits.push((update_btn, Hit::Next));
    } else if let ui::InstallPhase::Running { progress, .. } = &st.install {
        let pct = *progress as f32 / 100.0;
        r.fill(Rect::new(0.0, foot.top, w * pct, foot.top + 2.0), pal.accent);
        r.text(&format!("{}… {progress} %", r.tr("Actualizando")), &r.fonts().ui_12, Rect::new(20.0, foot.top, foot.right - 20.0, foot.bottom), pal.text_3);
    }

    r.end_paint();
    st.hits = hits;
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
        if let Some(res) =
            frame_message(hwnd, msg, wparam, lparam, ptr.as_mut().map(|st| &mut st.renderer), (UPDATE_MIN_W, UPDATE_MIN_H))
        {
            return res;
        }
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
                if let Some(edge) = adaptive::edge_hit(hwnd, lparam) {
                    return edge;
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
                        Hit::Minimize => {
                            let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_MINIMIZE);
                        }
                        Hit::Next => {
                            st.installing = true;
                            let install_folder = PathBuf::from(r"C:\Program Files\notty");
                            spawn_install(hwnd, FeatureToggles::default(), install_folder, st.renderer.lang());
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
