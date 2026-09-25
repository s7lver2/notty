use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMSBT_MAINWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{InvalidateRect, ValidateRect};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, VK_CONTROL, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetClientRect,
    GetMessageW, GetWindowLongPtrW, MSG, PostQuitMessage, RegisterClassExW, SW_SHOW, SetWindowLongPtrW,
    SetWindowTextW, ShowWindow, TranslateMessage, WHEEL_DELTA, WM_CHAR, WM_DESTROY, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SIZE, WM_SYSKEYDOWN, WNDCLASSEXW,
    WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW,
};
use windows::core::{PCWSTR, Result, w};

use std::cell::RefCell;
use std::rc::Rc;

use crate::{EditorState, Modifiers, Renderer, Viewport};

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
            900,
            600,
            None,
            None,
            Some(instance.into()),
            None,
        )?;

        enable_mica(hwnd);

        let mut ws = crate::Workspace::new();
        if let Some(p) = path {
            *ws.active_mut() = EditorState::from_opened(
                crate::open_as_document(std::path::Path::new(p)).expect("no se pudo abrir el archivo"),
            );
        }
        let renderer = Renderer::new(hwnd)?;

        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);
        let height = (client.bottom - client.top).max(0) as f32;
        ws.active_mut().viewport =
            Viewport::new(renderer.line_height(), (height - renderer.line_height()).max(0.0));
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

/// Activa el fondo Mica y el modo oscuro de la barra de título si el sistema está en oscuro.
/// Si `DwmSetWindowAttribute` falla (Windows más viejo que 11 22621), la ventana sigue
/// funcionando con el fondo por defecto: no es un error fatal.
unsafe fn enable_mica(hwnd: HWND) {
    unsafe {
        let dark: i32 = if system_uses_dark_mode() { 1 } else { 0 };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        );
        let backdrop = DWMSBT_MAINWINDOW.0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as *const _,
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
                    w.renderer.paint(&w.ws, &ui);
                }
                let _ = ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }
            WM_SIZE => {
                if let Some(w) = ptr.as_mut() {
                    let width = (lparam.0 as u32) & 0xFFFF;
                    let height = ((lparam.0 as u32) >> 16) & 0xFFFF;
                    w.renderer.resize(width, height);
                    let line_height = w.renderer.line_height();
                    let new_viewport = Viewport::new(line_height, (height as f32 - line_height).max(0.0));
                    w.ws.active_mut().viewport.visible_lines = new_viewport.visible_lines;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if let Some(w) = ptr.as_mut() {
                    let vk = wparam.0 as u32;
                    let alt_down = (GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0;
                    let mods = Modifiers {
                        ctrl: (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0,
                        shift: (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0,
                        alt: false,
                    };
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
                    let clicked_tab = w
                        .renderer
                        .tab_rects()
                        .iter()
                        .position(|&(l, t, r, b)| x >= l && x < r && y >= t && y < b);
                    if let Some(i) = clicked_tab {
                        w.ws.activate(i);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    } else {
                        let idx = w.renderer.char_index_at(w.ws.active(), x, y);
                        w.ws.active_mut().doc.set_cursor(idx);
                        w.selection_anchor = idx;
                        w.mouse_down = true;
                        SetCapture(hwnd);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if let Some(w) = ptr.as_mut() {
                    if w.mouse_down {
                        let (x, y) = point_from_lparam(lparam);
                        let idx = w.renderer.char_index_at(w.ws.active(), x, y);
                        w.ws.active_mut().doc.set_selection(w.selection_anchor, idx);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if let Some(w) = ptr.as_mut() {
                    w.mouse_down = false;
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
