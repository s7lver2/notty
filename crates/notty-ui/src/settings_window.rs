use std::cell::RefCell;
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Controls::BST_CHECKED;
use windows::Win32::UI::WindowsAndMessaging::{
    BM_SETCHECK, BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW,
    GWLP_USERDATA, GetWindowLongPtrW, HMENU, RegisterClassExW, SW_SHOW, SendMessageW, SetWindowLongPtrW,
    ShowWindow, WINDOW_STYLE, WM_COMMAND, WM_DESTROY, WNDCLASSEXW, WS_CHILD, WS_OVERLAPPEDWINDOW,
    WS_VISIBLE,
};
use windows::core::{PCWSTR, Result, w};

use notty_config::{Config, Files, Preset, Theme, apply_preset};

const ID_PRESET_MODERNA: usize = 100;
const ID_PRESET_CLASICA: usize = 101;
const ID_PRESET_ZEN: usize = 102;
const ID_THEME_SYSTEM: usize = 110;
const ID_THEME_LIGHT: usize = 111;
const ID_THEME_DARK: usize = 112;
const ID_LINE_NUMBERS: usize = 120;
const ID_FILES_TABS: usize = 130;
const ID_FILES_BUFFERS: usize = 131;
const ID_VIM_ALWAYS: usize = 121;

struct State {
    cfg: Rc<RefCell<Config>>,
    on_change: Box<dyn Fn()>,
}

/// Abre la ventana de Ajustes. `on_change` se llama cada vez que el usuario
/// cambia algo (ya guardado en disco), para que la ventana principal repinte.
pub fn open(parent: HWND, cfg: Rc<RefCell<Config>>, on_change: Box<dyn Fn()>) -> Result<()> {
    unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
        let class_name = w!("NottySettingsClass");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            w!("Ajustes · notty"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            420,
            360,
            Some(parent),
            None,
            Some(instance.into()),
            None,
        )?;

        let preset = cfg.borrow().ui.preset;
        let theme = cfg.borrow().ui.theme;
        let line_numbers = cfg.borrow().ui.line_numbers;
        let files = cfg.borrow().ui.files;
        let vim_always = cfg.borrow().ui.vim_always;

        radio(hwnd, instance, "Moderna", 20, 20, ID_PRESET_MODERNA, preset == Preset::Moderna);
        radio(hwnd, instance, "Clásica", 20, 48, ID_PRESET_CLASICA, preset == Preset::Clasica);
        radio(hwnd, instance, "Zen", 20, 76, ID_PRESET_ZEN, preset == Preset::Zen);

        radio(hwnd, instance, "Tema: sistema", 20, 116, ID_THEME_SYSTEM, theme == Theme::System);
        radio(hwnd, instance, "Tema: claro", 20, 144, ID_THEME_LIGHT, theme == Theme::Light);
        radio(hwnd, instance, "Tema: oscuro", 20, 172, ID_THEME_DARK, theme == Theme::Dark);

        checkbox(hwnd, instance, "Números de línea", 20, 212, ID_LINE_NUMBERS, line_numbers);
        checkbox(hwnd, instance, "Modo vim siempre", 20, 240, ID_VIM_ALWAYS, vim_always);

        radio(hwnd, instance, "Pestañas", 20, 280, ID_FILES_TABS, files == Files::Tabs);
        radio(hwnd, instance, "Buffers", 160, 280, ID_FILES_BUFFERS, files == Files::Buffers);

        let state = Box::new(State { cfg, on_change });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);

        let _ = ShowWindow(hwnd, SW_SHOW);
    }
    Ok(())
}

unsafe fn radio(
    parent: HWND,
    instance: windows::Win32::Foundation::HMODULE,
    text: &str,
    x: i32,
    y: i32,
    id: usize,
    checked: bool,
) {
    unsafe {
        make_button(parent, instance, text, x, y, id, BS_AUTORADIOBUTTON as u32, checked);
    }
}

unsafe fn checkbox(
    parent: HWND,
    instance: windows::Win32::Foundation::HMODULE,
    text: &str,
    x: i32,
    y: i32,
    id: usize,
    checked: bool,
) {
    unsafe {
        make_button(parent, instance, text, x, y, id, BS_AUTOCHECKBOX as u32, checked);
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn make_button(
    parent: HWND,
    instance: windows::Win32::Foundation::HMODULE,
    text: &str,
    x: i32,
    y: i32,
    id: usize,
    style_bits: u32,
    checked: bool,
) {
    unsafe {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let style = WS_CHILD.0 | WS_VISIBLE.0 | style_bits;
        if let Ok(btn) = CreateWindowExW(
            Default::default(),
            w!("BUTTON"),
            PCWSTR(wide.as_ptr()),
            WINDOW_STYLE(style),
            x,
            y,
            200,
            24,
            Some(parent),
            Some(HMENU(id as *mut _)),
            Some(instance.into()),
            None,
        ) {
            if checked {
                let _ = SendMessageW(btn, BM_SETCHECK, Some(WPARAM(BST_CHECKED.0 as usize)), None);
            }
        }
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_COMMAND => {
                let id = wparam.0 & 0xFFFF;
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
                if !ptr.is_null() {
                    let state = &mut *ptr;
                    apply_control(state, id);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
                if !ptr.is_null() {
                    drop(Box::from_raw(ptr));
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn apply_control(state: &mut State, id: usize) {
    let mut cfg = state.cfg.borrow_mut();
    match id {
        ID_PRESET_MODERNA => apply_preset(&mut cfg.ui, Preset::Moderna),
        ID_PRESET_CLASICA => apply_preset(&mut cfg.ui, Preset::Clasica),
        ID_PRESET_ZEN => apply_preset(&mut cfg.ui, Preset::Zen),
        ID_THEME_SYSTEM => cfg.ui.theme = Theme::System,
        ID_THEME_LIGHT => cfg.ui.theme = Theme::Light,
        ID_THEME_DARK => cfg.ui.theme = Theme::Dark,
        ID_LINE_NUMBERS => cfg.ui.line_numbers = !cfg.ui.line_numbers,
        ID_VIM_ALWAYS => cfg.ui.vim_always = !cfg.ui.vim_always,
        ID_FILES_TABS => cfg.ui.files = Files::Tabs,
        ID_FILES_BUFFERS => cfg.ui.files = Files::Buffers,
        _ => return,
    }
    let _ = notty_config::save(&cfg, &notty_config::default_path());
    drop(cfg);
    (state.on_change)();
}
