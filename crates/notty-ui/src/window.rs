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
    GetWindowLongPtrW, GetWindowRect, HICON, HTCAPTION, HTCLIENT, HTMAXBUTTON, HTTOP, IDC_ARROW, IDC_IBEAM, IsWindow, IsZoomed, KillTimer,
    LoadCursorW, LoadIconW, MSG, NCCALCSIZE_PARAMS, PostQuitMessage, RegisterClassExW, SM_CXPADDEDBORDER,
    SM_CYFRAME, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SW_SHOW, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SetCursor, SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow,
    TranslateMessage, WHEEL_DELTA, WM_ACTIVATE, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_ENDSESSION, WM_DPICHANGED, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP,
    WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_PAINT, WM_SETCURSOR, WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER,
    WNDCLASSEXW, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW, MINMAXINFO, SIZE_MINIMIZED, WM_CONTEXTMENU, WM_GETMINMAXINFO,
    WM_RBUTTONDOWN, WM_RBUTTONUP,
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
/// Llega cuando el hilo del pipe de instancia única (o el chequeo de actualizaciones)
/// deja algo en `ipc_rx` (ver `wake_for_ipc`). Antes se sondeaba con un temporizador
/// cada 150 ms, que despertaba la ventana 7 veces por segundo sin hacer nada.
const WM_IPC: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 9;
/// Ventana principal de este proceso, para `wake_for_ipc` (0 si aún no hay).
static IPC_HWND: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Avisa a la ventana de que hay mensajes nuevos en su `ipc_rx`. Se puede llamar desde
/// cualquier hilo; si la ventana aún no existe no hace nada (al crearse lee lo pendiente).
pub fn wake_for_ipc() {
    let h = IPC_HWND.load(std::sync::atomic::Ordering::Acquire);
    if h != 0 {
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::PostMessageW(Some(HWND(h as *mut _)), WM_IPC, WPARAM(0), LPARAM(0)) };
    }
}
/// Id del `SetTimer` de animación (60Hz): repinta mientras haya alguna animación en
/// curso (menú/sugerencias al abrir, cambio de pestaña...) y se para en cuanto la
/// última termina.
const ID_ANIM_TIMER: usize = 3;
/// Se lo manda la ventana a sí misma tras el primer pintado: arranca pintando por CPU
/// (`Renderer::new_software`, se ve al instante) y aquí pasa a la GPU. Probado también
/// a cargar el driver en otro hilo mientras tanto: acababa a la vez y gastaba ~12 MB más.
const WM_GPU_READY: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 7;
/// Lo manda `syntax` al terminar un resaltado hecho en otro hilo (archivos grandes).
const WM_SYNTAX_READY: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 8;

/// Snapshot de documentos sucios (nombre, texto, sucio) leído por el `panic hook` para
/// volcar a `notty_io::recovery_dir()`. Vive en memoria estática (`Box::leak`) para que
/// el hook, que se instala una única vez para todo el proceso, tenga una dirección
/// válida sin depender de que el hilo que entra en pánico coopere activamente.
/// El texto va como `Buffer` (un rope): clonarlo cada segundo es O(1) y comparte la
/// memoria con el documento, en vez de copiar el archivo entero.
type RecoverySnapshot = Mutex<Vec<(String, notty_core::Buffer, bool, Option<std::path::PathBuf>)>>;

/// Estado ligado a una ventana concreta: se guarda en `GWLP_USERDATA` mientras vive.
struct WindowState {
    ws: crate::Workspace,
    renderer: Renderer,
    mouse_down: bool,
    selection_anchor: usize,
    /// Arrastre en curso del tirador de la barra de scroll del editor: (y del ratón,
    /// `first_line` de cuando se empezó a arrastrar). `None` en reposo.
    scroll_drag: Option<(f32, usize)>,
    cfg: Rc<RefCell<notty_config::Config>>,
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
    /// Bandas (`layout::Bands`) resueltas en el último `WM_PAINT`: comparar contra
    /// ellas es cómo se detecta que un cambio de ajuste (desde Ajustes, u otra vía)
    /// acaba de hacer aparecer/desaparecer una banda entera, para fundirla en vez de
    /// que salte de golpe (Task 3 del plan de animaciones). `None` antes del primer
    /// pintado: nada que comparar todavía, así que ese primer pintado nunca anima.
    last_bands: Option<layout::Bands>,
    /// Fracción visible de cada banda de la que se viene mientras `chrome_fade_anim`
    /// está en curso (`None` en reposo): junto con ella, `render.rs` funde el
    /// contenido que cambia de visibilidad en vez de saltar directamente al nuevo
    /// estado. Es fraccionaria (no `Bands`) para que un cambio a mitad de transición
    /// (Alt pulsado varias veces seguidas) arranque desde donde está ahora.
    chrome_from: Option<layout::BandFrac>,
    /// Animación en curso de ese fundido (extensión de `start_tab_switch_anim` a
    /// cualquier banda de la interfaz, no solo la pestaña activa). `None` en reposo.
    chrome_fade_anim: Option<crate::Anim>,
    /// Tema (oscuro o no) del último pintado, y fundido en curso desde el anterior.
    last_dark: Option<bool>,
    theme_anim: Option<(bool, crate::Anim)>,
    /// Color de acento del último pintado, y fundido en curso desde el anterior
    /// (Ajustes → Apariencia → Color de acento), igual que `last_dark`/`theme_anim`.
    last_accent: Option<notty_config::AccentColor>,
    accent_anim: Option<(notty_config::AccentColor, crate::Anim)>,
    /// Si el `SetTimer` de animación (`ID_ANIM_TIMER`) está corriendo.
    anim_timer_running: bool,
    /// Si ya se pidió pasar a la GPU tras el primer pintado (ver `WM_GPU_READY`).
    gpu_requested: bool,
    /// Aviso/panel de actualización disponible (Task 4 del plan del actualizador):
    /// qué release se encontró (si alguna), si el panel de notas está abierto, y el
    /// progreso de la descarga en curso. Ver `crate::update_panel::UpdateState`.
    update: crate::UpdateState,
    /// Extremo receptor del hilo de descarga+verificación lanzado al pulsar
    /// "Actualizar" (Task 4, Step 3), si hay uno en curso.
    download_rx: Option<std::sync::mpsc::Receiver<DownloadEvent>>,
    /// Extremo receptor del hilo de "Buscar ahora" (Task 4, Step 4), si hay uno en
    /// curso: a diferencia del chequeo automático, siempre deja un mensaje.
    manual_check_rx: Option<std::sync::mpsc::Receiver<ManualCheckEvent>>,
    /// Clave pública Ed25519 contra la que se verifica `notty-setup.exe` antes de
    /// ejecutarlo (Task 4, Step 5 / Task 5 del plan): la define `notty` (el binario
    /// que se distribuye), nunca `notty-update` — ver `notty::PUBKEY`.
    pubkey: [u8; 32],
    /// "owner/repo" de GitHub Releases contra el que se comprueban actualizaciones
    /// (ver `notty::REPO`): también lo define el binario, no `notty-ui`.
    repo: String,
    /// Recorrido guiado en curso (Task 4 del plan de tutorial), `None` en reposo.
    /// Mientras hay uno, `WM_KEYDOWN`/`WM_LBUTTONDOWN` se lo ofrecen antes que a
    /// cualquier otro manejador, y `WM_PAINT` lo dibuja el último (por encima de
    /// todo lo demás).
    tour: Option<crate::tour::Tour>,
    /// Menú contextual abierto (clic derecho, `Shift+F10`, tecla Menú), si lo hay.
    ctx_menu: Option<crate::context_menu::ContextMenu>,
    /// Elemento resaltado con las flechas en el desplegable o menú contextual abierto.
    menu_sel: Option<usize>,
    /// Pestañas entrando (creciendo) o saliendo (encogiendo), ver `tab_anim.rs`.
    tab_anims: crate::tab_anim::TabAnims,
    /// Ventana "Acerca de notty" (menú Ayuda) abierta.
    about_open: bool,
    /// Popup de Novedades abierto desde este instante (ver `crate::whats_new`).
    whats_new_opened: Option<std::time::Instant>,
    /// Cuándo se copió la ruta por última vez (aviso de 1,8 s).
    path_copied_at: Option<std::time::Instant>,
    /// Aviso breve en la barra de estado (texto y cuándo apareció), ver `show_notice`.
    notice: Option<(String, std::time::Instant)>,
    /// Descartar el próximo `WM_CHAR` (la tecla ya se usó en `WM_KEYDOWN`).
    swallow_char: bool,
    /// Mitad alta de un par sustituto pendiente de `WM_CHAR` (emoji).
    pending_surrogate: Option<u16>,
    /// Ya se resolvió qué hacer con lo no guardado: el próximo `WM_CLOSE` cierra de verdad.
    close_confirmed: bool,
    /// Pestañas y paneles de `Files::Splits`. Se mantiene al día también en los otros
    /// modos (al cerrar documentos) para que volver a Paneles no apunte a índices que
    /// ya no existen.
    layouts: crate::splits::Layouts,
    /// Modo acomodar (Ctrl+Shift+R): las flechas cambian el ancho de los paneles.
    resize_mode: bool,
}

impl WindowState {
    /// La config a pasar al renderer: si el menú es `Alt`, se sustituye por
    /// `Visible`/`Hidden` según `menu_visible` sin tocar la config en disco.
    fn render_ui(&self) -> notty_config::UiConfig {
        let mut ui = self.cfg.borrow().ui;
        // Siguiendo a essentials, su tema y su acento en vez de los propios.
        (ui.theme, ui.accent) = crate::shared_theme::effective(&ui);
        if ui.menubar == notty_config::MenuBar::Alt {
            ui.menubar =
                if self.menu_visible { notty_config::MenuBar::Visible } else { notty_config::MenuBar::Hidden };
        }
        ui
    }

