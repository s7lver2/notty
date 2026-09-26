use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{InvalidateRect, ValidateRect};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{MARGINS, WM_MOUSELEAVE};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, GetSystemMetricsForDpi,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent, VK_CONTROL,
    VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, HICON, HTCAPTION, HTCLIENT, HTMAXBUTTON, HTTOP, IDC_ARROW, IDC_IBEAM, IsZoomed, KillTimer,
    LoadCursorW, LoadIconW, MSG, NCCALCSIZE_PARAMS, PostQuitMessage, RegisterClassExW, SM_CXPADDEDBORDER,
    SM_CYFRAME, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SW_SHOW, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SetCursor, SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow,
    TranslateMessage, WHEEL_DELTA, WM_ACTIVATE, WM_CHAR, WM_DESTROY, WM_DPICHANGED, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP,
    WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_PAINT, WM_SETCURSOR, WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER,
    WNDCLASSEXW, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW,
};
use windows::core::{PCWSTR, Result, w};

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

use crate::layout;
use crate::{EditorState, Hit, Modifiers, Renderer, Viewport};

/// Id del `SetTimer` de autoguardado (dispara cada segundo; el manejador de `WM_TIMER`
/// decide si de verdad hay algo que guardar).
const ID_AUTOSAVE_TIMER: usize = 1;
/// Id del `SetTimer` de sondeo del pipe de instancia única (Task 8): más corto que el
/// de autoguardado para que abrir un archivo desde una segunda invocación de `notty`
/// se note casi al instante.
const ID_IPC_TIMER: usize = 2;
/// Id del `SetTimer` de animación (60Hz): repinta mientras haya alguna animación en
/// curso (menú/sugerencias al abrir, cambio de pestaña...) y se para en cuanto la
/// última termina.
const ID_ANIM_TIMER: usize = 3;

/// Snapshot de documentos sucios (nombre, texto, sucio) leído por el `panic hook` para
/// volcar a `notty_io::recovery_dir()`. Vive en memoria estática (`Box::leak`) para que
/// el hook, que se instala una única vez para todo el proceso, tenga una dirección
/// válida sin depender de que el hilo que entra en pánico coopere activamente.
type RecoverySnapshot = Mutex<Vec<(String, String, bool)>>;

/// Estado ligado a una ventana concreta: se guarda en `GWLP_USERDATA` mientras vive.
struct WindowState {
    ws: crate::Workspace,
    renderer: Renderer,
    mouse_down: bool,
    selection_anchor: usize,
    cfg: Rc<RefCell<notty_config::Config>>,
    ui_keymap: std::collections::HashMap<(u32, notty_input::Modifiers), notty_input::UiCommand>,
    /// Solo se usa cuando `cfg.ui.menubar == MenuBar::Alt`: si el menú está desplegado.
    menu_visible: bool,
    /// Zona bajo el ratón / con el botón pulsado (barra de título, pestañas, ✕, +, ...).
    hover: Hit,
    pressed: Hit,
    active_window: bool,
    /// Índice del menú de la barra abierto (clic en `Hit::Menu(i)`), si lo hay.
    open_menu: Option<usize>,
    /// Snapshot para el `panic hook`, ver `RecoverySnapshot`.
    recovery: &'static RecoverySnapshot,
    /// Extremo receptor del pipe de instancia única (Task 8): `None` si `spawn_pipe_server`
    /// no llegó a arrancar (no debería pasar en la instancia con ventana, pero se trata
    /// como "nadie más pide abrir nada" en vez de entrar en pánico).
    ipc_rx: Option<std::sync::mpsc::Receiver<notty_ipc::Message>>,
    /// Si las animaciones del sistema están activadas (Accesibilidad → Efectos
    /// visuales); calculado una vez al arrancar, ver `system_animations_enabled`.
    animations_enabled: bool,
    /// Animación en curso del menú/sugerencias que se acaba de abrir (fundido +
    /// desplazamiento). `None` en reposo. Se limpia sola cuando `is_done`, no hace
    /// falta borrarla al cerrar el menú (al cerrarse ya no se dibuja).
    popup_open_anim: Option<crate::Anim>,
    /// Animación en curso del fundido de fondo de la pestaña activa al cambiar de
    /// pestaña. `None` en reposo.
    tab_switch_anim: Option<crate::Anim>,
    /// Si el `SetTimer` de animación (`ID_ANIM_TIMER`) está corriendo.
    anim_timer_running: bool,
}

impl WindowState {
    /// La config a pasar al renderer: si el menú es `Alt`, se sustituye por
    /// `Visible`/`Hidden` según `menu_visible` sin tocar la config en disco.
    fn render_ui(&self) -> notty_config::UiConfig {
        let mut ui = self.cfg.borrow().ui;
        if ui.menubar == notty_config::MenuBar::Alt {
            ui.menubar =
                if self.menu_visible { notty_config::MenuBar::Visible } else { notty_config::MenuBar::Hidden };
        }
        ui
    }

    /// Contexto de dibujo que no vive en `Workspace`/`UiConfig`.
    fn view_state(&self, hwnd: HWND) -> crate::ViewState {
        let dark = crate::is_dark(self.cfg.borrow().ui.theme, system_uses_dark_mode());
        let maximized = unsafe { IsZoomed(hwnd).as_bool() };
        crate::ViewState {
            dark,
            hover: self.hover,
            pressed: self.pressed,
            maximized,
            active_window: self.active_window,
            menu_bar_visible: self.menu_bar_visible(),
            open_menu: self.open_menu,
            popup_open: self.popup_open_anim.map(|a| a.value(std::time::Instant::now(), 0.0, 1.0)),
            tab_switch: self.tab_switch_anim.map(|a| a.value(std::time::Instant::now(), 0.0, 1.0)),
        }
    }

    /// Si la barra de menús debe dibujarse: `Visible` siempre, `Alt` solo mientras
    /// `menu_visible` (el usuario acaba de pulsar Alt).
    fn menu_bar_visible(&self) -> bool {
        let menubar = self.cfg.borrow().ui.menubar;
        menubar == notty_config::MenuBar::Visible || (menubar == notty_config::MenuBar::Alt && self.menu_visible)
    }

    /// Rectángulo del cuerpo y ancho del canal de números para el estado actual
    /// (bandas resueltas con la config y el número de pestañas reales).
    fn body_and_gutter(&self) -> (layout::Rect, f32) {
        let ui = self.cfg.borrow().ui;
        let total = self.ws.active().doc.buffer().len_lines();
        self.renderer.body_and_gutter(&ui, self.ws.len(), self.menu_bar_visible(), total, self.ws.active().raw.is_some())
    }
}

/// Arranca el temporizador de animación (60Hz) si no estaba ya corriendo. Se llama
/// cada vez que arranca una animación nueva.
fn ensure_anim_timer(w: &mut WindowState, hwnd: HWND) {
    if !w.anim_timer_running {
        unsafe {
            let _ = SetTimer(Some(hwnd), ID_ANIM_TIMER, 16, None);
        }
        w.anim_timer_running = true;
    }
}

/// Arranca (o reinicia) la animación de fundido+desplazamiento del menú/sugerencias
/// que se acaba de abrir.
fn start_popup_anim(w: &mut WindowState, hwnd: HWND) {
    w.popup_open_anim =
        Some(crate::Anim::new_maybe(std::time::Instant::now(), std::time::Duration::from_millis(120), w.animations_enabled));
    ensure_anim_timer(w, hwnd);
}

/// Arranca (o reinicia) la animación de fundido de fondo de la pestaña activa.
fn start_tab_switch_anim(w: &mut WindowState, hwnd: HWND) {
    w.tab_switch_anim =
        Some(crate::Anim::new_maybe(std::time::Instant::now(), std::time::Duration::from_millis(100), w.animations_enabled));
    ensure_anim_timer(w, hwnd);
}

