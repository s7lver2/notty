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
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetClientRect, GetMessageW,
    GetWindowLongPtrW, HTCAPTION, HTCLIENT, HTMAXBUTTON, HTTOP, IsZoomed, MSG, NCCALCSIZE_PARAMS, PostQuitMessage,
    RegisterClassExW, SM_CXPADDEDBORDER, SM_CYFRAME, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SW_SHOW,
    SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, SetWindowTextW,
    ShowWindow, TranslateMessage, WHEEL_DELTA, WM_ACTIVATE, WM_CHAR, WM_DESTROY, WM_DPICHANGED, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCHITTEST, WM_NCLBUTTONDOWN,
    WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_PAINT, WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN, WNDCLASSEXW,
    WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW,
};
use windows::core::{PCWSTR, Result, w};

use std::cell::RefCell;
use std::rc::Rc;

use crate::layout;
use crate::{EditorState, Hit, Modifiers, Renderer, Viewport};

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
            menu_bar_visible: self.cfg.borrow().ui.menubar == notty_config::MenuBar::Visible || self.menu_visible,
            open_menu: None,
        }
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

/// Abre la ventana principal de notty y bloquea hasta que se cierra.
/// `path` es la ruta pasada por línea de comandos, si la hay; `load` es el resultado
/// de cargar `config.toml` (que puede traer un aviso si el archivo estaba roto).
pub fn run(path: Option<&str>, load: notty_config::LoadResult) -> Result<()> {
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

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
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
        if let Some(p) = path {
            let p = std::path::Path::new(p);
            match crate::open_as_document(p) {
                Ok(opened) => *ws.active_mut() = EditorState::from_opened(opened),
                Err(_) => {
                    // No es texto (o no se pudo decodificar): se abre directamente en vista
                    // raw, como pide la Task 7 de este plan.
                    let mut state = EditorState::new_empty();
                    state.path = Some(p.to_path_buf());
                    if let Ok(raw) = crate::open_raw_doc(p) {
                        state.raw = Some(raw);
                    }
                    *ws.active_mut() = state;
                }
            }
        }
        let dpi = GetDpiForWindow(hwnd);
        let renderer = Renderer::new(hwnd, dpi)?;

        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);
        let scale = renderer.scale();
        let height = ((client.bottom - client.top).max(0) as f32) / scale;
        let width = ((client.right - client.left).max(0) as f32) / scale;
        let frame = layout::frame(width, height, layout::Bands::default());
        ws.active_mut().viewport = Viewport::new(renderer.line_height(), frame.body.height());
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
        });
        let ptr = Box::into_raw(window_state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);

        let _ = ShowWindow(hwnd, SW_SHOW);

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
unsafe fn apply_dark_mode(hwnd: HWND, dark: bool) {
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
fn system_uses_dark_mode() -> bool {
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
                    let scale = w.renderer.scale();
                    let (w_dip, h_dip) = (width as f32 / scale, height as f32 / scale);
                    let frame = layout::frame(w_dip, h_dip, layout::Bands::default());
                    w.ws.active_mut().viewport.visible_lines = layout::visible_lines(frame.body);
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

                    if !matches!(w.ws.prompt, crate::Prompt::None) {
                        handle_prompt_keydown(w, hwnd, vk, mods);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }

                    let ui_mods = notty_input::Modifiers { ctrl: mods.ctrl, shift: mods.shift, alt: alt_down };
                    if let Some(cmd) = w.ui_keymap.get(&(vk, ui_mods)).copied() {
                        match cmd {
                            notty_input::UiCommand::NewTab => w.ws.open(crate::EditorState::new_empty()),
                            notty_input::UiCommand::NextTab => w.ws.next(),
                            notty_input::UiCommand::PrevTab => w.ws.prev(),
                            notty_input::UiCommand::CloseTab => {
                                w.ws.close_active();
                            }
                            notty_input::UiCommand::OpenSettings => {
                                let cfg_for_settings = w.cfg.clone();
                                let hwnd_copy = hwnd;
                                let _ = crate::settings_window::open(
                                    hwnd,
                                    cfg_for_settings,
                                    Box::new(move || {
                                        let _ = InvalidateRect(Some(hwnd_copy), None, false);
                                    }),
                                );
                            }
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
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::EditorAction::FindNext | crate::EditorAction::FindPrev => {
                            // Fuera de un prompt de búsqueda activo (ya cubierto arriba, antes de
                            // llegar aquí), F3 no tiene una búsqueda que repetir: no hace nada.
                        }
                        crate::EditorAction::Save => {
                            let _ = w.ws.active_mut().save();
                            update_title(hwnd, w.ws.active());
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        other => {
                            w.ws.active_mut().apply(other, std::time::Instant::now());
                            update_title(hwnd, w.ws.active());
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
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                        w.ws.active_mut().insert_char(ch, std::time::Instant::now());
                        update_title(hwnd, w.ws.active());
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
                        crate::Hit::Tab(i) => {
                            w.ws.activate(i);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        crate::Hit::Body if w.ws.active().raw.is_none() => {
                            let ui = w.render_ui();
                            let total = w.ws.active().doc.buffer().len_lines();
                            let (body, gutter_w) = w.renderer.body_and_gutter(&ui, total);
                            let idx = w.renderer.char_index_at(w.ws.active(), body, gutter_w, x, y);
                            w.ws.active_mut().doc.set_cursor(idx);
                            w.selection_anchor = idx;
                            w.mouse_down = true;
                            SetCapture(hwnd);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        _ => {}
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
                        let ui = w.render_ui();
                        let total = w.ws.active().doc.buffer().len_lines();
                        let (body, gutter_w) = w.renderer.body_and_gutter(&ui, total);
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
            WM_MOUSEWHEEL => {
                if let Some(w) = ptr.as_mut() {
                    let delta = ((wparam.0 >> 16) as i16) as i32;
                    w.ws.active_mut().scroll_by(-(delta / WHEEL_DELTA as i32) * 3);
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
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

// --- Modo vim: WM_CHAR (letras) -----------------------------------------------------

fn handle_vim_char(w: &mut WindowState, ch: char) {
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
        crate::VimOutcome::Bubble => {
            // La tecla no es de vim: se trata como si vim no estuviera activo.
            if !ch.is_control() {
                w.ws.active_mut().insert_char(ch, std::time::Instant::now());
            }
        }
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
        0x4F if mods.ctrl => {
            // TODO(plan futuro): diálogo nativo de Windows
        }
        _ => {}
    }
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

    match hint {
        notty_io::Hint::Empty => {}
        notty_io::Hint::Dir => {
            // No tiene sentido "abrir" ni "guardar" una carpeta: no hacer nada.
        }
        notty_io::Hint::Exists if purpose == crate::Purpose::Open => {
            if let Ok(opened) = crate::open_as_document(&path) {
                w.ws.open(crate::EditorState::from_opened(opened));
                done = true;
            }
        }
        notty_io::Hint::Exists | notty_io::Hint::New | notty_io::Hint::DirNew => {
            let _ = notty_io::create_parent_dirs(&path);
            match purpose {
                crate::Purpose::Open => {
                    // Caso raro: se pidió "abrir" algo que no existe. Se trata como crear
                    // un documento nuevo con esa ruta.
                    let mut state = crate::EditorState::new_empty();
                    state.path = Some(path.clone());
                    w.ws.open(state);
                    done = true;
                }
                crate::Purpose::Save => {
                    w.ws.active_mut().path = Some(path.clone());
                    done = w.ws.active_mut().save().is_ok();
                }
            }
        }
    }

    if done {
        w.ws.close_prompt();
        unsafe {
            update_title(hwnd, w.ws.active());
        }
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
            let _ = w.ws.active_mut().save();
        }
        crate::VimCmd::Quit => {
            w.ws.close_active();
        }
        crate::VimCmd::SaveAndQuit => {
            let _ = w.ws.active_mut().save();
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
        let mut q = s.query.clone();
        q.push(ch);
        s.set_query(q);
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