    /// Contexto de dibujo que no vive en `Workspace`/`UiConfig`.
    fn view_state(&self, hwnd: HWND) -> crate::ViewState {
        let dark = crate::is_dark(self.render_ui().theme, system_uses_dark_mode());
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
            chrome_from: self.chrome_from,
            chrome_fade: self.chrome_fade_anim.map(|a| a.value(std::time::Instant::now(), 0.0, 1.0)),
            theme_from: self.theme_anim.map(|(from, a)| (from, a.value(std::time::Instant::now(), 0.0, 1.0))),
            accent_from: self.accent_anim.map(|(from, a)| (from, a.value(std::time::Instant::now(), 0.0, 1.0))),
            menu_sel: self.menu_sel,
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
    /// En modo Paneles, el cuerpo es el del panel con foco (que siempre muestra el
    /// documento activo), así que todo el manejo de ratón existente vale tal cual.
    fn body_and_gutter(&self) -> (layout::Rect, f32) {
        let ui = self.cfg.borrow().ui;
        let total = self.ws.active().doc.buffer().len_lines();
        let (body, gutter_w) =
            self.renderer.body_and_gutter(&ui, self.ws.len(), self.menu_bar_visible(), total, self.ws.active().raw.is_some() || self.ws.active().md_preview);
        let body = self.pane_rects(body).get(self.layouts.focus()).copied().unwrap_or(body);
        (body, gutter_w)
    }

    fn splits_on(&self) -> bool {
        self.cfg.borrow().ui.files == notty_config::Files::Splits
    }

    /// Rect de cada panel dentro de `body` (uno solo, `body`, fuera del modo Paneles).
    fn pane_rects(&self, body: layout::Rect) -> Vec<layout::Rect> {
        if self.splits_on() { crate::splits::pane_rects(body, self.layouts.weights()) } else { vec![body] }
    }

    /// Pone `layouts` al día con la lista de documentos (ver `Layouts::reconcile`).
    fn sync_layout(&mut self) {
        let (n, active) = (self.ws.len(), self.ws.active_index());
        self.layouts.reconcile(n, active);
    }

    /// Índice del panel bajo `(x, y)`, solo si hay más de uno.
    fn pane_at(&self, x: f32, y: f32) -> Option<usize> {
        let ui = self.cfg.borrow().ui;
        let body = self.renderer.current_frame(&ui, self.ws.len(), self.menu_bar_visible()).body;
        let rects = self.pane_rects(body);
        if rects.len() < 2 {
            return None;
        }
        rects.iter().position(|r| r.contains(x, y))
    }

    /// "Filas" totales del documento activo para la barra de scroll: líneas de texto,
    /// o filas de 16 bytes en la vista raw (ver `Renderer::editor_scrollbar_geom`).
    fn total_rows(&self) -> usize {
        let st = self.ws.active();
        match &st.raw {
            Some(raw) => raw.len().div_ceil(16).max(1),
            None => st.doc.buffer().len_lines(),
        }
    }
}

/// Arranca el temporizador de animación (60Hz) si no estaba ya corriendo. Se llama
/// cada vez que arranca una animación nueva.
fn ensure_anim_timer(w: &mut WindowState, hwnd: HWND) {
    if !w.anim_timer_running {
        crate::anim::start_frame_timer(hwnd, ID_ANIM_TIMER);
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

/// Abre `state` como pestaña nueva, que entra creciendo desde ancho 0.
fn open_tab(w: &mut WindowState, hwnd: HWND, state: EditorState) {
    w.ws.open(state);
    w.tab_anims.on_open(w.ws.active_index(), std::time::Instant::now(), w.animations_enabled);
    w.sync_layout();
    ensure_anim_timer(w, hwnd);
}

/// Cierra la pestaña `idx`; si de verdad desaparece de la fila (no era la última, que
/// solo se vacía), sale encogiendo mientras sus vecinas se deslizan a ocupar el hueco.
fn close_tab(w: &mut WindowState, hwnd: HWND, idx: usize) {
    let Some(st) = w.ws.iter().nth(idx) else { return };
    let name = crate::doc_name_lang(st.path.as_deref(), crate::lang::resolve(w.cfg.borrow().ui.lang));
    let dirty = st.doc.is_dirty();
    w.sync_layout();
    // Modo Paneles: al cerrar el panel con foco, el foco se queda en su pestaña (el
    // panel de la izquierda, o el de la derecha si era el primero).
    let neighbour = (w.splits_on() && idx == w.ws.active_index())
        .then(|| {
            let docs = w.layouts.tab_docs_of(idx);
            let pos = docs.iter().position(|&d| d == idx)?;
            if pos > 0 { docs.get(pos - 1).copied() } else { docs.get(1).copied() }
        })
        .flatten();
    if w.ws.close(idx) {
        w.layouts.on_doc_closed(idx);
        if let Some(n) = neighbour {
            w.ws.activate(if n > idx { n - 1 } else { n });
        }
        w.sync_layout();
        if !w.splits_on() {
            w.tab_anims.on_close(idx, name, dirty, std::time::Instant::now(), w.animations_enabled);
        }
        ensure_anim_timer(w, hwnd);
    } else {
        w.sync_layout();
    }
}

/// Cierra los documentos `indices` (y la ventana si `quit`), resolviendo antes lo que
/// no esté guardado según `files.on_close_unsaved`: preguntar o volcar a recuperación.
fn request_close(w: &mut WindowState, hwnd: HWND, indices: &[usize], quit: bool) {
    let mode = w.cfg.borrow().files.on_close_unsaved;
    let req = crate::CloseRequest::new(indices.iter().filter_map(|&i| w.ws.get(i)), mode, quit);
    continue_close(w, hwnd, req);
}

/// Pregunta por el siguiente documento pendiente o, si no queda ninguno, cierra.
fn continue_close(w: &mut WindowState, hwnd: HWND, mut req: crate::CloseRequest) {
    req.ask.retain(|id| w.ws.index_of(*id).is_some());
    if let Some(idx) = req.ask.first().and_then(|id| w.ws.index_of(*id)) {
        w.ws.activate(idx);
        req.name = crate::doc_name_lang(w.ws.active().path.as_deref(), crate::lang::resolve(w.cfg.borrow().ui.lang));
        close_menus(w);
        w.ws.prompt = crate::Prompt::CloseUnsaved(req);
        unsafe { update_title(hwnd, w.ws.active(), lang_of(w)) };
        return;
    }
    if matches!(w.ws.prompt, crate::Prompt::CloseUnsaved(_)) {
        w.ws.close_prompt();
    }
    let to_recover = w.ws.iter().filter(|st| req.recover.contains(&st.id));
    if let Err(e) = dump_docs_to_recovery(to_recover) {
        show_notice(w, hwnd, format!("{}: {}", tr_w(w, "No se pudo guardar para recuperar"), tr_w_owned(w, &e.to_string())));
        return;
    }
    if req.quit {
        w.close_confirmed = true;
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        return;
    }
    let mut idxs: Vec<usize> = req.targets.iter().filter_map(|id| w.ws.index_of(*id)).collect();
    idxs.sort_unstable();
    for i in idxs.into_iter().rev() {
        close_tab(w, hwnd, i);
    }
    refresh_recovery(w);
    unsafe { update_title(hwnd, w.ws.active(), lang_of(w)) };
}

/// Respuesta a "tiene cambios sin guardar" (tecla o clic).
fn answer_close(w: &mut WindowState, hwnd: HWND, choice: crate::CloseChoice) {
    let crate::Prompt::CloseUnsaved(mut req) = std::mem::take(&mut w.ws.prompt) else { return };
    match choice {
        crate::CloseChoice::Cancel => {}
        crate::CloseChoice::Discard => {
            req.ask.remove(0);
            continue_close(w, hwnd, req);
        }
        crate::CloseChoice::Save => {
            let Some(idx) = req.ask.first().and_then(|id| w.ws.index_of(*id)) else {
                continue_close(w, hwnd, req);
                return;
            };
            w.ws.activate(idx);
            req.ask.remove(0);
            let raw_dirty = w.ws.active().raw.as_ref().is_some_and(|r| r.is_dirty());
            if raw_dirty {
                match w.ws.active_mut().raw.as_mut().map(|raw| raw.save()) {
                    Some(Err(e)) => show_notice(w, hwnd, format!("{}: {}", tr_w(w, "No se pudo guardar"), tr_w_owned(w, &e.to_string()))),
                    _ => continue_close(w, hwnd, req),
                }
            } else if w.ws.active().path.is_none() {
                save_as_then_close(w, hwnd, req);
            } else if save_now(w, hwnd) {
                continue_close(w, hwnd, req);
            }
            // Si no se pudo guardar (o hay conflicto con el disco), el cierre se para ahí.
        }
    }
}

/// "Guardar" sobre un documento sin ruta: pide dónde con la línea de ruta (o el diálogo
/// nativo) y, si se guarda, el cierre continúa (ver `commit_path_prompt_with`).
fn save_as_then_close(w: &mut WindowState, hwnd: HWND, req: crate::CloseRequest) {
    let initial = default_save_dir(w).map(|d| format!("{}\\", d.display())).unwrap_or_default();
    let mut p = crate::PathPromptState::new(crate::Purpose::Save, initial);
    p.then_close = Some(req);
    if w.cfg.borrow().ui.native_file_dialog {
        let dir = default_save_dir(w);
        let Some(path) = crate::native_dialog::pick_path(hwnd, crate::Purpose::Save, dir.as_deref()) else { return };
        let Some(value) = path.to_str() else { return };
        let ctx = path_ctx(w);
        p.type_text(value, &ctx);
        w.ws.prompt = crate::Prompt::Path(p);
        commit_path_prompt(w, hwnd);
    } else {
        w.ws.prompt = crate::Prompt::Path(p);
        start_popup_anim(w, hwnd);
    }
}

/// Parte el panel con foco: el nuevo muestra el primer documento que no se ve en
/// ningún panel, o uno vacío nuevo si ya se ven todos.
fn split_pane(w: &mut WindowState, hwnd: HWND) {
    w.sync_layout();
    if w.layouts.split() {
        // Sin `open_tab`: no es una pestaña nueva que tenga que entrar creciendo.
        let cfg = w.cfg.borrow().clone();
        w.ws.open(maybe_vim(EditorState::new_empty(), &cfg));
        w.sync_layout();
    } else {
        show_notice(w, hwnd, format!("{} {} {}", tr_w(w, "Como mucho"), crate::splits::MAX_PANES, tr_w(w, "paneles por pestaña")));
    }
}

/// Da el foco al panel `i` (clic dentro de él, Alt+número): su documento pasa a ser
/// el activo.
fn focus_pane(w: &mut WindowState, i: usize) {
    w.sync_layout();
    if let Some(doc) = w.layouts.focus_pane(i) {
        w.ws.activate(doc);
    }
}

/// Pestaña siguiente/anterior: de documentos, o de grupos de paneles en modo Paneles.
fn step_tab(w: &mut WindowState, hwnd: HWND, dir: isize) {
    if w.splits_on() {
        w.sync_layout();
        let doc = w.layouts.step_tab(dir);
        w.ws.activate(doc);
    } else if dir > 0 {
        w.ws.next();
    } else {
        w.ws.prev();
    }
    start_tab_switch_anim(w, hwnd);
}

/// Ctrl+1…9: pestaña `n` (1 = la primera, 9 = la última, como en los navegadores).
fn goto_tab(w: &mut WindowState, hwnd: HWND, n: usize) {
    if w.splits_on() {
        w.sync_layout();
        let i = if n == 9 { w.layouts.tab_count() - 1 } else { n - 1 };
        if let Some(doc) = w.layouts.goto_tab(i) {
            w.ws.activate(doc);
        }
    } else {
        let i = if n == 9 { w.ws.len() - 1 } else { n - 1 };
        if i < w.ws.len() {
            w.ws.activate(i);
        }
    }
    start_tab_switch_anim(w, hwnd);
}

/// Documentos de la pestaña que contiene a `doc`: todos los de su grupo en modo
/// Paneles, o solo él.
fn tab_docs(w: &mut WindowState, doc: usize) -> Vec<usize> {
    if w.splits_on() {
        w.sync_layout();
        w.layouts.tab_docs_of(doc)
    } else {
        vec![doc]
    }
}

/// "Cerrar otras" / "Cerrar a la derecha" de la pestaña de `doc`, en documentos.
fn tab_neighbours(w: &mut WindowState, doc: usize, right_only: bool) -> Vec<usize> {
    if !w.splits_on() {
        return if right_only { (doc + 1..w.ws.len()).collect() } else { (0..w.ws.len()).filter(|&j| j != doc).collect() };
    }
    w.sync_layout();
    let groups = w.layouts.tab_groups();
    let Some(at) = groups.iter().position(|g| g.1.contains(&doc)) else { return Vec::new() };
    groups
        .iter()
        .enumerate()
        .filter(|(i, _)| if right_only { *i > at } else { *i != at })
        .flat_map(|(_, g)| g.1.clone())
        .collect()
}

/// Modo acomodar: flechas = ancho del panel con foco, Tab = otro panel, `=` = todos
/// iguales, Enter/Esc = salir. El resto de teclas no hace nada mientras dure.
fn handle_resize_key(w: &mut WindowState, vk: u32, shift: bool) {
    let step = if shift { 0.10 } else { 0.03 };
    match vk {
        0x27 => {
            w.layouts.resize(step);
        }
        0x25 => {
            w.layouts.resize(-step);
        }
        0x09 => {
            let doc = w.layouts.cycle_focus(if shift { -1 } else { 1 });
            w.ws.activate(doc);
        }
        0xBB | 0x30 | 0x6B => w.layouts.equalize(),
        0x0D | 0x1B => w.resize_mode = false,
        _ => {}
    }
}

/// Cierra el desplegable de la barra de menús y el menú contextual, si hay alguno.
fn close_menus(w: &mut WindowState) {
    w.open_menu = None;
    w.ctx_menu = None;
    w.menu_sel = None;
}

/// Dónde se pidió el menú contextual.
#[derive(Debug, Clone, Copy)]
enum CtxTarget {
    Tab(usize),
    Body,
}

fn clipboard_has_text() -> bool {
    unsafe {
        windows::Win32::System::DataExchange::IsClipboardFormatAvailable(windows::Win32::System::Ole::CF_UNICODETEXT.0 as u32)
            .is_ok()
    }
}

fn selected_text(st: &EditorState) -> String {
    let sel = st.doc.selection();
    if sel.is_empty() { String::new() } else { st.doc.buffer().slice(sel.range()) }
}

/// Abre el menú contextual de `target` con la esquina en `(x, y)` (DIPs de cliente).
fn open_context_menu(w: &mut WindowState, hwnd: HWND, target: CtxTarget, x: f32, y: f32) {
    let lang = crate::lang::resolve(w.cfg.borrow().ui.lang);
    let items = match target {
        CtxTarget::Tab(i) => {
            let Some(st) = w.ws.iter().nth(i) else { return };
            crate::context_menu::tab_menu(i, w.ws.len(), st.path.is_some(), lang)
        }
        CtxTarget::Body => crate::context_menu::body_menu(
            &crate::context_menu::BodyCtx {
                selection: selected_text(w.ws.active()),
                clipboard_has_text: clipboard_has_text(),
                search_open: matches!(w.ws.prompt, crate::Prompt::Find(_) | crate::Prompt::Replace(_)),
            },
            lang,
        ),
    };
    w.open_menu = None;
    w.about_open = false;
    w.ctx_menu = Some(crate::context_menu::ContextMenu { items, x, y });
    w.menu_sel = None;
    start_popup_anim(w, hwnd);
    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
}

/// Menú contextual del texto con el teclado (`Shift+F10`, tecla Menú): se abre bajo
/// el cursor de texto.
fn open_context_menu_at_caret(w: &mut WindowState, hwnd: HWND) {
    if w.ws.active().raw.is_some() {
        return;
    }
    let (body, gutter_w) = w.body_and_gutter();
    let (x, y) = w.renderer.caret_point(w.ws.active(), body, gutter_w);
    open_context_menu(w, hwnd, CtxTarget::Body, x, y);
}

/// Ctrl+C / Ctrl+X y sus equivalentes del menú contextual. En vim no cambia de modo.
fn clipboard_copy(w: &mut WindowState, hwnd: HWND, cut: bool) {
    let text = selected_text(w.ws.active());
    if !text.is_empty() {
        let _ = crate::clipboard::set_clipboard_text(hwnd, &text);
        if cut {
            w.ws.active_mut().doc.backspace(std::time::Instant::now());
        }
    }
}

/// Ctrl+V y "Pegar": inserta en el cursor, también en el modo Normal de vim.
fn clipboard_paste(w: &mut WindowState, hwnd: HWND) {
    if let Ok(text) = crate::clipboard::get_clipboard_text(hwnd) {
        if !text.is_empty() {
            let st = w.ws.active_mut();
            st.doc.insert(&text, std::time::Instant::now());
            let (line, _) = st.doc.buffer().line_col(st.doc.selection().head);
            st.viewport.scroll_to_include(line, st.doc.buffer().len_lines());
        }
    }
}

/// Abre Buscar (o Reemplazar) con la selección ya escrita, apuntando a la
/// coincidencia que es la propia selección. Conserva las opciones (Aa, ab, .*) de un
/// prompt de búsqueda que ya estuviera abierto.
fn find_prefilled(w: &mut WindowState, replace: bool) {
    let st = w.ws.active();
    let sel = st.doc.selection().range();
    let mut s = crate::SearchState::default();
    if let crate::Prompt::Find(prev) | crate::Prompt::Replace(prev) = &w.ws.prompt {
        s.opts = prev.opts;
        s.replacement = prev.replacement.clone();
    }
    s.set_query(selected_text(st));
    if let Ok(m) = s.matches(&st.doc) {
        if let Some(i) = m.iter().position(|r| r.start >= sel.start) {
            s.current = i;
        }
    }
    w.ws.prompt = if replace { crate::Prompt::Replace(s) } else { crate::Prompt::Find(s) };
}

/// `ShellExecuteW` con verbo "open" (sin esperar a que termine).
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

fn run_ctx_cmd(w: &mut WindowState, hwnd: HWND, cmd: crate::context_menu::CtxCmd) {
    use crate::context_menu::CtxCmd;
    let now = std::time::Instant::now();
    let path_of = |w: &WindowState, i: usize| w.ws.iter().nth(i).and_then(|st| st.path.clone());
    match cmd {
        CtxCmd::NewTab => {
            let cfg = w.cfg.borrow().clone();
            open_tab(w, hwnd, maybe_vim(crate::EditorState::new_empty(), &cfg));
        }
        CtxCmd::CloseTab(i) => {
            let docs = tab_docs(w, i);
            request_close(w, hwnd, &docs, false);
        }
        CtxCmd::CloseOthers(i) => {
            if i < w.ws.len() {
                let others = tab_neighbours(w, i, false);
                request_close(w, hwnd, &others, false);
            }
        }
        CtxCmd::CloseRight(i) => {
            let right = tab_neighbours(w, i, true);
            request_close(w, hwnd, &right, false);
        }
        CtxCmd::CopyPath(i) => {
            if let Some(p) = path_of(w, i) {
                if crate::clipboard::set_clipboard_text(hwnd, &p.display().to_string()).is_ok() {
                    w.path_copied_at = Some(std::time::Instant::now());
                    ensure_anim_timer(w, hwnd);
                }
            }
        }
        CtxCmd::OpenFolder(i) => {
            if let Some(p) = path_of(w, i) {
                if p.exists() {
                    shell_open(hwnd, "explorer.exe", Some(&format!("/select,\"{}\"", p.display())));
                } else if let Some(dir) = p.parent().filter(|d| d.exists()) {
                    shell_open(hwnd, &dir.display().to_string(), None);
                }
            }
        }
        CtxCmd::SaveAs(i) => {
            if i != w.ws.active_index() && i < w.ws.len() {
                w.ws.activate(i);
                start_tab_switch_anim(w, hwnd);
            }
            let initial = w.ws.active().path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
            if start_path_entry(w, hwnd, crate::Purpose::Save, initial) {
                start_popup_anim(w, hwnd);
            }
        }
        CtxCmd::Undo => w.ws.active_mut().apply(crate::EditorAction::Undo, now),
        CtxCmd::Redo => w.ws.active_mut().apply(crate::EditorAction::Redo, now),
        CtxCmd::Cut => clipboard_copy(w, hwnd, true),
        CtxCmd::Copy => clipboard_copy(w, hwnd, false),
        CtxCmd::Paste => clipboard_paste(w, hwnd),
        CtxCmd::Delete => {
            if !w.ws.active().doc.selection().is_empty() {
                w.ws.active_mut().doc.backspace(now);
            }
        }
        CtxCmd::SelectAll => w.ws.active_mut().apply(crate::EditorAction::SelectAll, now),
        CtxCmd::FindSelection => find_prefilled(w, false),
        CtxCmd::ReplaceSelection => find_prefilled(w, true),
        CtxCmd::FindNext | CtxCmd::FindPrev => {
            if !matches!(w.ws.prompt, crate::Prompt::Find(_) | crate::Prompt::Replace(_)) {
                find_prefilled(w, false);
            }
            nav_search(w, cmd == CtxCmd::FindNext);
        }
    }
    unsafe { update_title(hwnd, w.ws.active(), lang_of(w)) };
    refresh_recovery(w);
}

/// Flechas / Inicio / Fin / Enter / Esc (y ← → entre menús de la barra) mientras hay
/// un desplegable o menú contextual abierto. `true` si la tecla era para el menú.
fn handle_menu_key(w: &mut WindowState, hwnd: HWND, vk: u32) -> bool {
    use crate::menu::{MENUS, MenuItem};
    let flags: Vec<bool> = if let Some(m) = &w.ctx_menu {
        m.items.iter().map(|it| it.is_enabled()).collect()
    } else if let Some(i) = w.open_menu {
        MENUS[i].items.iter().map(|it| matches!(it, MenuItem::Entry { .. })).collect()
    } else {
        return false;
    };
    match vk {
        0x1B => close_menus(w),
        0x28 => w.menu_sel = crate::context_menu::step_selection(&flags, w.menu_sel, 1),
        0x26 => w.menu_sel = crate::context_menu::step_selection(&flags, w.menu_sel, -1),
        0x24 => w.menu_sel = crate::context_menu::step_selection(&flags, None, 1),
        0x23 => w.menu_sel = crate::context_menu::step_selection(&flags, None, -1),
        0x0D => {
            if let Some(j) = w.menu_sel {
                activate_menu_row(w, hwnd, j);
            }
        }
        0x25 | 0x27 if w.ctx_menu.is_none() => {
            if let Some(i) = w.open_menu {
                let n = MENUS.len();
                w.open_menu = Some(if vk == 0x27 { (i + 1) % n } else { (i + n - 1) % n });
                w.menu_sel = None;
                start_popup_anim(w, hwnd);
            }
        }
        _ => {
            // Cualquier otra tecla cierra el menú contextual y sigue su camino normal
            // (el desplegable de la barra se queda abierto, como antes).
            if w.ctx_menu.is_some() {
                close_menus(w);
                let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            }
            return false;
        }
    }
    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    true
}

/// Ejecuta la fila `j` del menú abierto (contextual o de la barra) y lo cierra.
fn activate_menu_row(w: &mut WindowState, hwnd: HWND, j: usize) {
    if let Some(menu) = w.ctx_menu.take() {
        w.menu_sel = None;
        if let Some(cmd) = menu.cmd(j) {
            run_ctx_cmd(w, hwnd, cmd);
        }
    } else if let Some(i) = w.open_menu.take() {
        w.menu_sel = None;
        run_menu_item(w, hwnd, i, j);
    }
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
fn window_title(state: &EditorState, lang: notty_config::Lang) -> String {
    let base = match &state.path {
        Some(p) => p.display().to_string(),
        None => crate::strings::tr(lang, "sin título").to_string(),
    };
    let dirty = if state.doc.is_dirty() { " •" } else { "" };
    format!("{base}{dirty} · notty")
}

unsafe fn update_title(hwnd: HWND, state: &EditorState, lang: notty_config::Lang) {
    unsafe {
        let title_wide = to_wide(&window_title(state, lang));
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
                open_tab(w, hwnd, maybe_vim(EditorState::from_opened(opened), &cfg));
            } else {
                let mut state = EditorState::new_empty();
                state.path = Some(path);
                open_tab(w, hwnd, maybe_vim(state, &cfg));
            }
            update_title(hwnd, w.ws.active(), lang_of(w));
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

thread_local! {
    /// HWND de la ventana de Ajustes ya abierta, si hay una (Task: "no debería dejarte
    /// abrir varias pestañas de ajustes a la vez"). Un solo hilo de UI, así que un
    /// `Cell` de por sí (sin `Mutex`) es seguro.
    static OPEN_SETTINGS_HWND: std::cell::Cell<Option<isize>> = const { std::cell::Cell::new(None) };
}

/// Abre la ventana de Ajustes sobre `hwnd`: `Ctrl+,`, el menú Archivo → Ajustes, y el
/// engranaje de la barra de título llegan todos aquí, para no repetir el `Box::new`.
fn open_settings(w: &WindowState, hwnd: HWND) {
    open_settings_at(w, hwnd, "");
}

const ZOOM_STEP: f32 = 0.1;

/// Ctrl+=/Ctrl+-/Ctrl+0: cambia el tamaño del texto del editor y lo deja guardado. El
/// recorte real (0.5x–3x) lo hace `Renderer::set_font_scale`; aquí solo se guarda lo
/// que haya quedado tras el recorte, para que Ajustes y el arranque siguiente
/// coincidan con lo que se ve.
fn set_font_scale(w: &mut WindowState, hwnd: HWND, scale: f32) {
    if w.renderer.set_font_scale(scale).is_err() {
        return;
    }
    let applied = w.renderer.font_scale();
    {
        let mut cfg = w.cfg.borrow_mut();
        cfg.ui.font_scale = applied;
        let _ = notty_config::save(&cfg, &notty_config::default_path());
    }
    let (body, _) = w.body_and_gutter();
    let line_h = w.renderer.line_height();
    w.ws.active_mut().viewport.visible_lines = layout::visible_lines(body, line_h);
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn open_settings_at(w: &WindowState, hwnd: HWND, section: &str) {
    if let Some(raw) = OPEN_SETTINGS_HWND.with(|c| c.get()) {
        let existing = HWND(raw as *mut _);
        if unsafe { IsWindow(Some(existing)) }.as_bool() {
            crate::settings_window::focus_existing(existing, section);
            return;
        }
    }
    // essentials pudo escribir `[updates] managed_by` con notty ya abierto.
    if let notty_config::LoadResult::Loaded(disk) = notty_config::load(&notty_config::default_path()) {
        w.cfg.borrow_mut().updates.managed_by = disk.updates.managed_by;
    }
    let cfg_for_settings = w.cfg.clone();
    let cfg_for_theme = w.cfg.clone();
    let _ = crate::settings_window::open(
        hwnd,
        cfg_for_settings,
        Box::new(move || unsafe {
            let dark = crate::is_dark(crate::shared_theme::effective(&cfg_for_theme.borrow().ui).0, system_uses_dark_mode());
            apply_dark_mode(hwnd, dark);
            // Por si el cambio fue la fuente del editor (Ajustes → Apariencia): se
            // reaplica siempre, es barato comparado con abrir Ajustes en sí, y así no
            // hace falta que este callback sepa qué ajuste concreto se tocó.
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if let Some(w) = ptr.as_mut() {
                let (family, scale) = {
                    let c = cfg_for_theme.borrow();
                    (c.ui.font_family, c.ui.font_scale)
                };
                let _ = w.renderer.set_mono_family(family.primary_name());
                // Ajustes → Fuentes → Tamaño.
                if (w.renderer.font_scale() - scale).abs() > 1e-3 {
                    let _ = w.renderer.set_font_scale(scale);
                }
                let (body, _) = w.body_and_gutter();
                let line_h = w.renderer.line_height();
                w.ws.active_mut().viewport.visible_lines = layout::visible_lines(body, line_h);
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
        }),
        Box::new(move |path| open_config_as_document(hwnd, path)),
        Box::new(move || trigger_check_updates_now(hwnd)),
        Box::new(move || {
            with_window(hwnd, |w| start_update_download(w, hwnd));
        }),
        Box::new(move || with_window(hwnd, |w| settings_update_info(w)).unwrap_or_default()),
        Box::new(move || start_tour(hwnd)),
        Box::new(move || {
            with_window(hwnd, |w| open_whats_new(w, hwnd));
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
                let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
            }
        }),
        section,
        |created_hwnd| OPEN_SETTINGS_HWND.with(|c| c.set(Some(created_hwnd.0 as isize))),
    );
    OPEN_SETTINGS_HWND.with(|c| c.set(None));
}

/// Ejecuta `f` sobre el `WindowState` de la ventana principal `hwnd` (Ajustes vive
/// más que el `&WindowState` con el que se abrió).
fn with_window<R>(hwnd: HWND, f: impl FnOnce(&mut WindowState) -> R) -> Option<R> {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        ptr.as_mut().map(f)
    }
}

/// Lo que enseña Ajustes → Actualizaciones.
fn settings_update_info(w: &WindowState) -> crate::settings_window::UpdateInfo {
    let release = w.update.available.as_ref();
    crate::settings_window::UpdateInfo {
        phase: w.update.phase_for_settings(),
        new_version: release.map(|r| r.version.clone()),
        notes: release.map(|r| r.body.clone()).unwrap_or_default(),
        release_url: release.map(|r| format!("https://github.com/{}/releases/tag/{}", w.repo, r.tag)),
        up_to_date: w.update.up_to_date,
    }
}

/// "Buscar ahora" desde Ajustes → Actualizaciones: recupera el `WindowState` de la
/// ventana principal desde `GWLP_USERDATA` (igual que `open_config_as_document`)
/// y lanza `check_updates_now` sobre él.
fn trigger_check_updates_now(hwnd: HWND) {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        if let Some(w) = ptr.as_mut() {
            check_updates_now(w, hwnd);
        }
    }
}

/// Arranca el recorrido guiado sobre la ventana `hwnd`: Ajustes → Ayuda → "Repetir
/// tutorial" y el paso Listo de `welcome_window` llegan aquí. Vuelve a buscar el
/// `WindowState` por `GWLP_USERDATA` (igual que `open_config_as_document`) porque
/// quien llama a esto es un `Box<dyn Fn()>` que vive más allá del `&WindowState`
/// original.
fn start_tour(hwnd: HWND) {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        if let Some(w) = ptr.as_mut() {
            w.tour = Some(crate::tour::Tour::start(w.animations_enabled));
            // La entrada (vela+foco fundiéndose) y la propia transición entre paradas
            // son animaciones cortas: sin este temporizador, `WM_PAINT` solo se
            // repetiría en reacción a otra entrada del usuario y la animación se
            // vería como un salto en vez de un fundido (parte del defecto #3).
            ensure_anim_timer(w, hwnd);
            let _ = InvalidateRect(Some(hwnd), None, false);
            // Se arranca desde la bienvenida o desde Ajustes, que se están cerrando: sin
            // esto el foco puede quedarse fuera y Ctrl+N/Ctrl+W no llegarían a la app.
            let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(hwnd));
        }
    }
}

/// Guarda el documento activo, pero antes comprueba si el archivo cambió en disco
/// desde que se abrió: si es así, abre `Prompt::Conflict` en vez de escribir encima.
fn try_save(w: &mut WindowState) -> SaveOutcome {
    let path_and_since = {
        let st = w.ws.active();
        st.path.clone().map(|p| (p, st.open_mtime))
    };
    if let Some((path, Some(since))) = path_and_since {
        if notty_io::changed_since(&path, since) {
            w.ws.open_conflict(crate::lang::resolve(w.cfg.borrow().ui.lang));
            return SaveOutcome::Conflict;
        }
    }
    if let Err(e) = w.ws.active_mut().save() {
        return SaveOutcome::Failed(e.to_string());
    }
    let st = w.ws.active_mut();
    st.autosave_failed_rev = None;
    st.autosave_paused = false;
    if let Some(path) = st.path.clone() {
        st.open_mtime = notty_io::mtime(&path).ok();
    }
    SaveOutcome::Saved
}

/// Una pregunta que no puede esperar (conflicto al autoguardar): se cierran menús y,
/// si la ventana no está delante, parpadea en la barra de tareas.
fn claim_attention(w: &mut WindowState, hwnd: HWND) {
    close_menus(w);
    if !w.active_window {
        use windows::Win32::UI::WindowsAndMessaging::{FLASHW_ALL, FLASHW_TIMERNOFG, FLASHWINFO, FlashWindowEx};
        let info = FLASHWINFO {
            cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
            hwnd,
            dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG,
            uCount: 0,
            dwTimeout: 0,
        };
        unsafe {
            let _ = FlashWindowEx(&info);
        }
    }
}

enum SaveOutcome {
    Saved,
    Conflict,
    Failed(String),
}

/// Guardado pedido por el usuario: si falla, lo dice en la barra de estado.
/// Devuelve `true` solo si se guardó.
fn save_now(w: &mut WindowState, hwnd: HWND) -> bool {
    match try_save(w) {
        SaveOutcome::Saved => true,
        SaveOutcome::Conflict => false,
        SaveOutcome::Failed(e) => {
            show_notice(w, hwnd, format!("{}: {}", tr_w(w, "No se pudo guardar"), tr_w_owned(w, &e.to_string())));
            false
        }
    }
}

/// `WM_TIMER` de autoguardado: dispara cada segundo, pero solo actúa si hay cambios
/// sin guardar (equivalente en la práctica a reprogramar el temporizador tras cada
/// tecla, y mucho más simple — ver Task 7 Step 3 del plan). Solo se autoguarda el
/// documento activo, solo si tiene ruta real, no es temporal volátil, y no hay ya un
/// conflicto sin resolver.
/// Devuelve `true` si cambió algo visible (se guardó, o hay aviso nuevo).
fn autosave_tick(w: &mut WindowState, hwnd: HWND) -> bool {
    refresh_recovery(w);
    if !w.cfg.borrow().files.autosave {
        return false;
    }
    if matches!(w.ws.prompt, crate::Prompt::Conflict(_)) {
        return false;
    }
    let st = w.ws.active();
    let has_real_path = st.path.is_some() && !matches!(st.temp, Some(notty_config::TempMode::Volatile));
    let already_failed = st.autosave_failed_rev == Some(st.doc.revision());
    if has_real_path && st.doc.is_dirty() && !already_failed && !st.autosave_paused {
        match try_save(w) {
            SaveOutcome::Failed(e) => {
                let st = w.ws.active_mut();
                let first = st.autosave_failed_rev.is_none();
                st.autosave_failed_rev = Some(st.doc.revision());
                if first {
                    show_notice(w, hwnd, format!("{}: {}", tr_w(w, "No se pudo autoguardar"), tr_w_owned(w, &e.to_string())));
                }
            }
            SaveOutcome::Conflict => claim_attention(w, hwnd),
            SaveOutcome::Saved => {}
        }
        return true;
    }
    false
}

/// Sondea `w.ipc_rx` (si lo hay) por mensajes del pipe de instancia única y los
/// aplica: `OpenPath` abre ese archivo igual que `Ctrl+O` con una ruta existente.
/// `NewTemp`/`NewPermanent` no llegan por este camino en la instancia normal (los
/// maneja el daemon lanzando `notty.exe --new-temp`/`--new-permanent`, Task 9); si
/// alguno llegara igualmente, no se hace nada. Devuelve `true` si hubo que repintar.
fn ipc_tick(w: &mut WindowState, hwnd: HWND) -> bool {
    let mut changed = false;
    let mut opened_docs = Vec::new();
    if let Some(rx) = w.ipc_rx.as_ref() {
        while let Ok(msg) = rx.try_recv() {
            match msg {
                notty_ipc::Message::OpenPath(p) => {
                    if !p.is_empty() {
                        let cfg = w.cfg.borrow().clone();
                        opened_docs.push(maybe_vim(state_for_path(std::path::Path::new(&p)), &cfg));
                        changed = true;
                    }
                }
                notty_ipc::Message::UpdateAvailable(r) => {
                    let release = notty_update::Release { tag: r.tag, version: r.version, body: r.body, setup_url: r.setup_url, sig_url: r.sig_url };
                    w.update.set_available(release);
                    changed = true;
                }
                notty_ipc::Message::NewTemp | notty_ipc::Message::NewPermanent => {}
            }
        }
    }
    let any_opened = !opened_docs.is_empty();
    for st in opened_docs {
        open_tab(w, hwnd, st);
    }
    if any_opened {
        unsafe {
            update_title(hwnd, w.ws.active(), lang_of(w));
            if windows::Win32::UI::WindowsAndMessaging::IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
        }
    }
    if download_tick(w) {
        changed = true;
    }
    if manual_check_tick(w) {
        changed = true;
    }
    changed
}

/// Eventos de "Buscar ahora" (Task 4, Step 4): a diferencia del chequeo
/// automático, siempre acaba en un mensaje (release nueva, ya actualizado, o
/// error), nunca en silencio.
#[derive(Debug)]
enum ManualCheckEvent {
    Found(notty_update::Release),
    UpToDate,
    Error(String),
}

fn manual_check_tick(w: &mut WindowState) -> bool {
    let Some(rx) = w.manual_check_rx.as_ref() else { return false };
    let events: Vec<ManualCheckEvent> = rx.try_iter().collect();
    let mut changed = false;
    for ev in events {
        changed = true;
        match ev {
            ManualCheckEvent::Found(release) => w.update.set_available(release),
            ManualCheckEvent::UpToDate => w.update.set_up_to_date(),
            ManualCheckEvent::Error(msg) => w.update.set_manual_error(msg),
        }
        w.manual_check_rx = None;
        // El chequeo manual también cuenta como "última comprobación" (Ajustes la
        // enseña), igual que el automático de arranque.
        let mut cfg = w.cfg.borrow_mut();
        cfg.updates.last_check = unix_now();
        let _ = notty_config::save(&cfg, &notty_config::default_path());
    }
    changed
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// "Buscar ahora" (Task 4, Step 4 del plan del actualizador): mismo chequeo que el
/// automático de arranque, pero disparado por el usuario y en un hilo aparte para
/// no bloquear la UI ("síncrono" en el plan quiere decir "lo pidió el usuario", no
/// "bloquea"). Siempre deja un resultado claro (nunca en silencio).
fn check_updates_now(w: &mut WindowState, hwnd: HWND) {
    if w.manual_check_rx.is_some() || w.cfg.borrow().updates.managed() {
        return; // ya hay una en marcha, o las actualizaciones son cosa de essentials
    }
    w.update.start_check();
    let (tx, rx) = std::sync::mpsc::channel();
    w.manual_check_rx = Some(rx);
    let repo = w.repo.clone();
    std::thread::spawn(move || {
        let version = crate::app_version();
        let ua = format!("notty/{version}");
        let event = match notty_update::http::latest_release(&repo, &ua) {
            Ok(release) if notty_update::is_newer(version, &release.version) => ManualCheckEvent::Found(release),
            Ok(_) => ManualCheckEvent::UpToDate,
            Err(e) => ManualCheckEvent::Error(e.to_string()),
        };
        let _ = tx.send(event);
        wake_for_ipc();
    });
    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
}

/// Sondea `w.download_rx` (Task 4, Step 3: hilo de descarga tras pulsar
/// "Actualizar") y aplica lo que haya llegado al `UpdateState`. El propio hilo se
/// borra a sí mismo el receptor cuando termina (éxito o error), así que no hace
/// falta acordarse de limpiar nada aquí aparte de vaciar la cola.
fn download_tick(w: &mut WindowState) -> bool {
    let Some(rx) = w.download_rx.as_ref() else { return false };
    let events: Vec<DownloadEvent> = rx.try_iter().collect();
    let mut changed = false;
    for ev in events {
        changed = true;
        match ev {
            DownloadEvent::Progress(done, total) => w.update.set_progress(done, total),
            DownloadEvent::VerifyFailed => {
                w.update.fail_verification();
                w.download_rx = None;
            }
            DownloadEvent::DownloadError(msg) => {
                w.update.download_error(msg);
                w.download_rx = None;
            }
            DownloadEvent::ReadyToRelaunch(setup_exe) => {
                relaunch_via_setup(w, &setup_exe);
                w.download_rx = None;
            }
        }
    }
    changed
}

/// Eventos del hilo de descarga (Task 4, Step 3 del plan del actualizador): vive
/// solo entre `start_update_download` y `download_tick`, no cruza procesos (a
/// diferencia de `notty_ipc::Message`, que sí).
#[derive(Debug)]
enum DownloadEvent {
    Progress(u64, u64),
    VerifyFailed,
    DownloadError(String),
    ReadyToRelaunch(std::path::PathBuf),
}

/// Clic en "Actualizar" del panel (Task 4, Step 3): descarga `notty-setup.exe` y su
/// `.sig` a `%TEMP%\notty-update\`, verifica la firma con `w.pubkey`, y si es válida
/// relanza `notty-setup.exe --update --relaunch`. Si la firma no cuadra o falla la
/// descarga, borra los archivos y dice por qué en el panel — nunca ejecuta un
/// binario sin verificar (`Global Constraints` del plan).
fn start_update_download(w: &mut WindowState, hwnd: HWND) {
    if w.cfg.borrow().updates.managed() {
        return;
    }
    let Some(release) = w.update.available.clone() else { return };
    w.update.start_download();
    let (tx, rx) = std::sync::mpsc::channel();
    w.download_rx = Some(rx);
    let pubkey = w.pubkey;
    let hwnd_usize = hwnd.0 as usize;

    std::thread::spawn(move || {
        let dir = std::env::temp_dir().join("notty-update");
        if std::fs::create_dir_all(&dir).is_err() {
            let _ = tx.send(DownloadEvent::DownloadError("no se pudo crear el directorio temporal".to_string()));
            wake_for_ipc();
            return;
        }
        let setup_path = dir.join("notty-setup.exe");
        let sig_path = dir.join("notty-setup.exe.sig");

        let progress_tx = tx.clone();
        // La barra de progreso se repinta como mucho ~30 veces por segundo, no una por
        // cada trozo descargado.
        let mut last_wake: Option<std::time::Instant> = None;
        if let Err(e) = notty_update::http::download(&release.setup_url, &setup_path, |done, total| {
            let _ = progress_tx.send(DownloadEvent::Progress(done, total));
            if last_wake.is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(33)) || done == total {
                last_wake = Some(std::time::Instant::now());
                wake_for_ipc();
            }
        }) {
            let _ = tx.send(DownloadEvent::DownloadError(e.to_string()));
            wake_for_ipc();
            return;
        }
        if let Err(e) = notty_update::http::download(&release.sig_url, &sig_path, |_, _| {}) {
            let _ = std::fs::remove_file(&setup_path);
            let _ = tx.send(DownloadEvent::DownloadError(e.to_string()));
            wake_for_ipc();
            return;
        }

        let (Ok(setup_bytes), Ok(sig_bytes)) = (std::fs::read(&setup_path), std::fs::read(&sig_path)) else {
            let _ = std::fs::remove_file(&setup_path);
            let _ = std::fs::remove_file(&sig_path);
            let _ = tx.send(DownloadEvent::DownloadError("no se pudo leer lo descargado".to_string()));
            wake_for_ipc();
            return;
        };
        let Ok(sig): std::result::Result<[u8; 64], _> = sig_bytes.try_into() else {
            let _ = std::fs::remove_file(&setup_path);
            let _ = std::fs::remove_file(&sig_path);
            let _ = tx.send(DownloadEvent::VerifyFailed);
            wake_for_ipc();
            return;
        };

        if !notty_update::verify(&setup_bytes, &sig, &pubkey) {
            let _ = std::fs::remove_file(&setup_path);
            let _ = std::fs::remove_file(&sig_path);
            let _ = tx.send(DownloadEvent::VerifyFailed);
            wake_for_ipc();
            return;
        }

        let _ = hwnd_usize; // Reservado por si una relanzada futura necesita notificar a esta ventana.
        let _ = tx.send(DownloadEvent::ReadyToRelaunch(setup_path));
        wake_for_ipc();
    });
}

/// Lanza `notty-setup.exe --update --relaunch --from <actual> --to <nuevo>` y
/// cierra esta instancia de notty (Task 4, Step 3). `notty-setup.exe` es del plan
/// del instalador (sibling plan); aquí solo se construye y lanza el comando.
///
/// "Guarda la sesión actual" (spec del plan) se resuelve reutilizando el volcado
/// de recuperación que ya existe (`refresh_recovery`/`notty_io::recovery_dir`):
/// no hay un mecanismo de sesión aparte en el resto del código (se buscó en
/// `daemon.rs` y `notty-ipc` primero, como pedía el plan), así que forzar un
/// volcado fresco justo antes de relanzar es el equivalente más fiel que hay.
fn relaunch_via_setup(w: &mut WindowState, setup_exe: &std::path::Path) {
    refresh_recovery(w);
    // Sin volcado a disco, lo no guardado se perdería al cerrar: mejor no relanzar.
    if let Err(e) = dump_recovery_snapshot(w.recovery) {
        w.update.download_error(format!("{}: {e}", tr_w(w, "no se pudo guardar la sesión")));
        return;
    }
    let current = std::env::current_exe().unwrap_or_default();
    let _ = std::process::Command::new(setup_exe)
        .arg("--update")
        .arg("--relaunch")
        .arg("--from")
        .arg(&current)
        .arg("--to")
        .arg(setup_exe)
        .spawn();
    unsafe { PostQuitMessage(0) };
}

/// Traduce `UpdateState` (Task 4 del plan del actualizador) al contenido que
/// `render.rs` necesita para pintar el panel de notas de versión, si está abierto.
/// `None` si no hay panel que pintar (nada encontrado, o cerrado).
fn update_panel_content(update: &crate::UpdateState, lang: notty_config::Lang) -> Option<crate::UpdatePanelContent> {
    if !update.panel_open {
        return None;
    }
    let release = update.available.as_ref()?;
    let status_line = update.error.as_ref().map(|e| crate::strings::tr_msg(lang, e)).or_else(|| {
        update.progress.map(|(done, total)| {
            let downloading = crate::strings::tr(lang, "Descargando...");
            if total > 0 {
                format!("{downloading} {} KB / {} KB", done / 1024, total / 1024)
            } else {
                downloading.to_string()
            }
        })
    });
    let downloading = matches!(update.phase, Some(crate::DownloadPhase::Downloading) | Some(crate::DownloadPhase::Verifying));
    Some(crate::UpdatePanelContent {
        version: release.version.clone(),
        body: release.body.clone(),
        status_line,
        show_actualizar: !downloading,
    })
}

/// Progreso del aviso "Ruta copiada" (`None` si no hay o ya terminó). Sin
/// animaciones se queda quieto y visible el mismo tiempo.
fn path_copied_progress(w: &WindowState) -> Option<f32> {
    let t0 = w.path_copied_at?;
    let p = t0.elapsed().as_secs_f32() / 1.8;
    if p >= 1.0 {
        return None;
    }
    Some(if w.animations_enabled { p } else { 0.5 })
}

/// Idioma efectivo de `w` (`Auto` ya resuelto). Atajo para traducir los avisos de
/// `show_notice`, que se disparan desde muchos sitios sueltos de este archivo.
fn lang_of(w: &WindowState) -> notty_config::Lang {
    crate::lang::resolve(w.cfg.borrow().ui.lang)
}

/// `crate::strings::tr` con el idioma de `w` ya resuelto.
fn tr_w(w: &WindowState, s: &'static str) -> &'static str {
    crate::strings::tr(lang_of(w), s)
}

/// Como `tr_w`, pero para texto que no es `&'static` (p.ej. el mensaje de un
/// `notty_io::CodecError` ya formateado): las pocas frases que puede devolver
/// (`editor.rs::save`) están en la tabla igual que cualquier otra.
fn tr_w_owned(w: &WindowState, s: &str) -> String {
    crate::strings::tr(lang_of(w), s).to_string()
}

/// Enseña `text` unos segundos sobre la barra de estado (mismo aviso que "Ruta copiada").
fn show_notice(w: &mut WindowState, hwnd: HWND, text: impl Into<String>) {
    w.path_copied_at = None;
    w.notice = Some((text.into(), std::time::Instant::now()));
    ensure_anim_timer(w, hwnd);
}

fn notice_progress(w: &WindowState) -> Option<(f32, String)> {
    let (text, t0) = w.notice.as_ref()?;
    let p = t0.elapsed().as_secs_f32() / 3.5;
    if p >= 1.0 {
        return None;
    }
    Some((if w.animations_enabled { p } else { 0.5 }, text.clone()))
}

/// Abre el popup de Novedades (Ayuda → Novedades, o solo al arrancar tras actualizar).
fn open_whats_new(w: &mut WindowState, hwnd: HWND) {
    w.about_open = false;
    w.whats_new_opened = Some(std::time::Instant::now());
    ensure_anim_timer(w, hwnd);
}

/// Al arrancar: si notty se acaba de actualizar a una versión con novedades, las
/// enseña. En cualquier caso apunta la versión actual como vista.
fn maybe_show_whats_new(w: &mut WindowState, hwnd: HWND) {
    let current = crate::app_version();
    let show = {
        let cfg = w.cfg.borrow();
        if cfg.last_seen_version == current {
            return;
        }
        crate::whats_new::should_show(&cfg.last_seen_version, cfg.first_run_done, current)
    };
    {
        let mut cfg = w.cfg.borrow_mut();
        cfg.last_seen_version = current.to_string();
        let _ = notty_config::save(&cfg, &notty_config::default_path());
    }
    if show {
        open_whats_new(w, hwnd);
    }
}

/// Contenido de "Acerca de notty": versión y enlace al repositorio (si `repo` no es
/// el marcador de posición de las compilaciones sin configurar).
fn about_content(repo: &str) -> crate::AboutContent {
    let url = (!repo.is_empty() && !repo.starts_with("OWNER/")).then(|| format!("https://github.com/{repo}"));
    crate::AboutContent { version: crate::app_version().to_string(), url }
}

/// Teclas de `Prompt::Conflict`: se escribe SI o NO y Enter; Esc cancela y deja el
/// autoguardado de ese documento en pausa hasta el próximo guardado a mano.
fn handle_conflict_key(w: &mut WindowState, hwnd: HWND, vk: u32) {
    let crate::Prompt::Conflict(c) = &mut w.ws.prompt else { return };
    let now = std::time::Instant::now();
    match vk {
        0x08 => c.backspace(),
        0x1B => {
            let id = c.doc;
            w.ws.close_prompt();
            if let Some(st) = w.ws.index_of(id).and_then(|i| w.ws.get_mut(i)) {
                st.autosave_paused = true;
                show_notice(w, hwnd, tr_w(w, "Autoguardado en pausa hasta que guardes a mano"));
            }
        }
        0x0D if c.accepts_input(now) => {
            let (id, answer) = (c.doc, c.answer());
            if answer == crate::ConflictAnswer::Invalid {
                c.invalid = true;
                c.input.clear();
                return;
            }
            w.ws.close_prompt();
            let Some(idx) = w.ws.index_of(id) else { return };
            let Some(st) = w.ws.get_mut(idx) else { return };
            match answer {
                crate::ConflictAnswer::KeepMine => match st.save() {
                    Ok(()) => {
                        st.autosave_failed_rev = None;
                        st.autosave_paused = false;
                        st.open_mtime = st.path.as_deref().and_then(|p| notty_io::mtime(p).ok());
                    }
                    Err(e) => show_notice(w, hwnd, format!("{}: {}", tr_w(w, "No se pudo guardar"), tr_w_owned(w, &e.to_string()))),
                },
                crate::ConflictAnswer::UseDisk => {
                    let reopened = st.path.clone().map(|p| (crate::open_as_document(&p), p));
                    match reopened {
                        Some((Ok(opened), path)) => {
                            st.doc = opened.document;
                            st.encoding = opened.encoding;
                            st.eol = opened.eol;
                            st.open_mtime = notty_io::mtime(&path).ok();
                            st.lossy_source = opened.lossy.then(|| path.clone());
                            st.autosave_paused = false;
                        }
                        Some((Err(e), _)) => show_notice(w, hwnd, format!("{}: {}", tr_w(w, "No se pudo leer el archivo"), tr_w_owned(w, &e.to_string()))),
                        None => {}
                    }
                }
                crate::ConflictAnswer::Invalid => {}
            }
        }
        _ => {}
    }
}

/// Documento para `path` como al abrirlo desde la línea de comandos: texto si se
/// puede, vista raw si es binario, y si no existe, un documento nuevo con esa ruta.
fn state_for_path(path: &std::path::Path) -> EditorState {
    match crate::open_as_document(path) {
        Ok(opened) => EditorState::from_opened(opened),
        Err(_) => {
            let mut state = EditorState::new_empty();
            match crate::open_raw_doc(path) {
                Ok(raw) => {
                    state.raw = Some(raw);
                    state.path = Some(path.to_path_buf());
                }
                // Existe pero no se puede leer: sin ruta, para que guardar no lo pise.
                Err(_) if path.exists() => {}
                Err(_) => state.path = Some(path.to_path_buf()),
            }
            state
        }
    }
}

/// Crea el documento con el `EditorState` que toque: si `cfg.ui.vim_always` está
/// activo, lo arranca ya en modo vim; si es un `.md`/`.markdown` y Ajustes → Archivos
/// dice que se abran en Previsualización, arranca también con ella puesta.
fn maybe_vim(mut st: EditorState, cfg: &notty_config::Config) -> EditorState {
    if cfg.ui.vim_always {
        st.vim = Some(crate::VimState::default());
    }
    if cfg.ui.md_open_mode == notty_config::MdOpenMode::Preview && is_markdown_path(st.path.as_deref()) {
        st.md_preview = true;
    }
    st
}

fn is_markdown_path(path: Option<&std::path::Path>) -> bool {
    path.and_then(|p| p.extension()).and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
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
    let snapshot: Vec<_> = w
        .ws
        .iter()
        .enumerate()
        .map(|(i, st)| {
            (recovery_name(st, i), st.doc.buffer().clone(), st.doc.is_dirty(), st.path.clone())
        })
        .collect();
    let mut guard = w.recovery.lock().unwrap_or_else(|e| e.into_inner());
    *guard = snapshot;
}

/// Guarda qué documentos con ruta quedan abiertos al cerrar la ventana (Ajustes →
/// Archivos → "Reabrir archivos anteriores"), y cuál de ellos tenía el foco. Con el
/// ajuste apagado borra la sesión anterior en vez de dejar una desactualizada.
fn save_session(w: &WindowState) {
    let file = notty_io::session_path();
    if !w.cfg.borrow().ui.reopen_previous {
        notty_io::save_session(&[], 0, &file);
        return;
    }
    let mut paths = Vec::new();
    let mut active = 0;
    for (i, st) in w.ws.iter().enumerate() {
        if let Some(p) = &st.path {
            if i == w.ws.active_index() {
                active = paths.len();
            }
            paths.push(p.clone());
        }
    }
    notty_io::save_session(&paths, active, &file);
}

/// Recuerda el tamaño/maximizado de la ventana para la próxima vez (Ajustes → Ventana
/// → "Restablecer tamaño de ventana" vuelve a `DEFAULT_WIN_W/H`). Si está maximizada,
/// solo se guarda esa bandera: el tamaño restaurado (el de antes de maximizar) no
/// cambia, para no perder el tamaño "normal" al que volver con doble clic en la barra.
fn save_window_geometry(hwnd: HWND, w: &WindowState) {
    unsafe {
        let maximized = IsZoomed(hwnd).as_bool();
        let mut cfg = w.cfg.borrow_mut();
        cfg.ui.win_maximized = maximized;
        if !maximized {
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_ok() {
                let scale = w.renderer.scale();
                let win_w = (rect.right - rect.left) as f32 / scale;
                let win_h = (rect.bottom - rect.top) as f32 / scale;
                if win_w >= layout::MIN_WINDOW_W && win_h >= layout::MIN_WINDOW_H {
                    cfg.ui.win_w = win_w;
                    cfg.ui.win_h = win_h;
                }
            }
        }
        let _ = notty_config::save(&cfg, &notty_config::default_path());
    }
}

/// Instala el `panic hook` de recuperación: si el proceso entra en pánico, vuelca a
/// `notty_io::recovery_dir()` el texto de cada documento sucio del último snapshot
/// leído (ver `refresh_recovery`). Se instala una sola vez, al arrancar `run`.
fn install_recovery_hook(snapshot: &'static RecoverySnapshot) {
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("notty: pánico: {info}");
        let _ = dump_recovery_snapshot(snapshot);
    }));
}