fn point_from_lparam(lparam: LPARAM) -> (f32, f32) {
    let x = (lparam.0 as i16) as f32;
    let y = ((lparam.0 >> 16) as i16) as f32;
    (x, y)
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// El icono propio de notty (`assets/notty.ico`, empotrado en el `.exe` como recurso 1
/// por `crates/notty/build.rs`), para la barra de título, Alt+Tab y la bandeja. `None`
/// si por lo que sea no se pudo cargar (no debería pasar con el recurso ya empotrado);
/// en ese caso Windows se queda con el icono por defecto, no es un error fatal.
///
/// # Safety
/// `instance` debe ser un módulo cargado válido (el de `GetModuleHandleW(None)`, como
/// en todos los llamadores de esta función).
#[allow(clippy::manual_dangling_ptr)] // MAKEINTRESOURCE(1): un entero disfrazado de puntero, nunca se desreferencia.
pub unsafe fn app_icon(instance: windows::Win32::Foundation::HINSTANCE) -> Option<HICON> {
    unsafe { LoadIconW(Some(instance), PCWSTR(1usize as *const u16)).ok() }
}

/// Título de la ventana: `"<ruta o 'sin título'>{ ' •' si hay cambios sin guardar} · notty"`.
fn window_title(state: &EditorState) -> String {
    let base = match &state.path {
        Some(p) => p.display().to_string(),
        None => "sin título".to_string(),
    };
    let dirty = if state.doc.is_dirty() { " •" } else { "" };
    format!("{base}{dirty} · notty")
}

unsafe fn update_title(hwnd: HWND, state: &EditorState) {
    unsafe {
        let title_wide = to_wide(&window_title(state));
        let _ = SetWindowTextW(hwnd, PCWSTR(title_wide.as_ptr()));
    }
}

/// `on_open_path` de Ajustes («Editar el archivo», «Abrir [keys]»): abre `path` como
/// documento en la ventana principal `hwnd`. Se recupera el `WindowState` desde
/// `GWLP_USERDATA`, igual que hace `wndproc`; si `hwnd` ya no es válido (se cerró
/// mientras Ajustes estaba abierto), no hace nada.
fn open_config_as_document(hwnd: HWND, path: std::path::PathBuf) {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        if let Some(w) = ptr.as_mut() {
            let cfg = w.cfg.borrow().clone();
            if let Ok(opened) = crate::open_as_document(&path) {
                w.ws.open(maybe_vim(EditorState::from_opened(opened), &cfg));
            } else {
                let mut state = EditorState::new_empty();
                state.path = Some(path);
                w.ws.open(maybe_vim(state, &cfg));
            }
            update_title(hwnd, w.ws.active());
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

/// Abre la ventana de Ajustes sobre `hwnd`: `Ctrl+,`, el menú Archivo → Ajustes, y el
/// engranaje de la barra de título llegan todos aquí, para no repetir el `Box::new`.
fn open_settings(w: &WindowState, hwnd: HWND) {
    let cfg_for_settings = w.cfg.clone();
    let cfg_for_theme = w.cfg.clone();
    let _ = crate::settings_window::open(
        hwnd,
        cfg_for_settings,
        Box::new(move || unsafe {
            let dark = crate::is_dark(cfg_for_theme.borrow().ui.theme, system_uses_dark_mode());
            apply_dark_mode(hwnd, dark);
            let _ = InvalidateRect(Some(hwnd), None, false);
        }),
        Box::new(move |path| open_config_as_document(hwnd, path)),
    );
}

/// Guarda el documento activo, pero antes comprueba si el archivo cambió en disco
/// desde que se abrió: si es así, abre `Prompt::Conflict` en vez de escribir encima.
fn try_save(w: &mut WindowState) {
    let path_and_since = {
        let st = w.ws.active();
        st.path.clone().map(|p| (p, st.open_mtime))
    };
    if let Some((path, Some(since))) = path_and_since {
        if notty_io::changed_since(&path, since) {
            w.ws.open_conflict();
            return;
        }
    }
    let _ = w.ws.active_mut().save();
    if let Some(path) = w.ws.active().path.clone() {
        w.ws.active_mut().open_mtime = notty_io::mtime(&path).ok();
    }
}

/// `WM_TIMER` de autoguardado: dispara cada segundo, pero solo actúa si hay cambios
/// sin guardar (equivalente en la práctica a reprogramar el temporizador tras cada
/// tecla, y mucho más simple — ver Task 7 Step 3 del plan). Solo se autoguarda el
/// documento activo, solo si tiene ruta real, no es temporal volátil, y no hay ya un
/// conflicto sin resolver.
fn autosave_tick(w: &mut WindowState) {
    if !w.cfg.borrow().files.autosave {
        return;
    }
    if matches!(w.ws.prompt, crate::Prompt::Conflict) {
        return;
    }
    let st = w.ws.active();
    let has_real_path = st.path.is_some() && !matches!(st.temp, Some(notty_config::TempMode::Volatile));
    if has_real_path && st.doc.is_dirty() {
        try_save(w);
    }
    refresh_recovery(w);
}

/// Sondea `w.ipc_rx` (si lo hay) por mensajes del pipe de instancia única y los
/// aplica: `OpenPath` abre ese archivo igual que `Ctrl+O` con una ruta existente.
/// `NewTemp`/`NewPermanent` no llegan por este camino en la instancia normal (los
/// maneja el daemon lanzando `notty.exe --new-temp`/`--new-permanent`, Task 9); si
/// alguno llegara igualmente, no se hace nada. Devuelve `true` si hubo que repintar.
fn ipc_tick(w: &mut WindowState) -> bool {
    let Some(rx) = w.ipc_rx.as_ref() else { return false };
    let mut changed = false;
    while let Ok(msg) = rx.try_recv() {
        if let notty_ipc::Message::OpenPath(p) = msg {
            if !p.is_empty() {
                let path = std::path::PathBuf::from(p);
                if let Ok(opened) = crate::open_as_document(&path) {
                    let cfg = w.cfg.borrow().clone();
                    w.ws.open(maybe_vim(EditorState::from_opened(opened), &cfg));
                    changed = true;
                }
            }
        }
    }
    changed
}

/// Resuelve `Prompt::Conflict`: `M` conserva lo escrito en notty y lo guarda, `D`
/// descarta los cambios locales y recarga lo que hay en disco. Cualquier otra tecla
/// no hace nada (Esc ya se maneja antes, en `handle_prompt_keydown`).
fn handle_conflict_key(w: &mut WindowState, vk: u32) {
    match vk {
        0x4D => {
            // M: el mío.
            let _ = w.ws.active_mut().save();
            if let Some(path) = w.ws.active().path.clone() {
                w.ws.active_mut().open_mtime = notty_io::mtime(&path).ok();
            }
            w.ws.close_prompt();
        }
        0x44 => {
            // D: el del disco.
            if let Some(path) = w.ws.active().path.clone() {
                if let Ok(opened) = crate::open_as_document(&path) {
                    let st = w.ws.active_mut();
                    st.doc = opened.document;
                    st.encoding = opened.encoding;
                    st.eol = opened.eol;
                    st.open_mtime = notty_io::mtime(&path).ok();
                }
            }
            w.ws.close_prompt();
        }
        _ => {}
    }
}

/// Crea el documento con el `EditorState` que toque, y si `cfg.ui.vim_always` está
/// activo, lo arranca ya en modo vim.
fn maybe_vim(mut st: EditorState, cfg: &notty_config::Config) -> EditorState {
    if cfg.ui.vim_always {
        st.vim = Some(crate::VimState::default());
    }
    st
}

/// Nombre legible para un documento en el volcado de recuperación: el nombre de
/// archivo si tiene ruta, o "sin-titulo-N" si no.
fn recovery_name(st: &EditorState, idx: usize) -> String {
    match &st.path {
        Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| format!("doc-{idx}")),
        None => format!("sin-titulo-{idx}"),
    }
}

/// Actualiza el snapshot leído por el `panic hook` con el estado actual de todos los
/// documentos. Se llama tras los manejadores que de verdad pueden ensuciar un
/// documento (edición de texto, autoguardado, abrir/cerrar pestañas).
fn refresh_recovery(w: &WindowState) {
    let snapshot: Vec<(String, String, bool)> = w
        .ws
        .iter()
        .enumerate()
        .map(|(i, st)| {
            let text = st.doc.buffer().slice(0..st.doc.buffer().len_chars());
            (recovery_name(st, i), text, st.doc.is_dirty())
        })
        .collect();
    let mut guard = w.recovery.lock().unwrap_or_else(|e| e.into_inner());
    *guard = snapshot;
}

/// Instala el `panic hook` de recuperación: si el proceso entra en pánico, vuelca a
/// `notty_io::recovery_dir()` el texto de cada documento sucio del último snapshot
/// leído (ver `refresh_recovery`). Se instala una sola vez, al arrancar `run`.
fn install_recovery_hook(snapshot: &'static RecoverySnapshot) {
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("notty: pánico: {info}");
        let entries: Vec<notty_io::RecoveryEntry> = {
            let guard = snapshot.lock().unwrap_or_else(|e| e.into_inner());
            guard
                .iter()
                .filter(|(_, _, dirty)| *dirty)
                .enumerate()
                .map(|(i, (name, text, _))| notty_io::RecoveryEntry { name: format!("{i}_{name}"), text: text.clone() })
                .collect()
        };
        let _ = notty_io::dump_recovery(&notty_io::recovery_dir(), &entries);
    }));
}

