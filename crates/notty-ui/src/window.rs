use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMSBT_MAINWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{InvalidateRect, ValidateRect};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetClientRect,
    GetMessageW, GetWindowLongPtrW, MSG, PostQuitMessage, RegisterClassExW, SW_SHOW, SetWindowLongPtrW,
    ShowWindow, TranslateMessage, WM_DESTROY, WM_PAINT, WM_SIZE, WNDCLASSEXW, WS_EX_APPWINDOW,
    WS_OVERLAPPEDWINDOW,
};
use windows::core::{PCWSTR, Result, w};

use crate::{EditorState, Renderer, Viewport};

/// Estado ligado a una ventana concreta: se guarda en `GWLP_USERDATA` mientras vive.
struct WindowState {
    state: EditorState,
    renderer: Renderer,
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Abre la ventana principal de notty y bloquea hasta que se cierra.
/// `path` es la ruta pasada por línea de comandos, si la hay.
pub fn run(path: Option<&str>) -> Result<()> {
    let title = match path {
        Some(p) => format!("{p} · notty"),
        None => "sin título · notty".to_string(),
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

        let mut state = match path {
            Some(p) => EditorState::from_opened(
                crate::open_as_document(std::path::Path::new(p)).expect("no se pudo abrir el archivo"),
            ),
            None => EditorState::new_empty(),
        };
        let renderer = Renderer::new(hwnd)?;

        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);
        let height = (client.bottom - client.top).max(0) as f32;
        state.viewport = Viewport::new(renderer.line_height(), height);

        let window_state = Box::new(WindowState { state, renderer });
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
                if let Some(ws) = ptr.as_mut() {
                    ws.renderer.paint(&ws.state);
                }
                let _ = ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }
            WM_SIZE => {
                if let Some(ws) = ptr.as_mut() {
                    let width = (lparam.0 as u32) & 0xFFFF;
                    let height = ((lparam.0 as u32) >> 16) & 0xFFFF;
                    ws.renderer.resize(width, height);
                    let new_viewport = Viewport::new(ws.renderer.line_height(), height as f32);
                    ws.state.viewport.visible_lines = new_viewport.visible_lines;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