/// Vuelca a `notty_io::recovery_dir()` los documentos sucios del snapshot.
fn dump_recovery_snapshot(snapshot: &RecoverySnapshot) -> std::io::Result<()> {
    let entries: Vec<notty_io::RecoveryEntry> = {
        let guard = snapshot.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .iter()
            .filter(|(_, _, dirty, _)| *dirty)
            .enumerate()
            .map(|(i, (name, text, _, path))| notty_io::RecoveryEntry {
                name: format!("{i}_{name}"),
                text: text.to_string(),
                path: path.clone(),
            })
            .collect()
    };
    notty_io::dump_recovery(&notty_io::recovery_dir(), &entries)
}

/// Vuelca `docs` a la carpeta de recuperación para que vuelvan al abrir notty (cerrar
/// en modo "Recuperar", o Windows cerrando la sesión). El prefijo de tiempo evita
/// pisar volcados anteriores que aún no se han recuperado.
fn dump_docs_to_recovery<'a>(docs: impl Iterator<Item = &'a EditorState>) -> std::io::Result<()> {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let entries: Vec<notty_io::RecoveryEntry> = docs
        .map(|st| notty_io::RecoveryEntry {
            name: format!("{stamp}_{}_{}", st.id, recovery_name(st, st.id as usize)),
            text: st.doc.text(),
            path: st.path.clone(),
        })
        .collect();
    if entries.is_empty() {
        return Ok(());
    }
    notty_io::dump_recovery(&notty_io::recovery_dir(), &entries)
}