/// Abre la ventana principal de notty y bloquea hasta que se cierra.
/// `path` es la ruta pasada por línea de comandos, si la hay; `load` es el resultado
/// de cargar `config.toml` (que puede traer un aviso si el archivo estaba roto).
pub fn run(path: Option<&str>, load: notty_config::LoadResult) -> Result<()> {
    run_with_ipc(path, load, None)
}

/// Igual que `run`, pero además recibe el extremo receptor del pipe de instancia
/// única (Task 8): cada `notty_ipc::Message::OpenPath` que llegue mientras esta
/// ventana vive se abre como si se hubiera pedido con `Ctrl+O`.
pub fn run_with_ipc(
    path: Option<&str>,
    load: notty_config::LoadResult,
    ipc_rx: Option<std::sync::mpsc::Receiver<notty_ipc::Message>>,
) -> Result<()> {
    run_inner(path, load, ipc_rx, None)
}

/// Variante de `run_with_ipc` usada por `notty --new-temp` (Task 9): abre la ventana
/// directamente con un documento temporal (`EditorState::new_temp`) en vez del vacío
/// de siempre. No tiene ruta de línea de comandos que abrir.
pub fn run_with_temp(
    load: notty_config::LoadResult,
    ipc_rx: Option<std::sync::mpsc::Receiver<notty_ipc::Message>>,
    mode: notty_config::TempMode,
    ext: String,
) -> Result<()> {
    run_inner(None, load, ipc_rx, Some((mode, ext)))
}

fn run_inner(
    path: Option<&str>,
    load: notty_config::LoadResult,
    ipc_rx: Option<std::sync::mpsc::Receiver<notty_ipc::Message>>,
    initial_temp: Option<(notty_config::TempMode, String)>,
) -> Result<()> {
    // Snapshot de recuperación: vive el resto del proceso (`Box::leak`) para que el
    // `panic hook`, instalado una sola vez, tenga una dirección `'static` válida.
    let recovery: &'static RecoverySnapshot = Box::leak(Box::new(Mutex::new(Vec::new())));
    install_recovery_hook(recovery);

    let (cfg, broken_msg) = match load {
        notty_config::LoadResult::Loaded(cfg) | notty_config::LoadResult::Missing(cfg) => (cfg, None),
        notty_config::LoadResult::Defaulted(cfg, msg) => (cfg, Some(msg)),
    };

    let title = match (path, &broken_msg) {
        (Some(p), Some(msg)) => format!("config.toml roto: {msg} — {p} · notty"),
        (Some(p), None) => format!("{p} · notty"),
        (None, Some(msg)) => format!("config.toml roto: {msg} · notty"),
        (None, None) => "sin título · notty".to_string(),
    };

    unsafe {
        // Per-monitor v2: cada ventana sigue el DPI del monitor en el que está, sin
        // reescalado borroso. Si ya estaba puesto (p.ej. por el manifiesto), se ignora
        // el error: no es fatal.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let instance = GetModuleHandleW(None)?;
        let class_name = w!("NottyWindowClass");
        let icon = app_icon(instance.into());

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hIcon: icon.unwrap_or_default(),
            hIconSm: icon.unwrap_or_default(),
            // Sin esto Windows no toca el cursor al entrar en la ventana: se queda con
            // el que hubiera antes (a veces uno de arrastre o de redimensionar de otra
            // ventana), lo que se ve como "cursor raro" al seleccionar texto.
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let title_wide = to_wide(&title);
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name,
            PCWSTR(title_wide.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            920,
            600,
            None,
            None,
            Some(instance.into()),
            None,
        )?;

        // La ventana se creó con un tamaño nominal en píxeles; ahora que existe, se
        // conoce su DPI real y se ajusta a 920x600 DIPs exactos.
        let dpi0 = GetDpiForWindow(hwnd);
        let scale0 = dpi0 as f32 / 96.0;
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            (920.0 * scale0).round() as i32,
            (600.0 * scale0).round() as i32,
            SWP_NOMOVE | SWP_NOZORDER,
        );

        let dark = crate::is_dark(cfg.ui.theme, system_uses_dark_mode());
        setup_chrome(hwnd, dark);

        let mut ws = crate::Workspace::new();
        if let Some((mode, ext)) = &initial_temp {
            *ws.active_mut() = maybe_vim(EditorState::new_temp(*mode, ext), &cfg);
        } else if let Some(p) = path {
            let p = std::path::Path::new(p);
            match crate::open_as_document(p) {
                Ok(opened) => *ws.active_mut() = maybe_vim(EditorState::from_opened(opened), &cfg),
                Err(_) => {
                    // No es texto (o no se pudo decodificar): se abre directamente en vista
                    // raw, como pide la Task 7 de este plan.
                    let mut state = EditorState::new_empty();
                    state.path = Some(p.to_path_buf());
                    if let Ok(raw) = crate::open_raw_doc(p) {
                        state.raw = Some(raw);
                    }
                    *ws.active_mut() = maybe_vim(state, &cfg);
                }
            }
        }

        // Recuperación tras una caída anterior: se ofrecen como documentos nuevos con
        // ruta CLICKME (ninguno es "el archivo original" — solo se volcó nombre+texto,
        // no la ruta —, así que el usuario decide dónde guardarlos, como con cualquier
        // documento nuevo). Se borra el volcado en cuanto se han recuperado.
        let recovered = notty_io::list_recovery(&notty_io::recovery_dir());
        if !recovered.is_empty() {
            for entry in &recovered {
                let mut st = EditorState::new_empty();
                st.doc = notty_core::Document::new(&entry.text, "\r\n");
                ws.open(st);
            }
            let _ = notty_io::clear_recovery(&notty_io::recovery_dir());
        }

        let dpi = GetDpiForWindow(hwnd);
        let renderer = Renderer::new(hwnd, dpi)?;

        let total_lines = ws.active().doc.buffer().len_lines();
        let menu_visible0 = cfg.ui.menubar == notty_config::MenuBar::Visible;
        let (body, _gutter_w) = renderer.body_and_gutter(&cfg.ui, ws.len(), menu_visible0, total_lines, ws.active().raw.is_some());
        ws.active_mut().viewport = Viewport::new(renderer.line_height(), body.height());
        update_title(hwnd, ws.active());

        let ui_keymap = {
            let mut m = notty_input::default_ui_keymap();
            notty_input::apply_overrides(&mut m, &cfg);
            m
        };
        let cfg = Rc::new(RefCell::new(cfg));

        let window_state = Box::new(WindowState {
            ws,
            renderer,
            mouse_down: false,
            selection_anchor: 0,
            cfg,
            ui_keymap,
            menu_visible: false,
            hover: Hit::None,
            pressed: Hit::None,
            active_window: true,
            open_menu: None,
            recovery,
            ipc_rx,
            animations_enabled: system_animations_enabled(),
            popup_open_anim: None,
            tab_switch_anim: None,
            anim_timer_running: false,
        });
        let ptr = Box::into_raw(window_state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
        if let Some(w) = ptr.as_ref() {
            refresh_recovery(w);
        }

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetTimer(Some(hwnd), ID_AUTOSAVE_TIMER, 1000, None);
        let _ = SetTimer(Some(hwnd), ID_IPC_TIMER, 150, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        drop(Box::from_raw(ptr));
    }
    Ok(())
}

/// Prepara la ventana para dibujar su propia barra de título: quita la nativa (con
/// `WM_NCCALCSIZE`, ver `wndproc`) pero deja que DWM siga dibujando sombra y esquinas
/// redondeadas (`DwmExtendFrameIntoClientArea` con un margen de 1 px arriba). Sin Mica:
/// la maqueta usa colores sólidos. Si `DwmSetWindowAttribute` falla (Windows más viejo
/// que 11), la ventana sigue funcionando con el aspecto por defecto: no es fatal.
unsafe fn setup_chrome(hwnd: HWND, dark: bool) {
    unsafe {
        apply_dark_mode(hwnd, dark);

        let prefer_round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &prefer_round as *const _ as *const _,
            std::mem::size_of_val(&prefer_round) as u32,
        );

        let margins = MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 1, cyBottomHeight: 0 };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        // Fuerza a que WM_NCCALCSIZE se vuelva a evaluar ya sin la barra nativa.
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        );
    }
}