/// Abre la ventana principal de notty y bloquea hasta que se cierra.
/// `path` es la ruta pasada por línea de comandos, si la hay; `load` es el resultado
/// de cargar `config.toml` (que puede traer un aviso si el archivo estaba roto).
pub fn run(path: Option<&str>, load: notty_config::LoadResult) -> Result<()> {
    run_with_ipc(path, load, None, notty_update::PUBKEY_PLACEHOLDER_UNSET, "OWNER/notty".to_string())
}

/// Igual que `run`, pero además recibe el extremo receptor del pipe de instancia
/// única (Task 8): cada `notty_ipc::Message::OpenPath` que llegue mientras esta
/// ventana vive se abre como si se hubiera pedido con `Ctrl+O`. `pubkey`/`repo` son
/// la clave Ed25519 contra la que se verifica `notty-setup.exe` y el "owner/repo"
/// de GitHub Releases (Task 4/5 del plan del actualizador): los define `notty`,
/// no `notty-ui` — ver `notty::PUBKEY`/`notty::REPO`.
pub fn run_with_ipc(
    path: Option<&str>,
    load: notty_config::LoadResult,
    ipc_rx: Option<std::sync::mpsc::Receiver<notty_ipc::Message>>,
    pubkey: [u8; 32],
    repo: String,
) -> Result<()> {
    run_inner(path, load, ipc_rx, None, pubkey, repo)
}

/// Variante de `run_with_ipc` usada por `notty --new-temp` (Task 9): abre la ventana
/// directamente con un documento temporal (`EditorState::new_temp`) en vez del vacío
/// de siempre. No tiene ruta de línea de comandos que abrir.
pub fn run_with_temp(
    load: notty_config::LoadResult,
    ipc_rx: Option<std::sync::mpsc::Receiver<notty_ipc::Message>>,
    mode: notty_config::TempMode,
    ext: String,
    pubkey: [u8; 32],
    repo: String,
) -> Result<()> {
    run_inner(None, load, ipc_rx, Some((mode, ext)), pubkey, repo)
}

fn run_inner(
    path: Option<&str>,
    load: notty_config::LoadResult,
    ipc_rx: Option<std::sync::mpsc::Receiver<notty_ipc::Message>>,
    initial_temp: Option<(notty_config::TempMode, String)>,
    pubkey: [u8; 32],
    repo: String,
) -> Result<()> {
    // Snapshot de recuperación: vive el resto del proceso (`Box::leak`) para que el
    // `panic hook`, instalado una sola vez, tenga una dirección `'static` válida.
    crate::bench_log::mark("run_inner");
    let recovery: &'static RecoverySnapshot = Box::leak(Box::new(Mutex::new(Vec::new())));
    install_recovery_hook(recovery);

    let (cfg, broken_msg) = match load {
        notty_config::LoadResult::Loaded(cfg) | notty_config::LoadResult::Missing(cfg) => (cfg, None),
        notty_config::LoadResult::Defaulted(cfg, msg) => (cfg, Some(msg)),
    };

    crate::anim::configure(&cfg.ui);
    crate::shared_theme::reload();
    let run_lang = crate::lang::resolve(cfg.ui.lang);
    let broken_label = crate::strings::tr(run_lang, "config.toml roto");
    let title = match (path, &broken_msg) {
        (Some(p), Some(msg)) => format!("{broken_label}: {msg} — {p} · notty"),
        (Some(p), None) => format!("{p} · notty"),
        (None, Some(msg)) => format!("{broken_label}: {msg} · notty"),
        (None, None) => format!("{} · notty", crate::strings::tr(run_lang, "sin título")),
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
        crate::bench_log::mark("class_registered");

        let (win_w, win_h) = (cfg.ui.win_w.max(layout::MIN_WINDOW_W), cfg.ui.win_h.max(layout::MIN_WINDOW_H));
        let title_wide = to_wide(&title);
        crate::bench_log::mark("before_create_window");
        // Se crea ya al tamaño final con el DPI del sistema (el del monitor principal,
        // donde casi siempre aparece): redimensionarla después costaba ~25 ms de arranque.
        let sys_scale = windows::Win32::UI::HiDpi::GetDpiForSystem() as f32 / 96.0;
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name,
            PCWSTR(title_wide.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            (win_w * sys_scale).round() as i32,
            (win_h * sys_scale).round() as i32,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        crate::bench_log::mark("window_created");

        // Si acabó en un monitor con otro DPI, se ajusta a `win_w`x`win_h` DIPs exactos.
        let scale0 = GetDpiForWindow(hwnd) as f32 / 96.0;
        if scale0 != sys_scale {
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                (win_w * scale0).round() as i32,
                (win_h * scale0).round() as i32,
                SWP_NOMOVE | SWP_NOZORDER,
            );
        }

        let dark = crate::is_dark(crate::shared_theme::effective(&cfg.ui).0, system_uses_dark_mode());
        setup_chrome(hwnd, dark);

        let mut ws = crate::Workspace::new();
        if let Some((mode, ext)) = &initial_temp {
            *ws.active_mut() = maybe_vim(EditorState::new_temp(*mode, ext), &cfg);
        } else if let Some(p) = path {
            *ws.active_mut() = maybe_vim(state_for_path(std::path::Path::new(p)), &cfg);
        } else if cfg.ui.reopen_previous {
            // Sin ningún archivo en la línea de comandos: reabre los que quedaron
            // abiertos la última vez (Ajustes → Archivos), si alguno de ellos todavía
            // existe. La primera línea de la sesión es la que estaba activa.
            let previous: Vec<_> =
                notty_io::load_session(&notty_io::session_path()).into_iter().filter(|p| p.is_file()).collect();
            for (i, p) in previous.iter().enumerate() {
                let state = maybe_vim(state_for_path(p), &cfg);
                if i == 0 {
                    *ws.active_mut() = state;
                } else {
                    ws.open(state);
                }
            }
        }

        // Recuperación (caída, relanzar tras actualizar o cerrar en modo "Recuperar"):
        // cada volcado vuelve como documento pendiente de guardar, con su ruta original
        // si la tenía. Se borra el volcado en cuanto se han recuperado.
        let recovered = notty_io::list_recovery(&notty_io::recovery_dir());
        if !recovered.is_empty() {
            for entry in &recovered {
                let mut st = EditorState::new_empty();
                st.doc = notty_core::Document::new(&entry.text, "\r\n");
                st.doc.mark_unsaved();
                st.path = entry.path.clone();
                ws.open(st);
            }
            let _ = notty_io::clear_recovery(&notty_io::recovery_dir());
        }

        crate::bench_log::mark("docs_loaded");
        let dpi = GetDpiForWindow(hwnd);
        let mut renderer = Renderer::new_software(hwnd, dpi)?;
        crate::syntax::set_repaint_target(hwnd, WM_SYNTAX_READY);
        crate::bench_log::mark("renderer_new");
        let _ = renderer.set_mono_family(cfg.ui.font_family.primary_name());
        let _ = renderer.set_font_scale(cfg.ui.font_scale);

        let total_lines = ws.active().doc.buffer().len_lines();
        let menu_visible0 = cfg.ui.menubar == notty_config::MenuBar::Visible;
        let (body, _gutter_w) = renderer.body_and_gutter(&cfg.ui, ws.len(), menu_visible0, total_lines, ws.active().raw.is_some());
        ws.active_mut().viewport = Viewport::new(renderer.line_height(), body.height());
        update_title(hwnd, ws.active(), run_lang);

        let cfg = Rc::new(RefCell::new(cfg));

        let window_state = Box::new(WindowState {
            ws,
            renderer,
            mouse_down: false,
            selection_anchor: 0,
            scroll_drag: None,
            cfg,
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
            last_bands: None,
            chrome_from: None,
            chrome_fade_anim: None,
            last_dark: None,
            theme_anim: None,
            last_accent: None,
            accent_anim: None,
            anim_timer_running: false,
            gpu_requested: false,
            update: crate::UpdateState::default(),
            download_rx: None,
            manual_check_rx: None,
            pubkey,
            repo: repo.clone(),
            tour: None,
            ctx_menu: None,
            menu_sel: None,
            tab_anims: Default::default(),
            about_open: false,
            whats_new_opened: None,
            path_copied_at: None,
            notice: None,
            swallow_char: false,
            pending_surrogate: None,
            close_confirmed: false,
            layouts: crate::splits::Layouts::default(),
            resize_mode: false,
        });
        let ptr = Box::into_raw(window_state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
        if let Some(w) = ptr.as_ref() {
            refresh_recovery(w);
        }

        crate::bench_log::mark("state_ready");
        let start_maximized = ptr.as_ref().is_some_and(|w| w.cfg.borrow().ui.win_maximized);
        let _ = ShowWindow(hwnd, if start_maximized { SW_MAXIMIZE } else { SW_SHOW });
        let _ = SetTimer(Some(hwnd), ID_AUTOSAVE_TIMER, 1000, None);
        IPC_HWND.store(hwnd.0 as usize, std::sync::atomic::Ordering::Release);
        // Lo que llegase antes de registrar la ventana.
        wake_for_ipc();

        // Recién actualizado: el popup de Novedades, una sola vez por versión.
        if let Some(w) = ptr.as_mut() {
            maybe_show_whats_new(w, hwnd);
        }

        // Primer arranque: la ventana de bienvenida (no la del recorrido directamente,
        // ver Task 4 Step 5 del plan de tutorial) — no modal, flota sobre esta ventana
        // sin bloquear su bucle de mensajes (que es justo el que la va a atender).
        if let Some(w) = ptr.as_ref() {
            if !w.cfg.borrow().first_run_done {
                let cfg_for_welcome = w.cfg.clone();
                let cfg_for_theme = w.cfg.clone();
                let _ = crate::welcome_window::show(
                    hwnd,
                    cfg_for_welcome,
                    Box::new(move || {
                        let dark = crate::is_dark(crate::shared_theme::effective(&cfg_for_theme.borrow().ui).0, system_uses_dark_mode());
                        apply_dark_mode(hwnd, dark);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }),
                    Box::new(move || start_tour(hwnd)),
                );
            }
        }

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
///
/// Además fija `DWMWA_BORDER_COLOR` al `chrome` de la paleta: si no, Windows 11 pinta
/// su propio contorno de 1 px, que cambia al activar/desactivar la ventana (gris
/// claro inactiva, color de acento activa si el usuario lo tiene puesto) y al cambiar
/// el tema — ese era el borde claro que "a veces" aparecía.
pub unsafe fn apply_dark_mode(hwnd: HWND, dark: bool) {
    unsafe {
        let value: i32 = if dark { 1 } else { 0 };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &value as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        );
        // El acento no afecta a `chrome` (el borde de la ventana no lo usa), así que
        // aquí basta con el por defecto en vez de hacer viajar el color elegido hasta
        // esta llamada de DWM.
        let border = colorref(crate::theme::palette(dark, notty_config::AccentColor::default()).chrome);
        let _ = DwmSetWindowAttribute(
            hwnd,
            windows::Win32::Graphics::Dwm::DWMWA_BORDER_COLOR,
            &border as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

/// `Rgba` → `COLORREF` (`0x00BBGGRR`), ignorando el alfa.
fn colorref(c: crate::Rgba) -> u32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    b(c.0) | (b(c.1) << 8) | (b(c.2) << 16)
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
                    // Bandas de esta pasada de pintado, para detectar si un cambio de
                    // ajuste (desde Ajustes u otra vía) acaba de hacer aparecer/
                    // desaparecer una banda entera de la interfaz (barra de menús,
                    // pestañas, atajos, línea de comandos fusionada...) y fundirla en
                    // vez de que salte de golpe (Task 3 del plan de animaciones,
                    // extensión de `start_tab_switch_anim`). Comparación estructural
                    // barata (`Bands` es un puñado de `bool`), una vez por pintado.
                    let bands = Renderer::resolve_bands(&ui, w.ws.len(), w.menu_bar_visible());
                    if let Some(prev) = w.last_bands {
                        if prev != bands {
                            // Si ya había una transición en marcha (hacia `prev`), la nueva
                            // arranca desde el punto exacto en que iba, no desde su final:
                            // pulsar Alt varias veces seguidas invierte la animación sin saltos.
                            let now = std::time::Instant::now();
                            let current = match (w.chrome_from, w.chrome_fade_anim) {
                                (Some(from), Some(a)) => from.lerp(layout::BandFrac::of(prev), a.value(now, 0.0, 1.0)),
                                _ => layout::BandFrac::of(prev),
                            };
                            w.chrome_from = Some(current);
                            w.chrome_fade_anim =
                                Some(crate::Anim::new_maybe(now, std::time::Duration::from_millis(300), w.animations_enabled));
                            ensure_anim_timer(w, hwnd);
                        }
                    }
                    w.last_bands = Some(bands);
                    // `visible_lines` depende del alto del cuerpo, que cambia no solo con
                    // `WM_SIZE` sino al aparecer/desaparecer bandas o al cambiar de pestaña.
                    let (body, _) = w.body_and_gutter();
                    let visible = layout::visible_lines(body, w.renderer.line_height());
                    w.ws.active_mut().viewport.visible_lines = visible;
                    if w.splits_on() {
                        w.sync_layout();
                        let panes = w.layouts.panes().to_vec();
                        for &doc in &panes {
                            if let Some(st) = w.ws.get_mut(doc) {
                                st.viewport.visible_lines = visible;
                            }
                        }
                        if panes.len() < 2 {
                            w.resize_mode = false;
                        }
                        w.renderer.set_panes(crate::PaneView {
                            docs: panes,
                            weights: w.layouts.weights().to_vec(),
                            focus: w.layouts.focus(),
                            resize_mode: w.resize_mode,
                            tabs: Some(w.layouts.tab_groups()),
                        });
                    } else {
                        w.resize_mode = false;
                        w.renderer.set_panes(crate::PaneView::default());
                    }
                    let dark = crate::is_dark(ui.theme, system_uses_dark_mode());
                    if let Some(prev) = w.last_dark {
                        if prev != dark {
                            let from = match w.theme_anim {
                                // A medio fundido: se parte del tema más cercano a lo que se ve.
                                Some((f, a)) if a.value(std::time::Instant::now(), 0.0, 1.0) < 0.5 => f,
                                _ => prev,
                            };
                            w.theme_anim = Some((
                                from,
                                crate::Anim::new_maybe(
                                    std::time::Instant::now(),
                                    std::time::Duration::from_millis(350),
                                    w.animations_enabled,
                                ),
                            ));
                            ensure_anim_timer(w, hwnd);
                        }
                    }
                    w.last_dark = Some(dark);
                    let accent = ui.accent;
                    if let Some(prev) = w.last_accent {
                        if prev != accent {
                            let from = match w.accent_anim {
                                Some((f, a)) if a.value(std::time::Instant::now(), 0.0, 1.0) < 0.5 => f,
                                _ => prev,
                            };
                            w.accent_anim = Some((
                                from,
                                crate::Anim::new_maybe(
                                    std::time::Instant::now(),
                                    std::time::Duration::from_millis(350),
                                    w.animations_enabled,
                                ),
                            ));
                            ensure_anim_timer(w, hwnd);
                        }
                    }
                    w.last_accent = Some(accent);
                    let view = w.view_state(hwnd);
                    let lang = lang_of(w);
                    w.renderer.set_update_notice(w.update.notice_text(lang));
                    w.renderer.set_update_panel(update_panel_content(&w.update, lang));
                    w.renderer.set_context_menu(w.ctx_menu.clone());
                    if w.open_menu.is_some() {
                        w.renderer.set_menu_keys(crate::menu::shortcut_labels(&w.cfg.borrow()));
                    }
                    w.renderer.set_tab_anim(w.tab_anims.frame(std::time::Instant::now()));
                    w.renderer.set_about(if w.about_open { Some(about_content(&w.repo)) } else { None });
                    w.renderer.set_whats_new(w.whats_new_opened.and_then(|opened| {
                        let r = crate::whats_new::release_for(crate::app_version())?;
                        Some(crate::WhatsNewView { version: r.version, items: r.items, opened, animate: w.animations_enabled })
                    }));
                    w.renderer.set_path_copied(path_copied_progress(w));
                    w.renderer.set_notice(notice_progress(w));
                    if w.tour.is_some() {
                        w.renderer.hold_next_frame();
                    }
                    let ligature_table = {
                        let cfg = w.cfg.borrow();
                        w.renderer.set_syntax_disabled(&cfg.syntax_disabled);
                        crate::ligature::resolve(&cfg.ligature_overrides, &cfg.ligature_disabled)
                    };
                    let paint_start = std::time::Instant::now();
                    w.renderer.paint(&w.ws, &ui, &view, &ligature_table);
                    if w.tour.is_some() {
                        // Se pinta último (capa por encima de todo lo demás), en una
                        // segunda pasada de dibujo sobre el mismo `ID2D1HwndRenderTarget`
                        // (`begin_overlay` no repite el `Clear`, así que no borra lo que
                        // `paint` ya dejó).
                        let frame = w.renderer.current_frame(&ui, w.ws.len(), w.menu_bar_visible());
                        let tour = w.tour.as_mut().expect("comprobado con is_some justo arriba");
                        tour.draw(&w.renderer, &frame, view.dark, ui.accent, std::time::Instant::now());
                    }
                    crate::bench_log::record_paint(paint_start.elapsed());
                    if w.renderer.is_software() && !w.gpu_requested {
                        w.gpu_requested = true;
                        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(Some(hwnd), WM_GPU_READY, WPARAM(0), LPARAM(0));
                    }
                }
                let _ = ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }
            WM_SIZE => {
                if let Some(w) = ptr.as_mut() {
                    if wparam.0 as u32 == SIZE_MINIMIZED {
                        return LRESULT(0);
                    }
                    let width = (lparam.0 as u32) & 0xFFFF;
                    let height = ((lparam.0 as u32) >> 16) & 0xFFFF;
                    w.ctx_menu = None;
                    w.renderer.resize(width, height);
                    let (body, _gutter_w) = w.body_and_gutter();
                    w.ws.active_mut().viewport.visible_lines = layout::visible_lines(body, w.renderer.line_height());
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                // Tamaño mínimo en DIPs, escalado al DPI del monitor en el que está.
                let info = &mut *(lparam.0 as *mut MINMAXINFO);
                let dpi = GetDpiForWindow(hwnd);
                let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };
                info.ptMinTrackSize.x = (layout::MIN_WINDOW_W * scale).round() as i32;
                info.ptMinTrackSize.y = (layout::MIN_WINDOW_H * scale).round() as i32;
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
                    if !w.active_window {
                        w.ctx_menu = None;
                        w.menu_sel = None;
                    }
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
                    let dark = crate::is_dark(w.render_ui().theme, system_uses_dark_mode());
                    apply_dark_mode(hwnd, dark);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KEYDOWN => {
                if let Some(w) = ptr.as_mut() {
                    let vk = wparam.0 as u32;

                    // Con el recorrido activo, Esc lo cierra; el resto de teclas siguen
                    // hasta la app para que los atajos que enseña funcionen de verdad.
                    if let Some(tour) = w.tour.as_mut() {
                        if tour.handle_key(vk) == crate::tour::TourInput::Closed {
                            w.tour = None;
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }

                    if w.resize_mode {
                        let shift = (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
                        handle_resize_key(w, vk, shift);
                        w.swallow_char = true;
                        update_title(hwnd, w.ws.active(), lang_of(w));
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }

                    let alt_down = (GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0;
                    let mods = Modifiers {
                        ctrl: (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0,
                        shift: (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0,
                        alt: alt_down,
                    };

                    if handle_menu_key(w, hwnd, vk) {
                        return LRESULT(0);
                    }
                    if w.whats_new_opened.is_some() && (vk == 0x1B || vk == 0x0D) {
                        w.whats_new_opened = None;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if w.about_open && (vk == 0x1B || vk == 0x0D) {
                        w.about_open = false;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    // Tecla Menú (VK_APPS): menú contextual bajo el cursor de texto. Si
                    // Windows manda además `WM_CONTEXTMENU`, allí se ignora (ya abierto).
                    if vk == 0x5D {
                        open_context_menu_at_caret(w, hwnd);
                        return LRESULT(0);
                    }

                    if !matches!(w.ws.prompt, crate::Prompt::None) {
                        handle_prompt_keydown(w, hwnd, vk, mods);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }

                    let ui_mods = notty_input::Modifiers { ctrl: mods.ctrl, shift: mods.shift, alt: alt_down };
                    // Atajos reasignables: se leen de `[keys]` en vivo en cada pulsación, así
                    // un cambio hecho en Ajustes → Teclado vale al instante.
                    let splits_on = w.splits_on();
                    let remapped = notty_input::command_for_key(&w.cfg.borrow(), vk, ui_mods)
                        .filter(|c| splits_on || !c.is_pane());
                    if let Some(cmd) = remapped {
                        match cmd {
                            notty_input::Command::NewTab => {
                                let cfg = w.cfg.borrow().clone();
                                open_tab(w, hwnd, maybe_vim(crate::EditorState::new_empty(), &cfg));
                            }
                            notty_input::Command::ToggleVim => {
                                let st = w.ws.active_mut();
                                st.vim = if st.vim.is_some() { None } else { Some(crate::VimState::default()) };
                            }
                            notty_input::Command::ToggleRaw => toggle_raw(w, hwnd),
                            notty_input::Command::ToggleMdPreview => toggle_md_preview(w),
                            notty_input::Command::ZoomIn => {
                                let target = w.cfg.borrow().ui.font_scale + ZOOM_STEP;
                                set_font_scale(w, hwnd, target);
                            }
                            notty_input::Command::ZoomOut => {
                                let target = w.cfg.borrow().ui.font_scale - ZOOM_STEP;
                                set_font_scale(w, hwnd, target);
                            }
                            notty_input::Command::ZoomReset => set_font_scale(w, hwnd, 1.0),
                            notty_input::Command::NewTempTab => {
                                let cfg = w.cfg.borrow().clone();
                                open_tab(
                                    w,
                                    hwnd,
                                    maybe_vim(
                                        crate::EditorState::new_temp(cfg.files.temp_mode, &cfg.files.default_extension),
                                        &cfg,
                                    ),
                                );
                            }
                            notty_input::Command::NextTab => step_tab(w, hwnd, 1),
                            notty_input::Command::PrevTab => step_tab(w, hwnd, -1),
                            notty_input::Command::CloseTab => {
                                let docs = tab_docs(w, w.ws.active_index());
                                request_close(w, hwnd, &docs, false);
                            }
                            notty_input::Command::MoveTabLeft | notty_input::Command::MoveTabRight => {
                                let dir = if cmd == notty_input::Command::MoveTabLeft { -1 } else { 1 };
                                if w.splits_on() {
                                    w.sync_layout();
                                    w.layouts.move_tab(dir);
                                } else {
                                    w.ws.move_active(dir);
                                }
                            }
                            notty_input::Command::SwapPaneLeft | notty_input::Command::SwapPaneRight => {
                                w.sync_layout();
                                w.layouts.swap_pane(if cmd == notty_input::Command::SwapPaneLeft { -1 } else { 1 });
                            }
                            notty_input::Command::ResizePanes => {
                                w.sync_layout();
                                if w.layouts.panes().len() > 1 {
                                    w.resize_mode = true;
                                } else {
                                    show_notice(w, hwnd, tr_w(w, "Para acomodar hacen falta dos paneles (Ctrl+Shift+Enter divide)").to_string());
                                }
                            }
                            notty_input::Command::OpenSettings => open_settings(w, hwnd),
                            notty_input::Command::SplitPane => split_pane(w, hwnd),
                            notty_input::Command::ClosePane => {
                                let i = w.ws.active_index();
                                request_close(w, hwnd, &[i], false);
                            }
                            notty_input::Command::FocusPaneLeft | notty_input::Command::FocusPaneRight => {
                                w.sync_layout();
                                let dir = if cmd == notty_input::Command::FocusPaneLeft { -1 } else { 1 };
                                if let Some(doc) = w.layouts.move_focus(dir) {
                                    w.ws.activate(doc);
                                }
                            }
                        }
                        update_title(hwnd, w.ws.active(), lang_of(w));
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    // Para moverse, fijos (no se reasignan): Ctrl+PgDn/PgUp y Ctrl+1…9.
                    if mods.ctrl && !mods.shift && !alt_down {
                        let handled = match vk {
                            0x22 => {
                                step_tab(w, hwnd, 1);
                                true
                            }
                            0x21 => {
                                step_tab(w, hwnd, -1);
                                true
                            }
                            0x31..=0x39 => {
                                goto_tab(w, hwnd, (vk - 0x30) as usize);
                                true
                            }
                            _ => false,
                        };
                        if handled {
                            update_title(hwnd, w.ws.active(), lang_of(w));
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                    }
                    // Vim/raw ya se resolvieron arriba con su atajo en vigor: el fijo de
                    // `action_for_vk` (Ctrl+Alt+V / Ctrl+Shift+H) no debe seguir
                    // disparando tras reasignarlos.
                    let action = match crate::action_for_vk(vk, mods) {
                        crate::EditorAction::ToggleVim | crate::EditorAction::ToggleRaw => crate::EditorAction::None,
                        a => a,
                    };

                    if w.ws.active().raw.is_some() {
                        handle_raw_keydown(w, hwnd, vk, action);
                        update_title(hwnd, w.ws.active(), lang_of(w));
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
                            clipboard_copy(w, hwnd, matches!(action, crate::EditorAction::Cut));
                            update_title(hwnd, w.ws.active(), lang_of(w));
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::EditorAction::Paste => {
                            if !md_preview_readonly_blocks_edits(w) {
                                clipboard_paste(w, hwnd);
                                update_title(hwnd, w.ws.active(), lang_of(w));
                                let _ = InvalidateRect(Some(hwnd), None, false);
                            }
                        }
                        crate::EditorAction::OpenPathPrompt => {
                            let initial = w.ws.active().path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                            if start_path_entry(w, hwnd, crate::Purpose::Open, initial) {
                                start_popup_anim(w, hwnd);
                            }
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
                            if start_path_entry(w, hwnd, crate::Purpose::Save, String::new()) {
                                start_popup_anim(w, hwnd);
                            }
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::EditorAction::FindNext | crate::EditorAction::FindPrev => {
                            // Fuera de un prompt de búsqueda activo (ya cubierto arriba, antes de
                            // llegar aquí), F3 no tiene una búsqueda que repetir: no hace nada.
                        }
                        crate::EditorAction::Save => {
                            save_now(w, hwnd);
                            update_title(hwnd, w.ws.active(), lang_of(w));
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        other => {
                            let edits = matches!(
                                other,
                                crate::EditorAction::Backspace
                                    | crate::EditorAction::DeleteForward
                                    | crate::EditorAction::InsertNewline
                                    | crate::EditorAction::Undo
                                    | crate::EditorAction::Redo
                            );
                            if !edits || !md_preview_readonly_blocks_edits(w) {
                                w.ws.active_mut().apply(other, std::time::Instant::now());
                                update_title(hwnd, w.ws.active(), lang_of(w));
                                refresh_recovery(w);
                                let _ = InvalidateRect(Some(hwnd), None, false);
                            }
                        }
                    }
                }
                LRESULT(0)
            }
            WM_CHAR => {
                if let Some(w) = ptr.as_mut() {
                    if std::mem::take(&mut w.swallow_char) {
                        return LRESULT(0);
                    }
                    if let Some(ch) = crate::char_from_utf16_unit(&mut w.pending_surrogate, wparam.0 as u16) {
                        if !matches!(w.ws.prompt, crate::Prompt::None) {
                            handle_prompt_char(w, ch);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                        if w.ws.active().raw.is_some() {
                            handle_raw_char(w, ch);
                            update_title(hwnd, w.ws.active(), lang_of(w));
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                        if w.ws.active().vim.is_some() {
                            handle_vim_char(w, ch);
                            update_title(hwnd, w.ws.active(), lang_of(w));
                            refresh_recovery(w);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                        if md_preview_readonly_blocks_edits(w) {
                            return LRESULT(0);
                        }
                        w.ws.active_mut().insert_char(ch, std::time::Instant::now());
                        update_title(hwnd, w.ws.active(), lang_of(w));
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

                    // Con el recorrido activo, los clics en su globo son suyos; el resto
                    // llegan a la app como siempre (ya no lo cierra un clic cualquiera).
                    if let Some(tour) = w.tour.as_mut() {
                        match tour.handle_click(x, y, std::time::Instant::now()) {
                            crate::tour::TourInput::Pass => {}
                            crate::tour::TourInput::Closed => {
                                w.tour = None;
                                let _ = InvalidateRect(Some(hwnd), None, false);
                                return LRESULT(0);
                            }
                            crate::tour::TourInput::None => {
                                ensure_anim_timer(w, hwnd);
                                let _ = InvalidateRect(Some(hwnd), None, false);
                                return LRESULT(0);
                            }
                        }
                    }

                    let mut hit = w.renderer.hit(x, y);

                    // Con un menú contextual abierto, un clic fuera solo lo cierra (se
                    // traga, como los menús de Windows); dentro, ejecuta el elemento.
                    if w.ctx_menu.is_some() {
                        match hit {
                            Hit::CtxItem(j) => activate_menu_row(w, hwnd, j),
                            Hit::PopupBox => {}
                            _ => close_menus(w),
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if w.whats_new_opened.is_some() {
                        if hit != Hit::PopupBox {
                            w.whats_new_opened = None;
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if w.about_open {
                        match hit {
                            Hit::AboutLink => {
                                shell_open(hwnd, &about_content(&w.repo).url.unwrap_or_default(), None);
                            }
                            Hit::PopupBox => {}
                            _ => w.about_open = false,
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }

                    // Clic en otro panel: primero se le da el foco, y el clic sigue hacia
                    // el documento de ese panel.
                    if matches!(hit, Hit::Body | Hit::ScrollThumb | Hit::ScrollTrack) {
                        if let Some(i) = w.pane_at(x, y).filter(|&i| i != w.layouts.focus()) {
                            focus_pane(w, i);
                            update_title(hwnd, w.ws.active(), lang_of(w));
                            // Las barras de scroll registradas son de antes del cambio de
                            // foco: ese clic solo enfoca.
                            if hit != Hit::Body {
                                hit = Hit::None;
                            }
                        }
                    }

                    match hit {
                        Hit::Min | Hit::Close => {
                            w.pressed = hit;
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        Hit::PopupBox => {}
                        Hit::TabScrollLeft | Hit::TabScrollRight => {
                            if let Some(i) = w.renderer.tab_scroll_target(hit == Hit::TabScrollLeft) {
                                w.ws.activate(i);
                                start_tab_switch_anim(w, hwnd);
                                update_title(hwnd, w.ws.active(), lang_of(w));
                            }
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        Hit::CloseChoice(k) => {
                            let choice = match k {
                                0 => crate::CloseChoice::Save,
                                1 => crate::CloseChoice::Discard,
                                _ => crate::CloseChoice::Cancel,
                            };
                            answer_close(w, hwnd, choice);
                            update_title(hwnd, w.ws.active(), lang_of(w));
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        Hit::Overwrite(k) => {
                            let choice = match k {
                                0 => crate::OverwriteChoice::Overwrite,
                                1 => crate::OverwriteChoice::OpenExisting,
                                _ => crate::OverwriteChoice::Cancel,
                            };
                            answer_overwrite(w, hwnd, choice);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::Pencil if w.ws.active().raw.is_some() => {
                            if let Some(raw) = w.ws.active_mut().raw.as_mut() {
                                raw.enable_write();
                            }
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::Clickme => {
                            if start_path_entry(w, hwnd, crate::Purpose::Save, String::new()) {
                                start_popup_anim(w, hwnd);
                            }
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::Settings => open_settings(w, hwnd),
                        crate::Hit::UpdateNotice => {
                            w.update.toggle_panel();
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::UpdatePanelCerrar => {
                            w.update.close_panel();
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::UpdatePanelActualizar => {
                            start_update_download(w, hwnd);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
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
                            let docs = tab_docs(w, i);
                            request_close(w, hwnd, &docs, false);
                            update_title(hwnd, w.ws.active(), lang_of(w));
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::NewTab => {
                            let cfg = w.cfg.borrow().clone();
                            open_tab(w, hwnd, maybe_vim(crate::EditorState::new_empty(), &cfg));
                            update_title(hwnd, w.ws.active(), lang_of(w));
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::Menu(i) => {
                            w.open_menu = if w.open_menu == Some(i) { None } else { Some(i) };
                            w.menu_sel = None;
                            if w.open_menu.is_some() {
                                start_popup_anim(w, hwnd);
                            }
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::MenuItem(j) => {
                            activate_menu_row(w, hwnd, j);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::Body if w.ws.active().raw.is_none() => {
                            w.open_menu = None;
                            let (body, gutter_w) = w.body_and_gutter();
                            let idx = w.renderer.char_index_at(w.ws.active(), body, gutter_w, x, y, w.cfg.borrow().ui.wrap, w.cfg.borrow().ui.md_preview_style == notty_config::MdPreviewStyle::Inline);
                            w.ws.active_mut().doc.set_cursor(idx);
                            w.selection_anchor = idx;
                            w.mouse_down = true;
                            SetCapture(hwnd);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::ScrollThumb => {
                            w.scroll_drag = Some((y, w.ws.active().viewport.first_line));
                            SetCapture(hwnd);
                        }
                        crate::Hit::ScrollTrack => {
                            let (body, _) = w.body_and_gutter();
                            let total_rows = w.total_rows();
                            let page = w.ws.active().viewport.visible_lines as i32;
                            let thumb_top = Renderer::editor_scrollbar_geom(body, &w.ws.active().viewport, total_rows)
                                .map(|(_, thumb)| thumb.top)
                                .unwrap_or(y);
                            w.ws.active_mut().scroll_by(if y < thumb_top { -page } else { page });
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
                    if let Some((y0, first0)) = w.scroll_drag {
                        let (body, _) = w.body_and_gutter();
                        let total_rows = w.total_rows();
                        if let Some((track, thumb)) =
                            Renderer::editor_scrollbar_geom(body, &w.ws.active().viewport, total_rows)
                        {
                            let free = (track.height() - thumb.height()).max(1.0);
                            let max_first = total_rows.saturating_sub(w.ws.active().viewport.visible_lines) as f32;
                            let new_first = (first0 as f32 + (y - y0) * max_first / free).round().clamp(0.0, max_first);
                            w.ws.active_mut().viewport.first_line = new_first as usize;
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    if w.mouse_down {
                        let (body, gutter_w) = w.body_and_gutter();
                        let idx = w.renderer.char_index_at(w.ws.active(), body, gutter_w, x, y, w.cfg.borrow().ui.wrap, w.cfg.borrow().ui.md_preview_style == notty_config::MdPreviewStyle::Inline);
                        w.ws.active_mut().doc.set_selection(w.selection_anchor, idx);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    let hit = w.renderer.hit(x, y);
                    if hit != w.hover {
                        w.hover = hit;
                        // El resaltado del teclado sigue al ratón, para que las flechas
                        // continúen desde el elemento señalado.
                        if let Hit::CtxItem(j) | Hit::MenuItem(j) = hit {
                            w.menu_sel = Some(j);
                        }
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
                    if w.scroll_drag.take().is_some() {
                        let _ = ReleaseCapture();
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
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
                                WM_CLOSE,
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
            WM_RBUTTONDOWN => {
                if let Some(w) = ptr.as_mut() {
                    let (x, y) = point_from_lparam(lparam);
                    let scale = w.renderer.scale();
                    let (x, y) = (x / scale, y / scale);
                    let hit = w.renderer.hit(x, y);
                    if !matches!(hit, Hit::CtxItem(_) | Hit::PopupBox) {
                        close_menus(w);
                        w.about_open = false;
                    }
                    // Como en cualquier editor: clic derecho fuera de la selección mueve
                    // el cursor ahí; dentro de ella, la conserva (para copiar/buscar).
                    if hit == Hit::Body {
                        if let Some(i) = w.pane_at(x, y).filter(|&i| i != w.layouts.focus()) {
                            focus_pane(w, i);
                            update_title(hwnd, w.ws.active(), lang_of(w));
                        }
                    }
                    if hit == Hit::Body && w.ws.active().raw.is_none() {
                        let (body, gutter_w) = w.body_and_gutter();
                        let idx = w.renderer.char_index_at(w.ws.active(), body, gutter_w, x, y, w.cfg.borrow().ui.wrap, w.cfg.borrow().ui.md_preview_style == notty_config::MdPreviewStyle::Inline);
                        let sel = w.ws.active().doc.selection();
                        let r = sel.range();
                        if sel.is_empty() || idx < r.start || idx > r.end {
                            w.ws.active_mut().doc.set_cursor(idx);
                            w.selection_anchor = idx;
                        }
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            // `WM_RBUTTONUP` se deja a `DefWindowProcW`, que manda `WM_CONTEXTMENU`.
            WM_RBUTTONUP => DefWindowProcW(hwnd, msg, wparam, lparam),
            WM_CONTEXTMENU => {
                if let Some(w) = ptr.as_mut() {
                    let sx = (lparam.0 as i16) as i32;
                    let sy = ((lparam.0 >> 16) as i16) as i32;
                    if sx == -1 && sy == -1 {
                        // Teclado (Shift+F10 / tecla Menú): si ya se abrió desde
                        // `WM_KEYDOWN`/`WM_SYSKEYDOWN`, no se vuelve a abrir.
                        if w.ctx_menu.is_none() {
                            open_context_menu_at_caret(w, hwnd);
                        }
                    } else {
                        let mut pt = windows::Win32::Foundation::POINT { x: sx, y: sy };
                        let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
                        let scale = w.renderer.scale();
                        let (x, y) = (pt.x as f32 / scale, pt.y as f32 / scale);
                        match w.renderer.hit(x, y) {
                            Hit::Tab(i) | Hit::TabClose(i) => open_context_menu(w, hwnd, CtxTarget::Tab(i), x, y),
                            Hit::Body if w.ws.active().raw.is_none() => open_context_menu(w, hwnd, CtxTarget::Body, x, y),
                            _ => {}
                        }
                    }
                }
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
                    w.ctx_menu = None;
                    if let crate::Prompt::Path(p) = &mut w.ws.prompt {
                        p.scroll_by(-notches);
                    } else {
                        // La rueda desplaza el panel bajo el ratón, sin moverle el foco.
                        let mut pt = windows::Win32::Foundation::POINT {
                            x: (lparam.0 as i16) as i32,
                            y: ((lparam.0 >> 16) as i16) as i32,
                        };
                        let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
                        let scale = w.renderer.scale();
                        w.sync_layout();
                        let doc = w
                            .pane_at(pt.x as f32 / scale, pt.y as f32 / scale)
                            .and_then(|i| w.layouts.panes().get(i).copied())
                            .unwrap_or(w.ws.active_index());
                        if let Some(st) = w.ws.get_mut(doc) {
                            st.scroll_by(-notches * 3);
                        }
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
                    // Alt+1…9: panel N de la pestaña (modo Paneles).
                    if (0x31..=0x39).contains(&vk) && w.splits_on() && (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) == 0 {
                        focus_pane(w, (vk - 0x31) as usize);
                        update_title(hwnd, w.ws.active(), lang_of(w));
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    // Shift+F10 (F10 llega siempre como tecla de sistema).
                    let shift = (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
                    if vk == 0x79 && shift {
                        open_context_menu_at_caret(w, hwnd);
                        return LRESULT(0);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_IPC => {
                if let Some(w) = ptr.as_mut() {
                    if ipc_tick(w, hwnd) {
                        update_title(hwnd, w.ws.active(), lang_of(w));
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_SYNTAX_READY => {
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_GPU_READY => {
                if let Some(w) = ptr.as_mut() {
                    if w.renderer.upgrade_to_gpu() {
                        crate::bench_log::mark("gpu");
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == ID_AUTOSAVE_TIMER {
                    if let Some(w) = ptr.as_mut() {
                        // Sin repintar si no cambió nada: en reposo, la ventana no se despierta a dibujar.
                        if autosave_tick(w, hwnd) {
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
                        if w.chrome_fade_anim.is_some_and(|a| a.is_done(now)) {
                            w.chrome_fade_anim = None;
                            w.chrome_from = None;
                        }
                        if w.theme_anim.is_some_and(|(_, a)| a.is_done(now)) {
                            w.theme_anim = None;
                        }
                        if w.accent_anim.is_some_and(|(_, a)| a.is_done(now)) {
                            w.accent_anim = None;
                        }
                        w.tab_anims.prune(now);
                        let tour_animating = w.tour.as_ref().is_some_and(|t| t.is_animating(now));
                        let still_animating = w.tab_anims.is_animating()
                            || w.popup_open_anim.is_some()
                            || w.tab_switch_anim.is_some()
                            || w.chrome_fade_anim.is_some()
                            || w.theme_anim.is_some()
                            || w.accent_anim.is_some()
                            || tour_animating
                            || w.whats_new_opened.is_some()
                            || path_copied_progress(w).is_some()
                            || notice_progress(w).is_some();
                        if !still_animating {
                            crate::anim::stop_frame_timer(hwnd, ID_ANIM_TIMER);
                            w.anim_timer_running = false;
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_CLOSE => {
                if let Some(w) = ptr.as_mut() {
                    if !w.close_confirmed {
                        let all: Vec<usize> = (0..w.ws.len()).collect();
                        request_close(w, hwnd, &all, true);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            // Windows cierra la sesión sin mandar `WM_CLOSE`: lo no guardado se vuelca a
            // recuperación (sea cual sea el modo) para no perderlo.
            WM_ENDSESSION => {
                if wparam.0 != 0 {
                    if let Some(w) = ptr.as_ref() {
                        let _ = dump_docs_to_recovery(w.ws.iter().filter(|st| crate::has_unsaved(st)));
                    }
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                if let Some(w) = ptr.as_ref() {
                    save_session(w);
                    save_window_geometry(hwnd, w);
                }
                let _ = KillTimer(Some(hwnd), ID_AUTOSAVE_TIMER);
                IPC_HWND.store(0, std::sync::atomic::Ordering::Release);
                crate::anim::stop_frame_timer(hwnd, ID_ANIM_TIMER);
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
            open_tab(w, hwnd, maybe_vim(crate::EditorState::new_empty(), &cfg));
        }
        MenuCmd::Open => {
            let initial = w.ws.active().path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
            if start_path_entry(w, hwnd, crate::Purpose::Open, initial) {
                start_popup_anim(w, hwnd);
            }
        }
        MenuCmd::Save => {
            if w.ws.active().path.is_none() {
                if start_path_entry(w, hwnd, crate::Purpose::Save, String::new()) {
                    start_popup_anim(w, hwnd);
                }
            } else {
                save_now(w, hwnd);
            }
        }
        MenuCmd::SaveAs => {
            if start_path_entry(w, hwnd, crate::Purpose::Save, String::new()) {
                start_popup_anim(w, hwnd);
            }
        }
        MenuCmd::Settings => open_settings(w, hwnd),
        MenuCmd::CloseTab => {
            let i = w.ws.active_index();
            request_close(w, hwnd, &[i], false);
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
        MenuCmd::ToggleRaw => toggle_raw(w, hwnd),
        MenuCmd::ToggleMdPreview => toggle_md_preview(w),
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
            open_tab(w, hwnd, maybe_vim(crate::EditorState::new_temp(cfg.files.temp_mode, &cfg.files.default_extension), &cfg));
        }
        // `settings_window::open` todavía no sabe abrir directamente una sección
        // concreta (Teclado): de momento abre Ajustes por el principio.
        MenuCmd::Shortcuts => open_settings_at(w, hwnd, "teclado"),
        MenuCmd::About => w.about_open = true,
        MenuCmd::WhatsNew => open_whats_new(w, hwnd),
    }
    unsafe {
        update_title(hwnd, w.ws.active(), lang_of(w));
    }
}

// --- Vista raw: teclado --------------------------------------------------------------

/// Cambia entre la vista de texto normal y la vista raw (`Ctrl+Shift+H`). Si el
/// contenido de la vista raw sigue siendo UTF-8 válido al volver a texto, se reconstruye
/// el `Document`; si no, se queda en raw (no hay forma segura de mostrarlo como texto).
fn toggle_raw(w: &mut WindowState, hwnd: HWND) {
    // Entrar y salir de raw recarga desde disco: con cambios sin guardar se perderían.
    let st = w.ws.active();
    if st.raw.as_ref().is_some_and(|r| r.is_dirty()) {
        show_notice(w, hwnd, tr_w(w, "Guarda antes de salir de raw"));
        return;
    }
    if st.raw.is_none() && st.doc.is_dirty() {
        show_notice(w, hwnd, tr_w(w, "Guarda antes de ver como raw"));
        return;
    }
    let st = w.ws.active_mut();
    if let Some(raw) = st.raw.take() {
        let reopened = st.path.as_deref().map(crate::open_as_document);
        if let Some(Ok(opened)) = reopened {
            st.doc = opened.document;
            st.encoding = opened.encoding;
            st.eol = opened.eol;
            st.open_mtime = notty_io::mtime(&opened.path).ok();
            st.lossy_source = opened.lossy.then(|| opened.path.clone());
            // `first_line` venía contando filas hex, no líneas.
            st.viewport.first_line = 0;
        } else {
            st.raw = Some(raw);
        }
    } else if let Some(path) = st.path.clone() {
        if let Ok(raw) = crate::open_raw_doc(&path) {
            st.raw = Some(raw);
            st.raw_cursor = 0;
            st.raw_pending_nibble = None;
            st.viewport.first_line = 0;
        }
    }
}

/// A diferencia de `toggle_raw`, Previsualización no lee/escribe nada del disco: es el
/// mismo `Document` de siempre, solo cambia cómo se dibuja (`render.rs`) y, con el
/// estilo "Solo lectura" (Ajustes → Archivos), si se puede escribir en él o no.
fn toggle_md_preview(w: &mut WindowState) {
    let st = w.ws.active_mut();
    st.md_preview = !st.md_preview;
}

/// Si Previsualización (estilo "Solo lectura") bloquea la edición del documento activo
/// ahora mismo: se deja escribir con normalidad en el estilo "En línea" (siempre, la
/// línea con el cursor se ve y edita en markdown crudo) y fuera de Previsualización.
fn md_preview_readonly_blocks_edits(w: &WindowState) -> bool {
    w.ws.active().md_preview && w.cfg.borrow().ui.md_preview_style == notty_config::MdPreviewStyle::ReadOnly
}

/// Flechas (mueven el byte seleccionado) y `Ctrl+S` (guarda) mientras hay un `RawDoc` activo.
fn handle_raw_keydown(w: &mut WindowState, hwnd: HWND, vk: u32, action: crate::EditorAction) {
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
        let new_cursor = (cur + delta).clamp(0, len as i64 - 1) as usize;
        let st = w.ws.active_mut();
        st.raw_cursor = new_cursor;
        st.raw_pending_nibble = None;
        let rows = len.div_ceil(16).max(1);
        st.viewport.scroll_to_include(new_cursor / 16, rows);
        return;
    }
    if matches!(action, crate::EditorAction::Save) {
        if let Some(Err(e)) = w.ws.active_mut().raw.as_mut().map(|raw| raw.save()) {
            show_notice(w, hwnd, format!("{}: {}", tr_w(w, "No se pudo guardar"), tr_w_owned(w, &e.to_string())));
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
    if matches!(w.ws.prompt, crate::Prompt::CloseUnsaved(_)) {
        if let Some(choice) = crate::CloseChoice::from_vk(vk) {
            w.swallow_char = true;
            answer_close(w, hwnd, choice);
            unsafe { update_title(hwnd, w.ws.active(), lang_of(w)) };
        }
        return;
    }
    // Pregunta "ya existe" pendiente: solo cuentan sus teclas (Esc = cancelar la
    // pregunta, no cerrar el prompt entero).
    if let crate::Prompt::Path(p) = &w.ws.prompt {
        if p.ask_overwrite {
            if let Some(choice) = crate::OverwriteChoice::from_vk(vk) {
                // La S/A/C también genera su `WM_CHAR`: que no acabe escrita en el
                // documento (o en la ruta) una vez respondida la pregunta.
                w.swallow_char = true;
                answer_overwrite(w, hwnd, choice);
            }
            return;
        }
    }
    if matches!(w.ws.prompt, crate::Prompt::Conflict(_)) {
        handle_conflict_key(w, hwnd, vk);
        return;
    }
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
    } else if let crate::Prompt::Conflict(c) = &mut w.ws.prompt {
        c.type_char(ch, std::time::Instant::now());
    }
}

fn handle_path_char(w: &mut WindowState, ch: char) {
    let ctx = path_ctx(w);
    if let crate::Prompt::Path(p) = &mut w.ws.prompt {
        if !p.ask_overwrite {
            p.type_char(ch, &ctx);
        }
    }
}

fn handle_path_backspace(w: &mut WindowState) {
    let ctx = path_ctx(w);
    if let crate::Prompt::Path(p) = &mut w.ws.prompt {
        p.backspace(&ctx);
    }
}

/// Ctrl+A/C/X/V/Z sobre la línea de ruta (el `WM_CHAR` de control que generan se
/// descarta en `handle_prompt_char`). `true` si era una de ellas.
fn handle_path_clipboard_key(w: &mut WindowState, hwnd: HWND, vk: u32) -> bool {
    let ctx = path_ctx(w);
    let pasted = if vk == 0x56 { crate::clipboard::get_clipboard_text(hwnd).ok() } else { None };
    let crate::Prompt::Path(p) = &mut w.ws.prompt else { return false };
    match vk {
        0x41 => p.select_all(),
        0x43 => {
            let _ = crate::clipboard::set_clipboard_text(hwnd, &p.copy_text());
        }
        0x58 => {
            let text = p.cut(&ctx);
            let _ = crate::clipboard::set_clipboard_text(hwnd, &text);
        }
        0x56 => {
            if let Some(text) = pasted {
                p.paste(&text, &ctx);
            }
        }
        0x5A => p.undo(&ctx),
        _ => return false,
    }
    true
}

fn handle_path_key(w: &mut WindowState, hwnd: HWND, vk: u32, mods: Modifiers) {
    if mods.ctrl && !mods.alt && handle_path_clipboard_key(w, hwnd, vk) {
        return;
    }
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
/// Arranca "Abrir"/"Guardar como": con el ajuste "Selector nativo de Windows" activo
/// va directo al diálogo de `IFileOpenDialog`/`IFileSaveDialog` (sin pasar por la
/// línea de ruta de abajo); si no, abre la línea de ruta de siempre con `initial`
/// como valor de partida. Devuelve `true` cuando abrió la línea de ruta (para que
/// quien llama sepa si le toca arrancar `start_popup_anim`).
/// Carpeta configurada en Ajustes → Archivos → "Guardar por defecto en" (Escritorio,
/// Documentos, Descargas...), si hay una. `None` deja el comportamiento de siempre
/// (carpeta de usuario).
fn default_save_dir(w: &WindowState) -> Option<std::path::PathBuf> {
    let dir = w.cfg.borrow().files.default_save_dir.clone()?;
    (!dir.is_empty()).then(|| std::path::PathBuf::from(dir))
}

fn start_path_entry(w: &mut WindowState, hwnd: HWND, purpose: crate::Purpose, initial: String) -> bool {
    let initial = if initial.is_empty() && purpose == crate::Purpose::Save {
        default_save_dir(w).map(|d| format!("{}\\", d.display())).unwrap_or(initial)
    } else {
        initial
    };
    if !w.cfg.borrow().ui.native_file_dialog {
        w.ws.prompt = crate::Prompt::Path(crate::PathPromptState::new(purpose, initial));
        return true;
    }
    let dir = if purpose == crate::Purpose::Save { default_save_dir(w) } else { None };
    if let Some(path) = crate::native_dialog::pick_path(hwnd, purpose, dir.as_deref()) {
        if let Some(value) = path.to_str() {
            let ctx = path_ctx(w);
            let mut p = crate::PathPromptState::new(purpose, String::new());
            p.type_text(value, &ctx);
            w.ws.prompt = crate::Prompt::Path(p);
            commit_path_prompt(w, hwnd);
        }
    }
    false
}

fn open_native_dialog(w: &mut WindowState, hwnd: HWND) {
    let purpose = match &w.ws.prompt {
        crate::Prompt::Path(p) => p.purpose,
        _ => return,
    };
    let dir = if purpose == crate::Purpose::Save { default_save_dir(w) } else { None };
    let Some(path) = crate::native_dialog::pick_path(hwnd, purpose, dir.as_deref()) else { return };
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
    commit_path_prompt_with(w, hwnd, false);
}

/// Respuesta a la pregunta "ya existe" de la línea de ruta (tecla o clic en su botón).
fn answer_overwrite(w: &mut WindowState, hwnd: HWND, choice: crate::OverwriteChoice) {
    let value = match &mut w.ws.prompt {
        crate::Prompt::Path(p) if p.ask_overwrite => {
            p.ask_overwrite = false;
            p.value.clone()
        }
        _ => return,
    };
    match choice {
        crate::OverwriteChoice::Overwrite => commit_path_prompt_with(w, hwnd, true),
        crate::OverwriteChoice::Cancel => {}
        crate::OverwriteChoice::OpenExisting => {
            let path = std::path::PathBuf::from(&value);
            match crate::open_as_document(&path) {
                Ok(opened) => {
                    let cfg = w.cfg.borrow().clone();
                    let state = maybe_vim(crate::EditorState::from_opened(opened), &cfg);
                    // Si el documento nuevo está vacío y sin tocar, el abierto ocupa su
                    // sitio en vez de dejar una pestaña vacía detrás.
                    let st = w.ws.active();
                    let blank = st.path.is_none() && !st.doc.is_dirty() && st.doc.buffer().len_chars() == 0;
                    if blank {
                        let visible_lines = st.viewport.visible_lines;
                        let slot = w.ws.active_mut();
                        *slot = state;
                        slot.viewport.visible_lines = visible_lines;
                    } else {
                        open_tab(w, hwnd, state);
                    }
                    notty_io::record_path_use(&path);
                    w.ws.close_prompt();
                    unsafe { update_title(hwnd, w.ws.active(), lang_of(w)) };
                }
                Err(e) => {
                    if let crate::Prompt::Path(p) = &mut w.ws.prompt {
                        p.last_error = Some(e.to_string());
                    }
                }
            }
        }
    }
}

fn commit_path_prompt_with(w: &mut WindowState, hwnd: HWND, force_overwrite: bool) {
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
                open_tab(w, hwnd, maybe_vim(crate::EditorState::from_opened(opened), &cfg));
                done = true;
            }
            Err(e) => error = Some(e.to_string()),
        },
        // Guardar encima de un archivo que ya existe y no es el propio documento:
        // antes de escribir encima, se pregunta (ver `answer_overwrite`).
        notty_io::Hint::Exists
            if purpose == crate::Purpose::Save
                && !w.ws.active().path.as_deref().is_some_and(|own| crate::same_file(own, &path))
                && !force_overwrite =>
        {
            if let crate::Prompt::Path(p) = &mut w.ws.prompt {
                p.ask_overwrite = true;
            }
            return;
        }
        notty_io::Hint::Exists | notty_io::Hint::New | notty_io::Hint::DirNew => {
            match notty_io::create_parent_dirs(&path) {
                Ok(_) => match purpose {
                    crate::Purpose::Open => {
                        // Caso raro: se pidió "abrir" algo que no existe. Se trata como crear
                        // un documento nuevo con esa ruta.
                        let mut state = crate::EditorState::new_empty();
                        state.path = Some(path.clone());
                        let cfg = w.cfg.borrow().clone();
                        open_tab(w, hwnd, maybe_vim(state, &cfg));
                        done = true;
                    }
                    crate::Purpose::Save => {
                        // Si save() falla, se deshace el cambio de `path`: el documento no
                        // se queda apuntando en silencio a una ruta que no se pudo escribir.
                        let previous_path = w.ws.active().path.clone();
                        w.ws.active_mut().path = Some(path.clone());
                        match w.ws.active_mut().save() {
                            Ok(()) => {
                                w.ws.active_mut().open_mtime = notty_io::mtime(&path).ok();
                                w.ws.active_mut().autosave_failed_rev = None;
                                done = true;
                            }
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
        let then_close = match &mut w.ws.prompt {
            crate::Prompt::Path(p) => p.then_close.take(),
            _ => None,
        };
        notty_io::record_path_use(&path);
        w.ws.close_prompt();
        unsafe {
            update_title(hwnd, w.ws.active(), lang_of(w));
        }
        if let Some(req) = then_close {
            continue_close(w, hwnd, req);
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
            if w.ws.active().path.is_none() {
                if start_path_entry(w, hwnd, crate::Purpose::Save, String::new()) {
                    start_popup_anim(w, hwnd);
                }
            } else {
                save_now(w, hwnd);
            }
        }
        crate::VimCmd::SaveAs(path) => vim_path_command(w, hwnd, crate::Purpose::Save, &path),
        crate::VimCmd::Edit(path) => vim_path_command(w, hwnd, crate::Purpose::Open, &path),
        crate::VimCmd::SaveAll => {
            save_all(w, hwnd);
        }
        crate::VimCmd::Quit => {
            let i = w.ws.active_index();
            request_close(w, hwnd, &[i], false);
        }
        crate::VimCmd::ForceQuit => {
            let i = w.ws.active_index();
            close_tab(w, hwnd, i);
            refresh_recovery(w);
        }
        crate::VimCmd::QuitAll => {
            let all: Vec<usize> = (0..w.ws.len()).collect();
            request_close(w, hwnd, &all, true);
        }
        crate::VimCmd::ForceQuitAll => {
            w.close_confirmed = true;
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        crate::VimCmd::SaveAndQuitAll => {
            if save_all(w, hwnd) {
                let all: Vec<usize> = (0..w.ws.len()).collect();
                request_close(w, hwnd, &all, true);
            }
        }
        crate::VimCmd::Revert => revert_active(w, hwnd),
        crate::VimCmd::New => {
            let cfg = w.cfg.borrow().clone();
            open_tab(w, hwnd, maybe_vim(crate::EditorState::new_empty(), &cfg));
        }
        crate::VimCmd::NextTab => step_tab(w, hwnd, 1),
        crate::VimCmd::PrevTab => step_tab(w, hwnd, -1),
        crate::VimCmd::GotoLine(n) => {
            let st = w.ws.active_mut();
            let total = st.doc.buffer().len_lines();
            let line = n.saturating_sub(1).min(total.saturating_sub(1));
            let at = st.doc.buffer().line_start(line);
            st.doc.set_cursor(at);
            st.viewport.scroll_to_include(line, total);
        }
        crate::VimCmd::LineNumbers(on) => {
            let mut cfg = w.cfg.borrow_mut();
            cfg.ui.line_numbers = on.unwrap_or(!cfg.ui.line_numbers);
            let _ = notty_config::save(&cfg, &notty_config::default_path());
        }
        crate::VimCmd::SaveAndQuit => {
            // Si no se pudo guardar, la pestaña no se cierra: se perdería lo escrito.
            if save_now(w, hwnd) {
                let i = w.ws.active_index();
                request_close(w, hwnd, &[i], false);
            }
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
        crate::VimCmd::Unknown(cmd) => {
            if !cmd.is_empty() {
                show_notice(w, hwnd, format!("{}: :{cmd}", tr_w(w, "No es un comando de vim")));
            }
        }
    }
    unsafe {
        update_title(hwnd, w.ws.active(), lang_of(w));
    }
}

/// `:e ruta` / `:w ruta`: lo mismo que escribir la ruta en la línea de Abrir/Guardar
/// como y pulsar Enter. Si hace falta preguntar algo (ya existe) o falla, la línea se
/// queda abierta con la pregunta o el error.
fn vim_path_command(w: &mut WindowState, hwnd: HWND, purpose: crate::Purpose, path: &str) {
    let ctx = path_ctx(w);
    let mut p = crate::PathPromptState::new(purpose, String::new());
    p.type_text(path, &ctx);
    w.ws.prompt = crate::Prompt::Path(p);
    commit_path_prompt(w, hwnd);
    if matches!(w.ws.prompt, crate::Prompt::Path(_)) {
        start_popup_anim(w, hwnd);
    }
}

/// `:wa`: guarda los documentos con cambios que tienen ruta. `false` si alguno falló.
fn save_all(w: &mut WindowState, hwnd: HWND) -> bool {
    let active = w.ws.active_index();
    let mut ok = true;
    for i in 0..w.ws.len() {
        if w.ws.get(i).is_some_and(|st| st.path.is_some() && st.doc.is_dirty()) {
            w.ws.activate(i);
            ok &= save_now(w, hwnd);
        }
    }
    w.ws.activate(active);
    ok
}

/// `:e!`: vuelve a cargar del disco el documento activo, descartando sus cambios.
fn revert_active(w: &mut WindowState, hwnd: HWND) {
    let Some(path) = w.ws.active().path.clone() else { return };
    match crate::open_as_document(&path) {
        Ok(opened) => {
            let cfg = w.cfg.borrow().clone();
            let state = maybe_vim(crate::EditorState::from_opened(opened), &cfg);
            let slot = w.ws.active_mut();
            let (visible_lines, top) = (slot.viewport.visible_lines, slot.viewport.first_line);
            *slot = state;
            slot.viewport.visible_lines = visible_lines;
            slot.viewport.first_line = top.min(slot.doc.buffer().len_lines().saturating_sub(1));
            refresh_recovery(w);
        }
        Err(e) => show_notice(w, hwnd, format!("{}: {}", tr_w(w, "No se pudo abrir"), tr_w_owned(w, &e.to_string()))),
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