/// `DWMWA_USE_IMMERSIVE_DARK_MODE`: oscurece el marco nativo (los 4 px de borde que
/// sigue dibujando DWM). Se vuelve a llamar cuando cambia el tema (Task 10).
pub unsafe fn apply_dark_mode(hwnd: HWND, dark: bool) {
    unsafe {
        let value: i32 = if dark { 1 } else { 0 };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &value as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        );
    }
}

/// Lee `HKCU\...\Personalize\AppsUseLightTheme`. Si no se puede leer, asume modo claro.
pub fn system_uses_dark_mode() -> bool {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    unsafe {
        let subkey = w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        let value = w!("AppsUseLightTheme");
        let mut data: u32 = 1;
        let mut size = std::mem::size_of::<u32>() as u32;
        let ok = RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut _ as *mut _),
            Some(&mut size),
        );
        ok.is_ok() && data == 0
    }
}

/// Si el usuario desactivó las animaciones del sistema (Accesibilidad → Efectos
/// visuales), las de notty también se saltan. Se consulta una vez al arrancar la
/// ventana y se guarda en `WindowState`; no hace falta escuchar cambios en caliente.
pub fn system_animations_enabled() -> bool {
    unsafe {
        let mut enabled = windows::core::BOOL(1);
        let ok = windows::Win32::UI::WindowsAndMessaging::SystemParametersInfoW(
            windows::Win32::UI::WindowsAndMessaging::SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut enabled as *mut _ as *mut core::ffi::c_void),
            Default::default(),
        );
        ok.is_err() || enabled.as_bool()
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        match msg {
            WM_PAINT => {
                if let Some(w) = ptr.as_mut() {
                    let ui = w.render_ui();
                    let view = w.view_state(hwnd);
                    w.renderer.paint(&w.ws, &ui, &view);
                }
                let _ = ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }
            WM_SIZE => {
                if let Some(w) = ptr.as_mut() {
                    let width = (lparam.0 as u32) & 0xFFFF;
                    let height = ((lparam.0 as u32) >> 16) & 0xFFFF;
                    w.renderer.resize(width, height);
                    let (body, _gutter_w) = w.body_and_gutter();
                    w.ws.active_mut().viewport.visible_lines = layout::visible_lines(body);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_NCCALCSIZE if wparam.0 != 0 => {
                // Quita la barra de título nativa pero conserva los bordes de
                // redimensionar de los lados y de abajo (técnica de Windows Terminal).
                let params = &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS);
                let original_top = params.rgrc[0].top;
                let r = DefWindowProcW(hwnd, msg, wparam, lparam);
                params.rgrc[0].top = original_top;
                if IsZoomed(hwnd).as_bool() {
                    // Maximizada, Windows la saca unos px por arriba: se recuperan.
                    let dpi = GetDpiForWindow(hwnd);
                    params.rgrc[0].top +=
                        GetSystemMetricsForDpi(SM_CYFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
                }
                r
            }
            WM_NCHITTEST => {
                let def = DefWindowProcW(hwnd, msg, wparam, lparam);
                if def.0 as u32 != HTCLIENT {
                    return def;
                }
                if let Some(w) = ptr.as_mut() {
                    let x = (lparam.0 as i16) as i32;
                    let y = ((lparam.0 >> 16) as i16) as i32;
                    let mut pt = windows::Win32::Foundation::POINT { x, y };
                    let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
                    let scale = w.renderer.scale();
                    let (x_dip, y_dip) = (pt.x as f32 / scale, pt.y as f32 / scale);

                    if !IsZoomed(hwnd).as_bool() {
                        let dpi = GetDpiForWindow(hwnd);
                        let border = (GetSystemMetricsForDpi(SM_CYFRAME, dpi)
                            + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)) as f32;
                        if (pt.y as f32) < border {
                            return LRESULT(HTTOP as isize);
                        }
                    }
                    match w.renderer.hit(x_dip, y_dip) {
                        Hit::Max => LRESULT(HTMAXBUTTON as isize),
                        Hit::Caption => LRESULT(HTCAPTION as isize),
                        _ => LRESULT(HTCLIENT as isize),
                    }
                } else {
                    LRESULT(HTCLIENT as isize)
                }
            }
            WM_NCLBUTTONDOWN => {
                let x = (lparam.0 as i16) as i32;
                let y = ((lparam.0 >> 16) as i16) as i32;
                let mut pt = windows::Win32::Foundation::POINT { x, y };
                let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
                if let Some(w) = ptr.as_mut() {
                    let scale = w.renderer.scale();
                    if w.renderer.hit(pt.x as f32 / scale, pt.y as f32 / scale) == Hit::Max {
                        w.pressed = Hit::Max;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_NCLBUTTONUP => {
                let x = (lparam.0 as i16) as i32;
                let y = ((lparam.0 >> 16) as i16) as i32;
                let mut pt = windows::Win32::Foundation::POINT { x, y };
                let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
                if let Some(w) = ptr.as_mut() {
                    let scale = w.renderer.scale();
                    let was_pressed = w.pressed == Hit::Max;
                    w.pressed = Hit::None;
                    if was_pressed && w.renderer.hit(pt.x as f32 / scale, pt.y as f32 / scale) == Hit::Max {
                        let _ = ShowWindow(hwnd, if IsZoomed(hwnd).as_bool() { SW_RESTORE } else { SW_MAXIMIZE });
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_NCMOUSEMOVE => {
                if let Some(w) = ptr.as_mut() {
                    let x = (lparam.0 as i16) as i32;
                    let y = ((lparam.0 >> 16) as i16) as i32;
                    let mut pt = windows::Win32::Foundation::POINT { x, y };
                    let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
                    let scale = w.renderer.scale();
                    let hit = w.renderer.hit(pt.x as f32 / scale, pt.y as f32 / scale);
                    if hit != w.hover {
                        w.hover = hit;
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE | TME_NONCLIENT,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        let _ = TrackMouseEvent(&mut tme);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_NCMOUSELEAVE => {
                if let Some(w) = ptr.as_mut() {
                    if w.hover != Hit::None {
                        w.hover = Hit::None;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_MOUSELEAVE => {
                if let Some(w) = ptr.as_mut() {
                    if w.hover != Hit::None {
                        w.hover = Hit::None;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_ACTIVATE => {
                if let Some(w) = ptr.as_mut() {
                    w.active_window = (wparam.0 & 0xFFFF) != 0;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_DPICHANGED => {
                if let Some(w) = ptr.as_mut() {
                    let new_dpi = (wparam.0 & 0xFFFF) as u32;
                    w.renderer.set_dpi(new_dpi);
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
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_SETTINGCHANGE => {
                // "ImmersiveColorSet": el usuario cambió el tema claro/oscuro de Windows
                // mientras notty (en `Theme::System`) seguía abierto.
                if let Some(w) = ptr.as_mut() {
                    let dark = crate::is_dark(w.cfg.borrow().ui.theme, system_uses_dark_mode());
                    apply_dark_mode(hwnd, dark);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KEYDOWN => {
                if let Some(w) = ptr.as_mut() {
                    let vk = wparam.0 as u32;
                    let alt_down = (GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0;
                    let mods = Modifiers {
                        ctrl: (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0,
                        shift: (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0,
                        alt: alt_down,
                    };

                    if w.open_menu.is_some() && vk == 0x1B {
                        w.open_menu = None;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }

                    if !matches!(w.ws.prompt, crate::Prompt::None) {
                        handle_prompt_keydown(w, hwnd, vk, mods);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }

                    let ui_mods = notty_input::Modifiers { ctrl: mods.ctrl, shift: mods.shift, alt: alt_down };
                    if let Some(cmd) = w.ui_keymap.get(&(vk, ui_mods)).copied() {
                        match cmd {
                            notty_input::UiCommand::NewTab => {
                                let cfg = w.cfg.borrow().clone();
                                w.ws.open(maybe_vim(crate::EditorState::new_empty(), &cfg));
                            }
                            notty_input::UiCommand::NewTempTab => {
                                let cfg = w.cfg.borrow().clone();
                                w.ws.open(maybe_vim(
                                    crate::EditorState::new_temp(cfg.files.temp_mode, &cfg.files.default_extension),
                                    &cfg,
                                ));
                            }
                            notty_input::UiCommand::NextTab => {
                                w.ws.next();
                                start_tab_switch_anim(w, hwnd);
                            }
                            notty_input::UiCommand::PrevTab => {
                                w.ws.prev();
                                start_tab_switch_anim(w, hwnd);
                            }
                            notty_input::UiCommand::CloseTab => {
                                w.ws.close_active();
                            }
                            notty_input::UiCommand::OpenSettings => open_settings(w, hwnd),
                        }
                        update_title(hwnd, w.ws.active());
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    let action = crate::action_for_vk(vk, mods);

                    // ToggleVim/ToggleRaw funcionan siempre, esté vim/raw activo o no.
                    if matches!(action, crate::EditorAction::ToggleVim) {
                        let st = w.ws.active_mut();
                        st.vim = if st.vim.is_some() { None } else { Some(crate::VimState::default()) };
                        update_title(hwnd, w.ws.active());
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if matches!(action, crate::EditorAction::ToggleRaw) {
                        toggle_raw(w);
                        update_title(hwnd, w.ws.active());
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }

                    if w.ws.active().raw.is_some() {
                        handle_raw_keydown(w, vk, action);
                        update_title(hwnd, w.ws.active());
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }

                    if w.ws.active().vim.is_some() {
                        // Solo Esc se enruta aquí explícitamente (ver nota de la Task 6 del
                        // plan: el resto del movimiento vim llega como texto por WM_CHAR).
                        // El resto de teclas (flechas, Ctrl+S, ...) caen al camino normal de
                        // abajo como "vía de escape" además de sus equivalentes propios de vim.
                        if vk == 0x1B {
                            let st = w.ws.active_mut();
                            let mut vim = st.vim.take().unwrap();
                            let _ = vim.handle_key(&mut st.doc, vk, None, std::time::Instant::now());
                            st.vim = Some(vim);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                    }

                    match action {
                        crate::EditorAction::None => {}
                        crate::EditorAction::Copy | crate::EditorAction::Cut => {
                            let sel = w.ws.active_mut().doc.selection();
                            if !sel.is_empty() {
                                let text = w.ws.active_mut().doc.buffer().slice(sel.range());
                                let _ = crate::clipboard::set_clipboard_text(hwnd, &text);
                                if matches!(action, crate::EditorAction::Cut) {
                                    w.ws.active_mut().doc.backspace(std::time::Instant::now());
                                }
                            }
                            update_title(hwnd, w.ws.active());
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::EditorAction::Paste => {
                            if let Ok(text) = crate::clipboard::get_clipboard_text(hwnd) {
                                if !text.is_empty() {
                                    w.ws.active_mut().doc.insert(&text, std::time::Instant::now());
                                }
                            }
                            update_title(hwnd, w.ws.active());
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::EditorAction::OpenPathPrompt => {
                            let initial = w.ws.active().path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                            w.ws.prompt = crate::Prompt::Path(crate::PathPromptState::new(crate::Purpose::Open, initial));
                            start_popup_anim(w, hwnd);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::EditorAction::Find => {
                            w.ws.prompt = crate::Prompt::Find(crate::SearchState::default());
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::EditorAction::Replace => {
                            w.ws.prompt = crate::Prompt::Replace(crate::SearchState::default());
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::EditorAction::Save if w.ws.active().path.is_none() => {
                            w.ws.prompt = crate::Prompt::Path(crate::PathPromptState::new(crate::Purpose::Save, String::new()));
                            start_popup_anim(w, hwnd);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::EditorAction::FindNext | crate::EditorAction::FindPrev => {
                            // Fuera de un prompt de búsqueda activo (ya cubierto arriba, antes de
                            // llegar aquí), F3 no tiene una búsqueda que repetir: no hace nada.
                        }
                        crate::EditorAction::Save => {
                            try_save(w);
                            update_title(hwnd, w.ws.active());
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        other => {
                            w.ws.active_mut().apply(other, std::time::Instant::now());
                            update_title(hwnd, w.ws.active());
                            refresh_recovery(w);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                    }
                }
                LRESULT(0)
            }
            WM_CHAR => {
                if let Some(w) = ptr.as_mut() {
                    if let Some(ch) = char::from_u32(wparam.0 as u32) {
                        if !matches!(w.ws.prompt, crate::Prompt::None) {
                            handle_prompt_char(w, ch);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                        if w.ws.active().raw.is_some() {
                            handle_raw_char(w, ch);
                            update_title(hwnd, w.ws.active());
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                        if w.ws.active().vim.is_some() {
                            handle_vim_char(w, ch);
                            update_title(hwnd, w.ws.active());
                            refresh_recovery(w);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                        w.ws.active_mut().insert_char(ch, std::time::Instant::now());
                        update_title(hwnd, w.ws.active());
                        refresh_recovery(w);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                if let Some(w) = ptr.as_mut() {
                    let (x, y) = point_from_lparam(lparam);
                    let scale = w.renderer.scale();
                    let (x, y) = (x / scale, y / scale);
                    let hit = w.renderer.hit(x, y);
                    match hit {
                        Hit::Min | Hit::Close => {
                            w.pressed = hit;
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::Pencil if w.ws.active().raw.is_some() => {
                            if let Some(raw) = w.ws.active_mut().raw.as_mut() {
                                raw.enable_write();
                            }
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::Clickme => {
                            w.ws.prompt = crate::Prompt::Path(crate::PathPromptState::new(crate::Purpose::Save, String::new()));
                            start_popup_anim(w, hwnd);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::Settings => open_settings(w, hwnd),
                        crate::Hit::SearchOpt(k) => {
                            if let crate::Prompt::Find(s) | crate::Prompt::Replace(s) = &mut w.ws.prompt {
                                match k {
                                    0 => s.toggle_case(),
                                    1 => s.toggle_word(),
                                    _ => s.toggle_regex(),
                                }
                            }
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::SearchField(k) => {
                            if let crate::Prompt::Replace(s) = &mut w.ws.prompt {
                                s.editing_replacement = k == 1;
                            }
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::Suggestion(i) => {
                            if let crate::Prompt::Path(p) = &mut w.ws.prompt {
                                p.selected = i;
                                p.accept();
                            }
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::Tab(i) => {
                            w.ws.activate(i);
                            start_tab_switch_anim(w, hwnd);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::TabClose(i) => {
                            w.ws.close(i);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::NewTab => {
                            let cfg = w.cfg.borrow().clone();
                            w.ws.open(maybe_vim(crate::EditorState::new_empty(), &cfg));
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::Menu(i) => {
                            w.open_menu = if w.open_menu == Some(i) { None } else { Some(i) };
                            if w.open_menu.is_some() {
                                start_popup_anim(w, hwnd);
                            }
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::MenuItem(j) => {
                            if let Some(i) = w.open_menu.take() {
                                run_menu_item(w, hwnd, i, j);
                            }
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::Body if w.ws.active().raw.is_none() => {
                            w.open_menu = None;
                            let (body, gutter_w) = w.body_and_gutter();
                            let idx = w.renderer.char_index_at(w.ws.active(), body, gutter_w, x, y);
                            w.ws.active_mut().doc.set_cursor(idx);
                            w.selection_anchor = idx;
                            w.mouse_down = true;
                            SetCapture(hwnd);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        _ => {
                            if w.open_menu.is_some() {
                                w.open_menu = None;
                                let _ = InvalidateRect(Some(hwnd), None, false);
                            }
                        }
                    }
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if let Some(w) = ptr.as_mut() {
                    let (x, y) = point_from_lparam(lparam);
                    let scale = w.renderer.scale();
                    let (x, y) = (x / scale, y / scale);
                    if w.mouse_down {
                        let (body, gutter_w) = w.body_and_gutter();
                        let idx = w.renderer.char_index_at(w.ws.active(), body, gutter_w, x, y);
                        w.ws.active_mut().doc.set_selection(w.selection_anchor, idx);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    let hit = w.renderer.hit(x, y);
                    if hit != w.hover {
                        w.hover = hit;
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        let _ = TrackMouseEvent(&mut tme);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if let Some(w) = ptr.as_mut() {
                    w.mouse_down = false;
                    let (x, y) = point_from_lparam(lparam);
                    let scale = w.renderer.scale();
                    let (x, y) = (x / scale, y / scale);
                    let still_over = w.renderer.hit(x, y);
                    let pressed = w.pressed;
                    w.pressed = Hit::None;
                    match (pressed, still_over) {
                        (Hit::Min, Hit::Min) => {
                            let _ = ShowWindow(hwnd, SW_MINIMIZE);
                        }
                        (Hit::Close, Hit::Close) => {
                            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                                Some(hwnd),
                                windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                                WPARAM(0),
                                LPARAM(0),
                            );
                        }
                        _ => {}
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                let _ = ReleaseCapture();
                LRESULT(0)
            }
            WM_SETCURSOR => {
                // El hit-test de `WM_NCHITTEST` ya decide bordes/barra de título; aquí solo
                // hace falta el I-beam sobre el documento (`Hit::Body`), y la flecha en el
                // resto del cliente propio (pestañas, barras, prompts) en vez de lo que sea
                // que el cursor tuviera antes de entrar en la ventana.
                if (lparam.0 as u32) & 0xFFFF == HTCLIENT {
                    if let Some(w) = ptr.as_ref() {
                        let over_text = w.mouse_down || w.hover == Hit::Body;
                        let id = if over_text { IDC_IBEAM } else { IDC_ARROW };
                        if let Ok(cursor) = LoadCursorW(None, id) {
                            SetCursor(Some(cursor));
                        }
                        return LRESULT(1);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_MOUSEWHEEL => {
                if let Some(w) = ptr.as_mut() {
                    let delta = ((wparam.0 >> 16) as i16) as i32;
                    let notches = delta / WHEEL_DELTA as i32;
                    if let crate::Prompt::Path(p) = &mut w.ws.prompt {
                        p.scroll_by(-notches);
                    } else {
                        w.ws.active_mut().scroll_by(-notches * 3);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_SYSKEYDOWN => {
                if let Some(w) = ptr.as_mut() {
                    let vk = wparam.0 as u32;
                    if vk == VK_MENU.0 as u32 && w.cfg.borrow().ui.menubar == notty_config::MenuBar::Alt {
                        w.menu_visible = !w.menu_visible;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_TIMER => {
                if wparam.0 == ID_AUTOSAVE_TIMER {
                    if let Some(w) = ptr.as_mut() {
                        autosave_tick(w);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    return LRESULT(0);
                }
                if wparam.0 == ID_IPC_TIMER {
                    if let Some(w) = ptr.as_mut() {
                        if ipc_tick(w) {
                            update_title(hwnd, w.ws.active());
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                    }
                    return LRESULT(0);
                }
                if wparam.0 == ID_ANIM_TIMER {
                    if let Some(w) = ptr.as_mut() {
                        let now = std::time::Instant::now();
                        if w.popup_open_anim.is_some_and(|a| a.is_done(now)) {
                            w.popup_open_anim = None;
                        }
                        if w.tab_switch_anim.is_some_and(|a| a.is_done(now)) {
                            w.tab_switch_anim = None;
                        }
                        let still_animating = w.popup_open_anim.is_some() || w.tab_switch_anim.is_some();
                        if !still_animating {
                            let _ = KillTimer(Some(hwnd), ID_ANIM_TIMER);
                            w.anim_timer_running = false;
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_DESTROY => {
                let _ = KillTimer(Some(hwnd), ID_AUTOSAVE_TIMER);
                let _ = KillTimer(Some(hwnd), ID_IPC_TIMER);
                let _ = KillTimer(Some(hwnd), ID_ANIM_TIMER);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

// --- Modo vim: WM_CHAR (letras) -----------------------------------------------------

fn handle_vim_char(w: &mut WindowState, ch: char) {
    let was_insert = w.ws.active().vim.as_ref().is_some_and(|v| v.mode == crate::VimMode::Insert);
    let outcome = {
        let st = w.ws.active_mut();
        let mut vim = st.vim.take().unwrap();
        let out = vim.handle_key(&mut st.doc, 0, Some(ch), std::time::Instant::now());
        st.vim = Some(vim);
        out
    };
    match outcome {
        crate::VimOutcome::Handled => {}
        crate::VimOutcome::OpenFind => w.ws.prompt = crate::Prompt::Find(crate::SearchState::default()),
        crate::VimOutcome::OpenCmdline => w.ws.prompt = crate::Prompt::VimCmdline(String::new()),
        // `Bubble` solo puede llegar de un modo Insert que rechazó un carácter de
        // control (ver `VimState::handle_insert`): ahí sí se trata como texto normal.
        // Una tecla sin mapear en Normal/Visual también da `Bubble`, pero vim de
        // verdad la ignora en vez de escribirla — si no, cualquier letra que vim no
        // reconozca (q, w, e, p, n...) se colaba en el documento mientras la barra
        // seguía diciendo "-- NORMAL --".
        crate::VimOutcome::Bubble => {
            if was_insert && !ch.is_control() {
                w.ws.active_mut().insert_char(ch, std::time::Instant::now());
            }
        }
    }
}

// --- Menús desplegables ---------------------------------------------------------------

/// Ejecuta el comando del elemento `item_idx` del menú `menu_idx` (`crate::menu::MENUS`),
/// mapeado a lo que ya existe en la app (ver Task 9 del plan). Los comandos que
/// todavía no tienen nada detrás (`NewTemp`, `Shortcuts`, `About`) no hacen nada.
fn run_menu_item(w: &mut WindowState, hwnd: HWND, menu_idx: usize, item_idx: usize) {
    use crate::menu::{MenuCmd, MenuItem};
    let Some(def) = crate::menu::MENUS.get(menu_idx) else { return };
    let Some(MenuItem::Entry { cmd, .. }) = def.items.get(item_idx).copied() else { return };
    match cmd {
        MenuCmd::New => {
            let cfg = w.cfg.borrow().clone();
            w.ws.open(maybe_vim(crate::EditorState::new_empty(), &cfg));
        }
        MenuCmd::Open => {
            let initial = w.ws.active().path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
            w.ws.prompt = crate::Prompt::Path(crate::PathPromptState::new(crate::Purpose::Open, initial));
            start_popup_anim(w, hwnd);
        }
        MenuCmd::Save => {
            if w.ws.active().path.is_none() {
                w.ws.prompt = crate::Prompt::Path(crate::PathPromptState::new(crate::Purpose::Save, String::new()));
                start_popup_anim(w, hwnd);
            } else {
                try_save(w);
            }
        }
        MenuCmd::SaveAs => {
            w.ws.prompt = crate::Prompt::Path(crate::PathPromptState::new(crate::Purpose::Save, String::new()));
            start_popup_anim(w, hwnd);
        }
        MenuCmd::Settings => open_settings(w, hwnd),
        MenuCmd::CloseTab => {
            w.ws.close_active();
        }
        MenuCmd::Undo => {
            w.ws.active_mut().doc.undo();
        }
        MenuCmd::Redo => {
            w.ws.active_mut().doc.redo();
        }
        MenuCmd::Find => w.ws.prompt = crate::Prompt::Find(crate::SearchState::default()),
        MenuCmd::Replace => w.ws.prompt = crate::Prompt::Replace(crate::SearchState::default()),
        MenuCmd::FindNext => nav_search(w, true),
        MenuCmd::FindPrev => nav_search(w, false),
        MenuCmd::ToggleVim => {
            let st = w.ws.active_mut();
            st.vim = if st.vim.is_some() { None } else { Some(crate::VimState::default()) };
        }
        MenuCmd::ToggleRaw => toggle_raw(w),
        MenuCmd::ToggleLineNumbers => {
            let mut cfg = w.cfg.borrow_mut();
            cfg.ui.line_numbers = !cfg.ui.line_numbers;
            let _ = notty_config::save(&cfg, &notty_config::default_path());
        }
        MenuCmd::ToggleHintsBar => {
            let mut cfg = w.cfg.borrow_mut();
            cfg.ui.hints_bar = !cfg.ui.hints_bar;
            let _ = notty_config::save(&cfg, &notty_config::default_path());
        }
        MenuCmd::NewTemp => {
            let cfg = w.cfg.borrow().clone();
            w.ws.open(maybe_vim(crate::EditorState::new_temp(cfg.files.temp_mode, &cfg.files.default_extension), &cfg));
        }
        MenuCmd::Shortcuts | MenuCmd::About => {}
    }
    unsafe {
        update_title(hwnd, w.ws.active());
    }
}

// --- Vista raw: teclado --------------------------------------------------------------

/// Cambia entre la vista de texto normal y la vista raw (`Ctrl+Shift+H`). Si el
/// contenido de la vista raw sigue siendo UTF-8 válido al volver a texto, se reconstruye
/// el `Document`; si no, se queda en raw (no hay forma segura de mostrarlo como texto).
fn toggle_raw(w: &mut WindowState) {
    let st = w.ws.active_mut();
    if let Some(raw) = st.raw.take() {
        let bytes: Vec<u8> = (0..raw.len()).map(|i| raw.byte(i)).collect();
        if let Ok(text) = String::from_utf8(bytes) {
            st.doc = notty_core::Document::new(&text, st.eol.as_str());
        } else {
            st.raw = Some(raw);
        }
    } else if let Some(path) = st.path.clone() {
        if let Ok(raw) = crate::open_raw_doc(&path) {
            st.raw = Some(raw);
            st.raw_cursor = 0;
            st.raw_pending_nibble = None;
        }
    }
}

/// Flechas (mueven el byte seleccionado) y `Ctrl+S` (guarda) mientras hay un `RawDoc` activo.
fn handle_raw_keydown(w: &mut WindowState, vk: u32, action: crate::EditorAction) {
    let len = w.ws.active().raw.as_ref().map(|r| r.len()).unwrap_or(0);
    let delta: i64 = match vk {
        0x25 => -1, // Left
        0x27 => 1,  // Right
        0x26 => -16, // Up
        0x28 => 16,  // Down
        _ => 0,
    };
    if delta != 0 {
        if len == 0 {
            return;
        }
        let cur = w.ws.active().raw_cursor as i64;
        w.ws.active_mut().raw_cursor = (cur + delta).clamp(0, len as i64 - 1) as usize;
        w.ws.active_mut().raw_pending_nibble = None;
        return;
    }
    if matches!(action, crate::EditorAction::Save) {
        if let Some(raw) = w.ws.active_mut().raw.as_mut() {
            let _ = raw.save();
        }
    }
}

/// Dígitos hexadecimales tecleados mientras hay un `RawDoc` activo: la primera pulsación
/// guarda el nibble alto, la segunda completa el byte y avanza la selección.
fn handle_raw_char(w: &mut WindowState, ch: char) {
    let Some(digit) = crate::hex_char(ch) else { return };
    let is_editing = w.ws.active().raw.as_ref().is_some_and(|r| r.is_editing());
    if !is_editing {
        // Solo lectura (o sin permiso de escritura hasta pulsar el lápiz): no hace nada.
        return;
    }
    let idx = w.ws.active().raw_cursor;
    match w.ws.active().raw_pending_nibble {
        None => {
            w.ws.active_mut().raw_pending_nibble = Some(digit);
        }
        Some(hi) => {
            let value = (hi << 4) | digit;
            if let Some(raw) = w.ws.active_mut().raw.as_mut() {
                raw.set_byte(idx, value);
            }
            w.ws.active_mut().raw_pending_nibble = None;
            let len = w.ws.active().raw.as_ref().map(|r| r.len()).unwrap_or(0);
            if len > 0 {
                w.ws.active_mut().raw_cursor = (idx + 1).min(len - 1);
            }
        }
    }
}

// --- Prompts (línea de ruta / buscar / reemplazar / comandos vim): teclado --------

/// Contexto de rutas del documento activo: `~` es el perfil del usuario y `.` es la
/// carpeta del archivo abierto (si lo hay).
fn path_ctx(w: &WindowState) -> notty_io::PathContext {
    notty_io::PathContext {
        home: notty_io::home_dir(),
        current_dir: w.ws.active().path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf())),
    }
}

fn handle_prompt_keydown(w: &mut WindowState, hwnd: HWND, vk: u32, mods: Modifiers) {
    // Esc cierra cualquier prompt.
    if vk == 0x1B {
        w.ws.close_prompt();
        return;
    }
    if matches!(w.ws.prompt, crate::Prompt::Path(_)) {
        handle_path_key(w, hwnd, vk, mods);
    } else if matches!(w.ws.prompt, crate::Prompt::Find(_) | crate::Prompt::Replace(_)) {
        handle_search_key(w, vk, mods);
    } else if matches!(w.ws.prompt, crate::Prompt::VimCmdline(_)) {
        handle_vim_cmdline_key(w, hwnd, vk);
    } else if matches!(w.ws.prompt, crate::Prompt::Conflict) {
        handle_conflict_key(w, vk);
    }
}

fn handle_prompt_char(w: &mut WindowState, ch: char) {
    if ch.is_control() {
        return;
    }
    if matches!(w.ws.prompt, crate::Prompt::Path(_)) {
        handle_path_char(w, ch);
    } else if matches!(w.ws.prompt, crate::Prompt::VimCmdline(_)) {
        if let crate::Prompt::VimCmdline(line) = &mut w.ws.prompt {
            line.push(ch);
        }
    } else if matches!(w.ws.prompt, crate::Prompt::Find(_) | crate::Prompt::Replace(_)) {
        handle_search_char(w, ch);
    }
}

fn handle_path_char(w: &mut WindowState, ch: char) {
    let ctx = path_ctx(w);
    if let crate::Prompt::Path(p) = &mut w.ws.prompt {
        let raw = format!("{}{ch}", p.value);
        p.type_text(&raw, &ctx);
    }
}

fn handle_path_backspace(w: &mut WindowState) {
    let ctx = path_ctx(w);
    if let crate::Prompt::Path(p) = &mut w.ws.prompt {
        let mut raw = p.value.clone();
        raw.pop();
        p.type_text(&raw, &ctx);
    }
}

fn handle_path_key(w: &mut WindowState, hwnd: HWND, vk: u32, mods: Modifiers) {
    match vk {
        0x08 => handle_path_backspace(w), // Backspace
        0x09 => {
            if let crate::Prompt::Path(p) = &mut w.ws.prompt {
                p.accept();
            }
        } // Tab
        0x26 => {
            if let crate::Prompt::Path(p) = &mut w.ws.prompt {
                p.move_selection(-1);
            }
        } // ArrowUp
        0x28 => {
            if let crate::Prompt::Path(p) = &mut w.ws.prompt {
                p.move_selection(1);
            }
        } // ArrowDown
        0x0D => commit_path_prompt(w, hwnd), // Enter
        0x4F if mods.ctrl => open_native_dialog(w, hwnd),
        _ => {}
    }
}

/// `Ctrl+O` dentro de la línea de ruta: diálogo nativo de Windows en vez de escribir
/// la ruta a mano. Si el usuario elige algo, se rellena el prompt y se acepta al
/// instante, como si lo hubiera escrito y pulsado Enter.
fn open_native_dialog(w: &mut WindowState, hwnd: HWND) {
    let purpose = match &w.ws.prompt {
        crate::Prompt::Path(p) => p.purpose,
        _ => return,
    };
    let Some(path) = crate::native_dialog::pick_path(hwnd, purpose) else { return };
    let Some(value) = path.to_str() else { return };
    let ctx = path_ctx(w);
    if let crate::Prompt::Path(p) = &mut w.ws.prompt {
        p.type_text(value, &ctx);
    }
    commit_path_prompt(w, hwnd);
}

/// `Enter` sobre la línea de ruta: valida, crea carpetas que falten si hace falta y,
/// según `Purpose`, abre o guarda. Ver Task 7 Step 3 del plan para el detalle de cada caso.
fn commit_path_prompt(w: &mut WindowState, hwnd: HWND) {
    let (value, purpose, invalid) = match &w.ws.prompt {
        crate::Prompt::Path(p) => (p.value.clone(), p.purpose, p.is_invalid()),
        _ => return,
    };
    if invalid || value.is_empty() {
        return;
    }
    if value.ends_with('\\') {
        // Carpeta que se acaba de aceptar (p.ej. con Tab): seguimos escribiendo dentro,
        // no cerramos el prompt.
        return;
    }

    let path = std::path::PathBuf::from(&value);
    let hint = notty_io::hint_for(&value);
    let mut done = false;
    // Nunca se cierra el prompt sin decir por qué si algo falla (permisos, disco
    // lleno, carpeta que no se pudo crear...): antes se descartaba el error con
    // `let _ =` y el usuario se quedaba mirando un Enter que no hacía nada.
    let mut error: Option<String> = None;

    match hint {
        notty_io::Hint::Empty => {}
        notty_io::Hint::Dir => {
            // No tiene sentido "abrir" ni "guardar" una carpeta: no hacer nada.
        }
        notty_io::Hint::Exists if purpose == crate::Purpose::Open => match crate::open_as_document(&path) {
            Ok(opened) => {
                let cfg = w.cfg.borrow().clone();
                w.ws.open(maybe_vim(crate::EditorState::from_opened(opened), &cfg));
                done = true;
            }
            Err(e) => error = Some(e.to_string()),
        },
        notty_io::Hint::Exists | notty_io::Hint::New | notty_io::Hint::DirNew => {
            match notty_io::create_parent_dirs(&path) {
                Ok(_) => match purpose {
                    crate::Purpose::Open => {
                        // Caso raro: se pidió "abrir" algo que no existe. Se trata como crear
                        // un documento nuevo con esa ruta.
                        let mut state = crate::EditorState::new_empty();
                        state.path = Some(path.clone());
                        let cfg = w.cfg.borrow().clone();
                        w.ws.open(maybe_vim(state, &cfg));
                        done = true;
                    }
                    crate::Purpose::Save => {
                        // Si save() falla, se deshace el cambio de `path`: el documento no
                        // se queda apuntando en silencio a una ruta que no se pudo escribir.
                        let previous_path = w.ws.active().path.clone();
                        w.ws.active_mut().path = Some(path.clone());
                        match w.ws.active_mut().save() {
                            Ok(()) => done = true,
                            Err(e) => {
                                w.ws.active_mut().path = previous_path;
                                error = Some(e.to_string());
                            }
                        }
                    }
                },
                Err(e) => error = Some(e.to_string()),
            }
        }
    }

    if done {
        w.ws.close_prompt();
        unsafe {
            update_title(hwnd, w.ws.active());
        }
    } else if let (Some(msg), crate::Prompt::Path(p)) = (error, &mut w.ws.prompt) {
        p.last_error = Some(msg);
    }
}

// --- Prompt de línea de comandos vim (`:w`, `:q`, `:%s/a/b/g`, ...) -----------------

fn handle_vim_cmdline_key(w: &mut WindowState, hwnd: HWND, vk: u32) {
    match vk {
        0x08 => {
            // Backspace
            if let crate::Prompt::VimCmdline(line) = &mut w.ws.prompt {
                line.pop();
            }
        }
        0x0D => commit_vim_cmdline(w, hwnd), // Enter
        _ => {}
    }
}

/// `Enter` sobre la línea de comandos vim: la interpreta con `parse_vim_cmd` y ejecuta
/// el resultado sobre el documento/pestaña activos.
fn commit_vim_cmdline(w: &mut WindowState, hwnd: HWND) {
    let line = match &w.ws.prompt {
        crate::Prompt::VimCmdline(line) => line.clone(),
        _ => return,
    };
    w.ws.close_prompt();
    match crate::parse_vim_cmd(&line) {
        crate::VimCmd::Save => {
            try_save(w);
        }
        crate::VimCmd::Quit => {
            w.ws.close_active();
        }
        crate::VimCmd::SaveAndQuit => {
            try_save(w);
            w.ws.close_active();
        }
        crate::VimCmd::Substitute { pattern, replacement, global, ignore_case } => {
            // `replace_all` ya sustituye todas las apariciones de cada línea, que es lo que
            // pide la bandera `g`; sin ella, vim de verdad solo reemplaza la primera
            // ocurrencia de cada línea, matiz que esta primera versión no modela (se trata
            // `global` como si siempre estuviera activa). Documentado como simplificación.
            let _ = global;
            let opts = notty_core::SearchOptions { case_sensitive: !ignore_case, whole_word: false, regex: false };
            let _ = w.ws.active_mut().doc.replace_all(&pattern, &replacement, opts, std::time::Instant::now());
        }
        crate::VimCmd::Unknown(_) => {}
    }
    unsafe {
        update_title(hwnd, w.ws.active());
    }
}

fn handle_search_char(w: &mut WindowState, ch: char) {
    if let crate::Prompt::Find(s) | crate::Prompt::Replace(s) = &mut w.ws.prompt {
        s.type_char(ch);
    }
}

fn handle_search_backspace(w: &mut WindowState) {
    if let crate::Prompt::Find(s) | crate::Prompt::Replace(s) = &mut w.ws.prompt {
        s.backspace();
    }
}

fn handle_search_key(w: &mut WindowState, vk: u32, mods: Modifiers) {
    if mods.alt {
        match vk {
            0x43 => {
                toggle_search(w, crate::SearchState::toggle_case);
                return;
            } // Alt+C
            0x57 => {
                toggle_search(w, crate::SearchState::toggle_word);
                return;
            } // Alt+W
            0x52 => {
                toggle_search(w, crate::SearchState::toggle_regex);
                return;
            } // Alt+R
            _ => {}
        }
    }
    match vk {
        0x08 => handle_search_backspace(w), // Backspace
        // Tab alterna el campo activo (buscar/por) solo tiene sentido en Reemplazar.
        0x09 if matches!(w.ws.prompt, crate::Prompt::Replace(_)) => {
            toggle_search(w, crate::SearchState::toggle_field);
        }
        0x0D if mods.ctrl && mods.alt => replace_all_matches(w), // Ctrl+Alt+Enter
        0x0D if mods.shift => nav_search(w, false),              // Shift+Enter
        0x0D if matches!(w.ws.prompt, crate::Prompt::Replace(_)) => replace_current_match(w),
        0x0D => nav_search(w, true), // Enter
        _ => {}
    }
}

fn toggle_search(w: &mut WindowState, f: impl FnOnce(&mut crate::SearchState)) {
    if let crate::Prompt::Find(s) | crate::Prompt::Replace(s) = &mut w.ws.prompt {
        f(s);
    }
}

fn nav_search(w: &mut WindowState, forward: bool) {
    let (prompt, active) = w.ws.prompt_and_active();
    if let crate::Prompt::Find(s) | crate::Prompt::Replace(s) = prompt {
        if forward {
            s.next(&active.doc);
        } else {
            s.prev(&active.doc);
        }
    }
}

/// `Enter` en el prompt de reemplazar: sustituye solo la coincidencia actual y avanza
/// a la siguiente (a diferencia de `Ctrl+Alt+Enter`, que usa `Document::replace_all`).
fn replace_current_match(w: &mut WindowState) {
    let (query, replacement, opts, current) = match &w.ws.prompt {
        crate::Prompt::Replace(s) => (s.query.clone(), s.replacement.clone(), s.opts, s.current),
        _ => return,
    };
    if let Ok(m) = w.ws.active().doc.find_all(&query, opts) {
        if let Some(range) = m.get(current).cloned() {
            w.ws.active_mut().doc.replace_range(range, &replacement, std::time::Instant::now());
        }
    }
    nav_search(w, true);
}

fn replace_all_matches(w: &mut WindowState) {
    let (query, replacement, opts) = match &w.ws.prompt {
        crate::Prompt::Replace(s) => (s.query.clone(), s.replacement.clone(), s.opts),
        _ => return,
    };
    let _ = w.ws.active_mut().doc.replace_all(&query, &replacement, opts, std::time::Instant::now());
}
