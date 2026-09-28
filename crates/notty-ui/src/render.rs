//! Renderer Direct2D/DirectWrite: dibuja con una sola brocha (a la que se le cambia
//! el color antes de cada uso), varios formatos de texto fijos, y va apuntando en
//! `hits` qué rectángulo es qué para el ratón (ver `Hit`/`hit()`).

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN,
    D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BRUSH_PROPERTIES, D2D1_CAP_STYLE_ROUND, D2D1_COMBINE_MODE_EXCLUDE,
    D2D1_DASH_STYLE_SOLID, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_LINE_JOIN_ROUND,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES, D2D1CreateFactory, ID2D1Factory,
    ID2D1HwndRenderTarget, ID2D1PathGeometry, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_BOLD,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_LINE_SPACING_METHOD_UNIFORM,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory, IDWriteFactory,
    IDWriteFontCollection, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::core::Result;
use windows_numerics::{Matrix3x2, Vector2};

use notty_config::UiConfig;

use crate::layout::{self, Rect};
use crate::theme::{self, Rgba};
use crate::{EditorState, Viewport, Workspace};

mod extra;
mod md;
mod whats_new;
pub use whats_new::WhatsNewView;

/// Convierte un desplazamiento en chars (relativo al inicio de `text`) a un
/// desplazamiento en unidades UTF-16, que es lo que espera `IDWriteTextLayout`.
fn char_offset_to_utf16(text: &str, char_offset: usize) -> u32 {
    text.chars().take(char_offset).map(|c| c.len_utf16()).sum::<usize>() as u32
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn color(c: Rgba) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.0, g: c.1, b: c.2, a: c.3 }
}

fn rect_of(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

/// Zonas del ratón que `paint` va registrando mientras dibuja; lo último pintado
/// queda encima al recorrer la lista en `hit()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hit {
    #[default]
    None,
    /// Zona libre de la barra de título: arrastrar mueve la ventana.
    Caption,
    Min,
    Max,
    Close,
    Tab(usize),
    TabClose(usize),
    NewTab,
    Menu(usize),
    MenuItem(usize),
    Clickme,
    Pencil,
    /// Botón de Ajustes en la barra de título (no está en la maqueta: existe para que
    /// Ajustes se pueda encontrar sin saber `Ctrl+,` de memoria).
    Settings,
    Suggestion(usize),
    /// 0 = Aa, 1 = ab, 2 = .*
    SearchOpt(u8),
    /// En el prompt de reemplazar: 0 = campo buscar, 1 = campo «por».
    SearchField(u8),
    Body,
    /// Aviso "Actualización X.Y.Z disponible" en la barra de estado (Task 4 del
    /// plan del actualizador): clic abre el panel de notas de versión.
    UpdateNotice,
    /// Botón "Actualizar" del panel de notas de versión.
    UpdatePanelActualizar,
    /// Botón "Cerrar" del panel de notas de versión.
    UpdatePanelCerrar,
    /// Elemento `j` del menú contextual abierto (solo los activados).
    CtxItem(usize),
    /// Fondo de un desplegable o menú contextual (separadores, márgenes, elementos
    /// desactivados): se traga el clic en vez de dejarlo caer a lo que hay debajo.
    PopupBox,
    /// Botones ‹ › de la fila de pestañas cuando no caben todas.
    TabScrollLeft,
    TabScrollRight,
    /// Respuesta a "ya existe" en la línea de ruta: 0 = sobrescribir, 1 = abrir el
    /// existente, 2 = cancelar.
    Overwrite(u8),
    /// Respuesta a "cambios sin guardar" al cerrar: 0 = guardar, 1 = no guardar, 2 = cancelar.
    CloseChoice(u8),
    /// Enlace al repositorio en "Acerca de notty".
    AboutLink,
    /// Botones "✕" y "Entendido" del popup de Novedades.
    WhatsNewClose,
    WhatsNewOk,
    /// Tirador de la barra de scroll del editor (arrastrar desplaza el documento).
    ScrollThumb,
    /// Zona de la pista por encima/debajo del tirador: clic avanza una página.
    ScrollTrack,
}

/// Lo que enseña "Acerca de notty" (menú Ayuda).
#[derive(Debug, Clone, PartialEq)]
pub struct AboutContent {
    pub version: String,
    /// `https://github.com/owner/repo`, si el binario lo tiene configurado.
    pub url: Option<String>,
}

/// Una fila de un desplegable o menú contextual, ya resuelta para dibujar.
struct MenuRow<'a> {
    label: &'a str,
    shortcut: &'a str,
    enabled: bool,
    sep: bool,
    /// `Some(activo)` en las opciones que se encienden/apagan: se dibuja ✓ a la
    /// izquierda cuando están activas.
    check: Option<bool>,
}

/// Qué paneles se ven y cómo (`Files::Splits`, ver `splits.rs`). Con menos de dos
/// paneles, el cuerpo entero es del documento activo.
#[derive(Debug, Clone, Default)]
pub struct PaneView {
    pub docs: Vec<usize>,
    pub weights: Vec<f32>,
    pub focus: usize,
    /// Modo acomodar (Ctrl+Shift+R): se resalta el panel con foco y se enseñan las teclas.
    pub resize_mode: bool,
    /// Pestañas del modo Paneles: el documento que representa a cada una (el de su
    /// panel con foco) y todos los suyos. `None` fuera de ese modo: una pestaña por
    /// documento.
    pub tabs: Option<Vec<(usize, Vec<usize>)>>,
}

/// Contexto que no vive en `Workspace`/`UiConfig` pero que `paint` necesita para
/// saber cómo dibujar: tema, estado del ratón, si la ventana está maximizada/activa
/// y si hay un menú de la barra desplegado.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ViewState {
    pub dark: bool,
    pub hover: Hit,
    pub pressed: Hit,
    pub maximized: bool,
    pub active_window: bool,
    /// Menú `Alt` desplegado, o barra de menús visible.
    pub menu_bar_visible: bool,
    pub open_menu: Option<usize>,
    /// Progreso (`0.0..=1.0`) del fundido+desplazamiento de apertura del menú o de la
    /// caja de sugerencias, si hay uno en curso. `None` en reposo (dibuja igual que
    /// siempre, sin coste extra).
    pub popup_open: Option<f32>,
    /// Progreso (`0.0..=1.0`) del fundido del fondo de la pestaña activa al cambiar,
    /// si hay uno en curso. `None` en reposo (se pinta igual que siempre).
    pub tab_switch: Option<f32>,
    /// Bandas (`layout::Bands`) tal como estaban antes del cambio de ajuste en curso
    /// (Ajustes → "Varios archivos", barra de menús, pestañas bajo el menú, atajos,
    /// línea de comandos fusionada...), para fundir el contenido de la banda que
    /// aparece/desaparece en vez de que salte de golpe (Task 3 del plan de
    /// animaciones, extensión de `start_tab_switch_anim`). `None` en reposo: sin
    /// transición en curso, cada banda se pinta a fundido 1.0 de siempre.
    pub chrome_from: Option<layout::BandFrac>,
    /// Progreso (`0.0..=1.0`) de esa transición. Solo tiene sentido junto a `chrome_from`.
    pub chrome_fade: Option<f32>,
    /// Cambio de tema en curso: de qué tema se viene (`true` = oscuro) y progreso.
    pub theme_from: Option<(bool, f32)>,
    /// Cambio de color de acento en curso: de cuál se viene y progreso.
    pub accent_from: Option<(notty_config::AccentColor, f32)>,
    /// Elemento resaltado con el teclado (flechas) en el desplegable o menú
    /// contextual abierto.
    pub menu_sel: Option<usize>,
}

/// Los formatos de texto fijos que usa la maqueta, creados una vez en `Renderer::new`.
/// `pub(crate)` porque `settings_window` (Task 11) también dibuja con ellos, a través
/// de `Renderer::fonts()`.
pub struct Fonts {
    pub ui_12: IDWriteTextFormat,
    pub ui_12_5: IDWriteTextFormat,
    pub ui_11: IDWriteTextFormat,
    pub ui_11_5: IDWriteTextFormat,
    pub ui_13: IDWriteTextFormat,
    pub ui_20_semibold: IDWriteTextFormat,
    pub ui_11_5_semibold: IDWriteTextFormat,
    pub ui_9: IDWriteTextFormat,
    pub ui_10_5: IDWriteTextFormat,
    pub ui_12_semibold: IDWriteTextFormat,
    pub ui_12_5_semibold: IDWriteTextFormat,
    pub ui_13_semibold: IDWriteTextFormat,
    pub ui_18_semibold: IDWriteTextFormat,
    pub mono_13: IDWriteTextFormat,
    pub mono_13_bold: IDWriteTextFormat,
    pub mono_11: IDWriteTextFormat,
    pub mono_11_bold: IDWriteTextFormat,
    pub mono_11_5: IDWriteTextFormat,
    pub mono_11_5_semibold: IDWriteTextFormat,
    pub mono_12: IDWriteTextFormat,
    pub mono_12_semibold: IDWriteTextFormat,
    pub mono_12_5: IDWriteTextFormat,
}

/// Busca `primary` en la colección de fuentes del sistema; si no existe, usa `fallback`.
fn resolve_family(collection: &IDWriteFontCollection, primary: &str, fallback: &str) -> String {
    unsafe {
        let mut index = 0u32;
        let mut exists = windows::core::BOOL(0);
        let name_wide: Vec<u16> = primary.encode_utf16().chain(std::iter::once(0)).collect();
        if collection
            .FindFamilyName(windows::core::PCWSTR(name_wide.as_ptr()), &mut index, &mut exists)
            .is_ok()
            && exists.as_bool()
        {
            primary.to_string()
        } else {
            fallback.to_string()
        }
    }
}

fn make_format(
    dwrite: &IDWriteFactory,
    family: &str,
    size: f32,
    weight: windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT,
) -> Result<IDWriteTextFormat> {
    let family_wide: Vec<u16> = family.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let fmt = dwrite.CreateTextFormat(
            windows::core::PCWSTR(family_wide.as_ptr()),
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            windows::core::w!(""),
        )?;
        let _ = fmt.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        Ok(fmt)
    }
}

/// Añade recorte con «…» (usado en pestañas, nombres largos) al formato dado.
fn with_ellipsis_trimming(dwrite: &IDWriteFactory, fmt: &IDWriteTextFormat) -> Result<()> {
    unsafe {
        let sign = dwrite.CreateEllipsisTrimmingSign(fmt)?;
        let trimming =
            DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
        fmt.SetTrimming(&trimming, &sign)?;
    }
    Ok(())
}

pub struct Renderer {
    _d2d: ID2D1Factory,
    hwnd: HWND,
    target: ID2D1HwndRenderTarget,
    /// El último `EndDraw` dijo que el dispositivo se perdió (driver reiniciado,
    /// escritorio remoto...): `recover_device` recrea el target antes de pintar.
    device_lost: std::cell::Cell<bool>,
    /// Pintando por CPU (ver `new_software`).
    software: bool,
    dwrite: IDWriteFactory,
    brush: ID2D1SolidColorBrush,
    fonts: Fonts,
    dpi: u32,
    hits: Vec<(Rect, Hit)>,
    /// Posición donde debe dibujarse el desplegable del menú abierto (si lo hay),
    /// calculada por `draw_menubar` pero pintada al final de `paint` para quedar
    /// encima de todo lo demás.
    pending_dropdown: Option<(f32, f32)>,
    /// Multiplicador de opacidad aplicado a todo lo que se dibuje con los helpers de
    /// `fill`/`stroke`/`text` de aquí en adelante (`1.0` = sin cambio). Usado por
    /// `settings_window` para el fundido de apertura de la ventana (Task 5 del plan de
    /// animaciones): más simple que tocar cada llamada de dibujo una a una.
    fade: std::cell::Cell<f32>,
    /// Idioma efectivo del fotograma actual (`lang::resolve`, nunca `Auto`), puesto al
    /// principio de `paint`/`settings_window::paint`: evita tener que hacer viajar
    /// `lang` como parámetro por cada función de dibujo, igual que `fade`.
    lang: std::cell::Cell<notty_config::Lang>,
    /// Si `paint` debe dejar el frame abierto para que una capa (el tour) se dibuje
    /// encima antes del `EndDraw`: dos `EndDraw` por frame presentan el editor sin la
    /// capa entre medias y se ve parpadear.
    hold_frame: std::cell::Cell<bool>,
    /// Texto del aviso de actualización disponible en la barra de estado (Task 4 del
    /// plan del actualizador), o `None` si no hay ninguna comprobada/pendiente. Vive en
    /// el `Renderer` (no en `ViewState`, que es `Copy`) porque es una `String` propia.
    update_notice: Option<String>,
    /// Contenido del panel de notas de versión, si está abierto. Ver `UpdatePanelContent`.
    update_panel: Option<UpdatePanelContent>,
    /// Menú contextual abierto (se dibuja el último, encima de todo).
    context_menu: Option<crate::context_menu::ContextMenu>,
    /// Pestañas entrando/saliendo en este fotograma (ver `tab_anim.rs`).
    tab_anim: crate::tab_anim::TabAnimFrame,
    /// Documento más cercano oculto a cada lado de la fila de pestañas (lo que
    /// activan los botones ‹ ›), calculado al dibujarla.
    tab_scroll_targets: (Option<usize>, Option<usize>),
    /// "Acerca de notty" abierto.
    about: Option<AboutContent>,
    whats_new: Option<WhatsNewView>,
    /// Progreso (`0..1` en 1,8 s) del aviso "Ruta copiada", si hay uno en curso.
    path_copied: Option<f32>,
    /// Aviso breve de la barra de estado (p. ej. un error al guardar): progreso y texto.
    notice: Option<(f32, String)>,
    /// Atajos en vigor (tras reasignaciones en `[keys]`) de los elementos de menú que
    /// los tienen, ya en formato corto ("^N").
    menu_keys: Vec<(crate::menu::MenuCmd, String)>,
    /// `Config::syntax_disabled`, copiado antes de cada `paint`.
    syntax_disabled: Vec<String>,
    /// Familia monoespaciada resuelta en `new` (Cascadia Mono o Consolas de *fallback*):
    /// hace falta guardarla para poder recrear `mono_13`/`mono_13_bold` a otro tamaño
    /// cuando cambia el zoom (Ctrl+=/Ctrl+-/Ctrl+0), sin repetir la búsqueda de familia.
    mono_family: String,
    /// Familia de la interfaz resuelta en `new` (Segoe UI Variable Text o Segoe UI).
    ui_family: String,
    /// Multiplicador de `layout::FONT_MONO`/`LINE_H` vigente (Ctrl+=/Ctrl+-/Ctrl+0).
    /// Solo afecta al cuerpo del editor y la vista raw, ver `set_font_scale`.
    font_scale: f32,
    /// Documento de cada panel y cuál tiene el foco (`Files::Splits`). Con menos de 2
    /// paneles se dibuja solo el documento activo, como siempre.
    panes: PaneView,
}

/// Lo que hay que pintar en el panel de notas de versión cuando está abierto
/// (Task 4 del plan del actualizador). `window.rs` lo reconstruye a partir de
/// `crate::UpdateState` en cada `WM_PAINT`, antes de llamar a `paint`.
#[derive(Debug, Clone)]
pub struct UpdatePanelContent {
    pub version: String,
    pub body: String,
    /// Progreso de descarga, error de verificación, etc.; se dibuja bajo las notas.
    pub status_line: Option<String>,
    /// Si se dibuja el botón "Actualizar" (se oculta durante la descarga).
    pub show_actualizar: bool,
}

/// Render target de `hwnd` (al tamaño actual de su área cliente) y la brocha única,
/// que depende de él. Se llama al crear el `Renderer` y cada vez que se pierde el dispositivo.
/// Con `software`, Direct2D pinta por CPU: se crea en ~20 ms en vez de los ~300 ms que
/// tarda en cargar el driver de la GPU la primera vez (ver `Renderer::new_software`).
unsafe fn create_target(d2d: &ID2D1Factory, hwnd: HWND, dpi: u32, software: bool) -> Result<(ID2D1HwndRenderTarget, ID2D1SolidColorBrush)> {
    unsafe {
        let mut client = windows::Win32::Foundation::RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client);
        let width = (client.right - client.left).max(1) as u32;
        let height = (client.bottom - client.top).max(1) as u32;
        let target = d2d.CreateHwndRenderTarget(
            &D2D1_RENDER_TARGET_PROPERTIES {
                r#type: if software { D2D1_RENDER_TARGET_TYPE_SOFTWARE } else { D2D1_RENDER_TARGET_TYPE_DEFAULT },
                ..Default::default()
            },
            &D2D1_HWND_RENDER_TARGET_PROPERTIES { hwnd, pixelSize: D2D_SIZE_U { width, height }, ..Default::default() },
        )?;
        target.SetDpi(dpi as f32, dpi as f32);
        let brush = target.CreateSolidColorBrush(
            &color(Rgba(1.0, 1.0, 1.0, 1.0)),
            Some(&D2D1_BRUSH_PROPERTIES { opacity: 1.0, transform: Matrix3x2::identity() }),
        )?;
        Ok((target, brush))
    }
}

impl Renderer {
    pub fn new(hwnd: HWND, dpi: u32) -> Result<Self> {
        Self::new_with(hwnd, dpi, false)
    }

    /// Como `new`, pero pintando por CPU hasta que se llame a `upgrade_to_gpu`: la
    /// ventana principal arranca así para no esperar a que cargue el driver de la GPU
    /// (~300 ms la primera vez) antes de enseñar nada: pasa a la GPU justo después del
    /// primer pintado.
    pub fn new_software(hwnd: HWND, dpi: u32) -> Result<Self> {
        Self::new_with(hwnd, dpi, true)
    }

    /// Pasa a pintar con la GPU si se creó con `new_software`. `false` si ya lo hacía o
    /// si no se pudo (entonces sigue por CPU, que también funciona).
    pub fn is_software(&self) -> bool {
        self.software
    }

    pub fn upgrade_to_gpu(&mut self) -> bool {
        if !self.software {
            return false;
        }
        let ok = match unsafe { create_target(&self._d2d, self.hwnd, self.dpi, false) } {
            Ok((target, brush)) => {
                self.target = target;
                self.brush = brush;
                self.software = false;
                true
            }
            Err(_) => false,
        };
        ok
    }

    fn new_with(hwnd: HWND, dpi: u32, software: bool) -> Result<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let (target, brush) = create_target(&d2d, hwnd, dpi, software)?;

            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let sys_fonts: IDWriteFontCollection = {
                let mut collection: Option<IDWriteFontCollection> = None;
                dwrite.GetSystemFontCollection(&mut collection, false)?;
                collection.expect("GetSystemFontCollection debe devolver una colección")
            };
            let ui_family = resolve_family(&sys_fonts, "Segoe UI Variable Text", "Segoe UI");
            let mono_family = resolve_family(&sys_fonts, "Cascadia Mono", "Consolas");

            let ui_12 = make_format(&dwrite, &ui_family, layout::FONT_UI, DWRITE_FONT_WEIGHT_NORMAL)?;
            with_ellipsis_trimming(&dwrite, &ui_12)?;
            let ui_12_5 = make_format(&dwrite, &ui_family, layout::FONT_MENU, DWRITE_FONT_WEIGHT_NORMAL)?;
            let ui_11 = make_format(&dwrite, &ui_family, layout::FONT_STATUS, DWRITE_FONT_WEIGHT_NORMAL)?;
            let ui_11_5 = make_format(&dwrite, &ui_family, layout::FONT_PROMPT_LABEL, DWRITE_FONT_WEIGHT_NORMAL)?;
            let ui_13 = make_format(&dwrite, &ui_family, 13.0, DWRITE_FONT_WEIGHT_NORMAL)?;
            let ui_20_semibold = make_format(&dwrite, &ui_family, 20.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let ui_11_5_semibold =
                make_format(&dwrite, &ui_family, layout::FONT_PROMPT_LABEL, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let ui_9 = make_format(&dwrite, &ui_family, 9.0, DWRITE_FONT_WEIGHT_NORMAL)?;
            let ui_10_5 = make_format(&dwrite, &ui_family, 10.5, DWRITE_FONT_WEIGHT_NORMAL)?;
            let ui_12_semibold = make_format(&dwrite, &ui_family, 12.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let ui_12_5_semibold = make_format(&dwrite, &ui_family, 12.5, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let ui_18_semibold = make_format(&dwrite, &ui_family, 18.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let ui_13_semibold = make_format(&dwrite, &ui_family, 13.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let mono_13 = make_format(&dwrite, &mono_family, layout::FONT_MONO, DWRITE_FONT_WEIGHT_NORMAL)?;
            let mono_13_bold = make_format(&dwrite, &mono_family, layout::FONT_MONO, DWRITE_FONT_WEIGHT_BOLD)?;
            let mono_11 = make_format(&dwrite, &mono_family, layout::FONT_HINTS, DWRITE_FONT_WEIGHT_NORMAL)?;
            let mono_11_bold = make_format(&dwrite, &mono_family, layout::FONT_HINTS, DWRITE_FONT_WEIGHT_BOLD)?;
            let mono_11_5 = make_format(&dwrite, &mono_family, 11.5, DWRITE_FONT_WEIGHT_NORMAL)?;
            let mono_11_5_semibold = make_format(&dwrite, &mono_family, 11.5, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let mono_12 = make_format(&dwrite, &mono_family, layout::FONT_SUGGEST, DWRITE_FONT_WEIGHT_NORMAL)?;
            with_ellipsis_trimming(&dwrite, &mono_12)?;
            let mono_12_semibold = make_format(&dwrite, &mono_family, layout::FONT_SUGGEST, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let mono_12_5 = make_format(&dwrite, &mono_family, layout::FONT_PROMPT, DWRITE_FONT_WEIGHT_NORMAL)?;

            Ok(Self {
                _d2d: d2d,
                hwnd,
                target,
                device_lost: std::cell::Cell::new(false),
                software,
                dwrite,
                brush,
                fonts: Fonts {
                    ui_12,
                    ui_12_5,
                    ui_11,
                    ui_11_5,
                    ui_13,
                    ui_20_semibold,
                    ui_11_5_semibold,
                    ui_9,
                    ui_10_5,
                    ui_12_semibold,
                    ui_12_5_semibold,
                    ui_13_semibold,
                    ui_18_semibold,
                    mono_13,
                    mono_13_bold,
                    mono_11,
                    mono_11_bold,
                    mono_11_5,
                    mono_11_5_semibold,
                    mono_12,
                    mono_12_semibold,
                    mono_12_5,
                },
                dpi,
                hits: Vec::new(),
                pending_dropdown: None,
                fade: std::cell::Cell::new(1.0),
                lang: std::cell::Cell::new(notty_config::Lang::Es),
                hold_frame: std::cell::Cell::new(false),
                update_notice: None,
                update_panel: None,
                context_menu: None,
                tab_anim: Default::default(),
                tab_scroll_targets: (None, None),
                about: None,
                whats_new: None,
                path_copied: None,
                notice: None,
                menu_keys: Vec::new(),
                syntax_disabled: Vec::new(),
                mono_family,
                ui_family,
                font_scale: 1.0,
                panes: PaneView::default(),
            })
        }
    }

    /// Alto de línea del documento en DIPs (`--lh` de la maqueta), ya escalado por el
    /// zoom de texto (`set_font_scale`).
    pub fn line_height(&self) -> f32 {
        layout::LINE_H * self.font_scale
    }

    pub fn font_scale(&self) -> f32 {
        self.font_scale
    }

    /// Cambia la familia monoespaciada del editor (Ajustes → Apariencia → Fuentes).
    /// `primary` es el nombre a buscar; si el sistema no la tiene (p.ej. se
    /// desinstaló), cae a Cascadia Mono, y si tampoco, a Consolas.
    pub fn set_mono_family(&mut self, primary: &str) -> Result<()> {
        let sys_fonts: IDWriteFontCollection = unsafe {
            let mut collection: Option<IDWriteFontCollection> = None;
            self.dwrite.GetSystemFontCollection(&mut collection, false)?;
            collection.expect("GetSystemFontCollection debe devolver una colección")
        };
        let fallback = resolve_family(&sys_fonts, "Cascadia Mono", "Consolas");
        let family = resolve_family(&sys_fonts, primary, &fallback);
        let size = layout::FONT_MONO * self.font_scale;
        self.fonts.mono_13 = make_format(&self.dwrite, &family, size, DWRITE_FONT_WEIGHT_NORMAL)?;
        self.fonts.mono_13_bold = make_format(&self.dwrite, &family, size, DWRITE_FONT_WEIGHT_BOLD)?;
        self.mono_family = family;
        Ok(())
    }

    /// Aplica el zoom de texto del editor (Ctrl+=/Ctrl+-/Ctrl+0): recrea `mono_13` y
    /// `mono_13_bold` (cuerpo del documento, gutter y vista raw — no el resto de la
    /// interfaz) al nuevo tamaño. `scale` se recorta a un rango razonable para que no
    /// se pueda dejar el texto ilegible ni desbordar la ventana.
    pub fn set_font_scale(&mut self, scale: f32) -> Result<()> {
        let scale = scale.clamp(0.5, 3.0);
        let size = layout::FONT_MONO * scale;
        self.fonts.mono_13 = make_format(&self.dwrite, &self.mono_family, size, DWRITE_FONT_WEIGHT_NORMAL)?;
        self.fonts.mono_13_bold = make_format(&self.dwrite, &self.mono_family, size, DWRITE_FONT_WEIGHT_BOLD)?;
        self.font_scale = scale;
        Ok(())
    }

    pub fn set_dpi(&mut self, dpi: u32) {
        self.dpi = dpi;
        unsafe {
            self.target.SetDpi(dpi as f32, dpi as f32);
        }
    }

    /// `dpi / 96`: factor para convertir píxeles físicos del ratón a DIPs.
    pub fn scale(&self) -> f32 {
        self.dpi as f32 / 96.0
    }

    /// Tamaño del render target en DIPs (ya escalado por Direct2D con `SetDpi`).
    pub fn size_dips(&self) -> (f32, f32) {
        let s = unsafe { self.target.GetSize() };
        (s.width, s.height)
    }

    /// Ancho de un dígito con `mono_13`, usado para el hueco de los números de línea.
    fn digit_width(&self) -> f32 {
        self.measure("0", &self.fonts.mono_13).max(1.0)
    }

    /// Recorre las zonas registradas en el último `paint`, de la última a la primera
    /// (lo último dibujado queda encima). Si nada coincide: `Caption` en la franja de
    /// título, `None` en cualquier otro sitio.
    pub fn hit(&self, x_dip: f32, y_dip: f32) -> Hit {
        for &(r, h) in self.hits.iter().rev() {
            if r.contains(x_dip, y_dip) {
                return h;
            }
        }
        if y_dip < layout::TITLEBAR_H { Hit::Caption } else { Hit::None }
    }

    /// Formatos de texto fijos, para que `settings_window` (Task 11) dibuje con los
    /// mismos que el resto de la app en vez de crear los suyos.
    pub fn fonts(&self) -> &Fonts {
        &self.fonts
    }

    /// `BeginDraw` + `Clear`. Junto con `end_paint`, deja que `settings_window`
    /// reutilice el mismo `ID2D1HwndRenderTarget` sin repetir el `unsafe` de Direct2D.
    pub fn begin_paint(&self, bg: Rgba) {
        unsafe {
            self.target.BeginDraw();
            self.target.Clear(Some(&color(bg)));
        }
    }

    /// `BeginDraw` sin `Clear`: para pintar una capa por encima de lo que ya se
    /// dibujó en el `paint()` normal de esta misma pasada (el recorrido guiado,
    /// ver `tour.rs`), en vez de repetir todo el dibujo del documento.
    pub(crate) fn begin_overlay(&self) {
        if self.hold_frame.get() {
            return;
        }
        unsafe {
            self.target.BeginDraw();
        }
    }

    pub(crate) fn hold_next_frame(&self) {
        self.hold_frame.set(true);
    }

    /// Vela oscura sobre `full` con un agujero de esquinas redondeadas en `hole`, más
    /// un resplandor suave de `ring` alrededor del agujero. Se resuelve con una resta
    /// de geometrías D2D (rectángulo completo menos rectángulo redondeado) en vez de
    /// cuatro franjas rectangulares, para que las esquinas del foco queden redondeadas
    /// de verdad en vez de en escuadra.
    ///
    /// El resplandor era antes un único trazo opaco de 2px (`box-shadow:0 0 0 2px
    /// #73b6fa`, un contorno azul duro) que el usuario reportó como un "flash": al
    /// cambiar de foco aparecía de golpe, sin gradiente, y contrastaba fuerte contra
    /// la vela. Aquí se sustituye por varias capas concéntricas cada vez más anchas y
    /// tenues (misma técnica que `draw_popup_shadow` para las sombras de popups), que
    /// se difuminan hacia afuera en vez de marcar un borde neto.
    pub(crate) fn fill_veil_with_hole(&self, full: Rect, hole: Rect, hole_radius: f32, veil: Rgba, ring: Rgba) {
        const GLOW_LAYERS: [(f32, f32); 5] = [
            // (ancho de trazo, multiplicador de alpha), de más ancho/tenue a más
            // fino/marcado: el resultado es un halo suave en vez de un anillo duro.
            (10.0, 0.05),
            (7.0, 0.09),
            (5.0, 0.14),
            (3.0, 0.22),
            (1.5, 0.32),
        ];
        unsafe {
            let Ok(outer) = self._d2d.CreateRectangleGeometry(&rect_of(full)) else { return };
            let Ok(inner) = self._d2d.CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
                rect: rect_of(hole),
                radiusX: hole_radius,
                radiusY: hole_radius,
            }) else {
                return;
            };
            let Ok(path) = self._d2d.CreatePathGeometry() else { return };
            if let Ok(sink) = path.Open() {
                let _ = outer.CombineWithGeometry(&inner, D2D1_COMBINE_MODE_EXCLUDE, None, 0.25, &sink);
                let _ = sink.Close();
            }
            self.brush.SetColor(&color(veil.faded(self.fade.get())));
            let _ = self.target.FillGeometry(&path, &self.brush, None);

            let hole_rect = D2D1_ROUNDED_RECT { rect: rect_of(hole), radiusX: hole_radius, radiusY: hole_radius };
            for (width, alpha_mul) in GLOW_LAYERS {
                let c = Rgba(ring.0, ring.1, ring.2, ring.3 * alpha_mul);
                self.brush.SetColor(&color(c.faded(self.fade.get())));
                self.target.DrawRoundedRectangle(&hole_rect, &self.brush, width, None);
            }
        }
    }

    pub fn end_paint(&self) {
        self.hold_frame.set(false);
        let r = unsafe { self.target.EndDraw(None, None) };
        self.note_end_draw(r);
    }

    /// Si `EndDraw` pide recrear el target, lo apunta y pide otro `WM_PAINT`: en vez de
    /// quedarse en negro, el siguiente pintado empieza por `recover_device`.
    fn note_end_draw(&self, r: Result<()>) {
        if r.is_err_and(|e| e.code() == windows::Win32::Foundation::D2DERR_RECREATE_TARGET) {
            self.device_lost.set(true);
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.hwnd), None, false);
            }
        }
    }

    /// Recrea el render target y la brocha si el dispositivo se perdió. Devuelve `true`
    /// si lo hizo: quien guarde recursos propios creados con este `Renderer` (bitmaps)
    /// debe soltarlos y volver a pedirlos. Hay que llamarla antes de `begin_paint`.
    pub fn recover_device(&mut self) -> bool {
        if !self.device_lost.get() {
            return false;
        }
        match unsafe { create_target(&self._d2d, self.hwnd, self.dpi, self.software) } {
            Ok((target, brush)) => {
                self.target = target;
                self.brush = brush;
                self.device_lost.set(false);
                true
            }
            Err(_) => false,
        }
    }

    /// Transformación de mundo para lo que se dibuje a partir de ahora (hasta el
    /// siguiente `set_transform`/`reset_transform`): usada por `settings_window` para
    /// la escala de apertura (Task 5 del plan de animaciones). No afecta al
    /// hit-testing: los rects guardados en `hits` siguen en coordenadas lógicas.
    pub fn set_transform(&self, m: Matrix3x2) {
        unsafe {
            self.target.SetTransform(&m);
        }
    }

    pub fn reset_transform(&self) {
        unsafe {
            self.target.SetTransform(&Matrix3x2::identity());
        }
    }

    /// Ver el campo `fade`: multiplica la opacidad de todo lo dibujado después de esta
    /// llamada hasta el próximo `set_fade`. `1.0` (el valor por defecto tras cada
    /// `Renderer::new`) no cambia nada.
    pub fn set_fade(&self, factor: f32) {
        self.fade.set(factor.clamp(0.0, 1.0));
    }

    pub fn fade(&self) -> f32 {
        self.fade.get()
    }

    pub fn set_lang(&self, lang: notty_config::Lang) {
        self.lang.set(lang);
    }

    pub fn lang(&self) -> notty_config::Lang {
        self.lang.get()
    }

    /// Traduce `es` al idioma puesto por `set_lang` (ver `strings::tr`).
    pub fn tr<'s>(&self, es: &'s str) -> &'s str {
        crate::strings::tr(self.lang.get(), es)
    }

    // --- Helpers de dibujo con la brocha única --------------------------------------

    pub fn fill(&self, r: Rect, c: Rgba) {
        if r.is_empty() {
            return;
        }
        unsafe {
            self.brush.SetColor(&color(c.faded(self.fade.get())));
            self.target.FillRectangle(&rect_of(r), &self.brush);
        }
    }

    pub fn fill_round(&self, r: Rect, radius: f32, c: Rgba) {
        if r.is_empty() {
            return;
        }
        unsafe {
            self.brush.SetColor(&color(c.faded(self.fade.get())));
            self.target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT { rect: rect_of(r), radiusX: radius, radiusY: radius },
                &self.brush,
            );
        }
    }

    /// Sombra difusa bajo un popup (desplegable de menú, sugerencias, `Select`): en vez
    /// de un único rect 2px más grande y opaco (que se veía como un anillo negro duro
    /// pegado al borde), varias capas cada vez más grandes/tenues y desplazadas hacia
    /// abajo, como la sombra real de una ventana flotante en vez de un contorno plano.
    pub fn draw_popup_shadow(&self, r: Rect, radius: f32, fade: f32, shadow: Rgba) {
        const LAYERS: [(f32, f32, f32); 4] = [
            // (desplazamiento en Y, crecimiento del rect, multiplicador de alpha)
            (1.0, 1.0, 0.5),
            (2.0, 3.0, 0.32),
            (4.0, 6.0, 0.18),
            (7.0, 10.0, 0.10),
        ];
        for (dy, grow, alpha_mul) in LAYERS {
            let c = Rgba(shadow.0, shadow.1, shadow.2, shadow.3 * alpha_mul);
            self.fill_round(
                Rect::new(r.left - grow, r.top - grow + dy, r.right + grow, r.bottom + grow + dy),
                radius + grow,
                c.faded(fade),
            );
        }
    }

    #[allow(dead_code)]
    pub fn stroke_line(&self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, c: Rgba) {
        unsafe {
            self.brush.SetColor(&color(c.faded(self.fade.get())));
            self.target.DrawLine(Vector2 { X: x0, Y: y0 }, Vector2 { X: x1, Y: y1 }, &self.brush, width, None);
        }
    }

    #[allow(dead_code)]
    pub fn stroke_rect(&self, r: Rect, width: f32, c: Rgba) {
        if r.is_empty() {
            return;
        }
        unsafe {
            self.brush.SetColor(&color(c.faded(self.fade.get())));
            self.target.DrawRectangle(&rect_of(r), &self.brush, width, None);
        }
    }

    /// Círculo centrado en `(cx, cy)` de radio `radius`, sin relleno.
    pub fn stroke_circle(&self, cx: f32, cy: f32, radius: f32, width: f32, c: Rgba) {
        unsafe {
            self.brush.SetColor(&color(c.faded(self.fade.get())));
            let ellipse = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: radius, radiusY: radius };
            self.target.DrawEllipse(&ellipse, &self.brush, width, None);
        }
    }

    /// Círculo relleno centrado en `(cx, cy)` de radio `radius` (el punto del icono).
    pub fn fill_circle(&self, cx: f32, cy: f32, radius: f32, c: Rgba) {
        unsafe {
            self.brush.SetColor(&color(c.faded(self.fade.get())));
            let ellipse = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: radius, radiusY: radius };
            self.target.FillEllipse(&ellipse, &self.brush);
        }
    }

    #[allow(dead_code)]
    pub fn stroke_round_rect(&self, r: Rect, radius: f32, width: f32, c: Rgba) {
        if r.is_empty() {
            return;
        }
        unsafe {
            self.brush.SetColor(&color(c.faded(self.fade.get())));
            self.target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT { rect: rect_of(r), radiusX: radius, radiusY: radius },
                &self.brush,
                width,
                None,
            );
        }
    }

    /// Dibuja `s` con `fmt` en `r`, centrado verticalmente en su alto (recorte con
    /// «…» si el formato lo tiene configurado).
    #[allow(dead_code)]
    pub fn text(&self, s: &str, fmt: &IDWriteTextFormat, r: Rect, c: Rgba) {
        self.text_aligned(s, fmt, r, c, false);
    }

    /// Igual que `text`, pero alineado a la derecha de `r`.
    #[allow(dead_code)]
    pub fn text_right(&self, s: &str, fmt: &IDWriteTextFormat, r: Rect, c: Rgba) {
        self.text_aligned(s, fmt, r, c, true);
    }

    fn text_aligned(&self, s: &str, fmt: &IDWriteTextFormat, r: Rect, c: Rgba, right: bool) {
        let w = wide(s);
        if w.is_empty() || r.width() <= 0.0 {
            return;
        }
        unsafe {
            if let Ok(layout) = self.dwrite.CreateTextLayout(&w, fmt, r.width().max(0.0), r.height().max(0.0)) {
                let _ = layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                if right {
                    let _ = layout.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING);
                }
                self.brush.SetColor(&color(c.faded(self.fade.get())));
                self.target.DrawTextLayout(
                    Vector2 { X: r.left, Y: r.top },
                    &layout,
                    &self.brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                );
            }
        }
    }

    /// Igual que `text`, pero centrado también en horizontal (etiquetas de botón).
    pub fn text_center(&self, s: &str, fmt: &IDWriteTextFormat, r: Rect, c: Rgba) {
        let w = wide(s);
        if w.is_empty() || r.width() <= 0.0 {
            return;
        }
        unsafe {
            if let Ok(layout) = self.dwrite.CreateTextLayout(&w, fmt, r.width(), r.height().max(0.0)) {
                let _ = layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                let _ = layout.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                self.brush.SetColor(&color(c.faded(self.fade.get())));
                self.target.DrawTextLayout(Vector2 { X: r.left, Y: r.top }, &layout, &self.brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
            }
        }
    }

    fn wrapped_layout(&self, s: &str, fmt: &IDWriteTextFormat, width: f32, line_h: Option<f32>) -> Option<IDWriteTextLayout> {
        let w = wide(s);
        if w.is_empty() || width <= 0.0 {
            return None;
        }
        unsafe {
            let layout = self.dwrite.CreateTextLayout(&w, fmt, width, f32::MAX).ok()?;
            let _ = layout.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
            let _ = layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
            if let Some(lh) = line_h {
                let _ = layout.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, lh, lh * 0.8);
            }
            Some(layout)
        }
    }

    /// Texto con salto de línea por palabras, pegado arriba a la izquierda de `r`.
    /// `line_h` fuerza un interlineado fijo (el `line-height` de CSS).
    pub fn text_wrapped(&self, s: &str, fmt: &IDWriteTextFormat, r: Rect, c: Rgba, line_h: Option<f32>) {
        let Some(layout) = self.wrapped_layout(s, fmt, r.width(), line_h) else { return };
        unsafe {
            self.brush.SetColor(&color(c.faded(self.fade.get())));
            self.target.DrawTextLayout(Vector2 { X: r.left, Y: r.top }, &layout, &self.brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        }
    }

    /// Alto que ocupa `s` con salto de línea en un ancho `width` (ver `text_wrapped`).
    pub fn measure_wrapped(&self, s: &str, fmt: &IDWriteTextFormat, width: f32, line_h: Option<f32>) -> f32 {
        let Some(layout) = self.wrapped_layout(s, fmt, width, line_h) else { return 0.0 };
        unsafe {
            let mut metrics = Default::default();
            if layout.GetMetrics(&mut metrics).is_ok() {
                return metrics.height;
            }
        }
        0.0
    }

    /// Igual que `text`, pero con los glifos a color de la fuente (emoji de Segoe UI
    /// Emoji en vez de su versión monocroma).
    pub fn text_color_font(&self, s: &str, fmt: &IDWriteTextFormat, r: Rect, c: Rgba) {
        let w = wide(s);
        if w.is_empty() || r.width() <= 0.0 {
            return;
        }
        unsafe {
            if let Ok(layout) = self.dwrite.CreateTextLayout(&w, fmt, r.width(), r.height().max(0.0)) {
                let _ = layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                // Los glifos de color (emoji) ignoran el alfa del pincel: para que se
                // fundan con el resto hace falta una capa con opacidad.
                let alpha = c.3 * self.fade.get();
                let layer = if alpha < 0.999 { self.target.CreateLayer(None).ok() } else { None };
                if let Some(l) = &layer {
                    let params = windows::Win32::Graphics::Direct2D::D2D1_LAYER_PARAMETERS {
                        contentBounds: rect_of(r),
                        geometricMask: std::mem::ManuallyDrop::new(None),
                        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                        maskTransform: windows_numerics::Matrix3x2::identity(),
                        opacity: alpha.max(0.0),
                        opacityBrush: std::mem::ManuallyDrop::new(None),
                        layerOptions: windows::Win32::Graphics::Direct2D::D2D1_LAYER_OPTIONS_NONE,
                    };
                    self.target.PushLayer(&params, l);
                }
                self.brush.SetColor(&color(Rgba(c.0, c.1, c.2, if layer.is_some() { 1.0 } else { alpha })));
                self.target.DrawTextLayout(
                    Vector2 { X: r.left, Y: r.top },
                    &layout,
                    &self.brush,
                    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                );
                if layer.is_some() {
                    self.target.PopLayer();
                }
            }
        }
    }

    /// Recorta todo lo que se dibuje hasta el `pop_clip` correspondiente a `r`.
    pub fn push_clip(&self, r: Rect) {
        unsafe {
            self.target.PushAxisAlignedClip(&rect_of(r), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
    }

    pub fn pop_clip(&self) {
        unsafe {
            self.target.PopAxisAlignedClip();
        }
    }

    fn path_of(&self, points: &[(f32, f32)], closed: bool) -> Option<ID2D1PathGeometry> {
        let (first, rest) = points.split_first()?;
        unsafe {
            let path = self._d2d.CreatePathGeometry().ok()?;
            let sink = path.Open().ok()?;
            let begin = if closed { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW };
            sink.BeginFigure(Vector2 { X: first.0, Y: first.1 }, begin);
            let rest: Vec<Vector2> = rest.iter().map(|p| Vector2 { X: p.0, Y: p.1 }).collect();
            sink.AddLines(&rest);
            sink.EndFigure(if closed { D2D1_FIGURE_END_CLOSED } else { D2D1_FIGURE_END_OPEN });
            sink.Close().ok()?;
            Some(path)
        }
    }

    /// Polígono relleno (p.ej. el escudo UAC).
    pub fn fill_polygon(&self, points: &[(f32, f32)], c: Rgba) {
        let Some(path) = self.path_of(points, true) else { return };
        unsafe {
            self.brush.SetColor(&color(c.faded(self.fade.get())));
            self.target.FillGeometry(&path, &self.brush, None);
        }
    }

    /// Trazo abierto con extremos y uniones redondeados (chevrones, ✓, iconos).
    pub fn stroke_polyline(&self, points: &[(f32, f32)], width: f32, c: Rgba) {
        let Some(path) = self.path_of(points, false) else { return };
        unsafe {
            let props = D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 10.0,
                dashStyle: D2D1_DASH_STYLE_SOLID,
                dashOffset: 0.0,
            };
            let style = self._d2d.CreateStrokeStyle(&props, None).ok();
            self.brush.SetColor(&color(c.faded(self.fade.get())));
            self.target.DrawGeometry(&path, &self.brush, width, style.as_ref());
        }
    }

    /// Ancho de `s` con `fmt`, sin límite (para medir nombres de pestaña, etc.).
    pub fn measure(&self, s: &str, fmt: &IDWriteTextFormat) -> f32 {
        let w = wide(s);
        if w.is_empty() {
            return 0.0;
        }
        unsafe {
            if let Ok(layout) = self.dwrite.CreateTextLayout(&w, fmt, f32::MAX, f32::MAX) {
                let mut metrics = Default::default();
                if layout.GetMetrics(&mut metrics).is_ok() {
                    return metrics.widthIncludingTrailingWhitespace;
                }
            }
        }
        0.0
    }

    /// Rectángulo del cuerpo y ancho del canal de números para el tamaño actual de la
    /// ventana, `total_lines` líneas y `doc_count`/`menu_bar_visible` (mismas bandas
    /// que resuelve `paint`). Lo usa `window.rs` para el hit-testing del ratón sobre
    /// el documento y para `WM_SIZE`.
    pub fn body_and_gutter(&self, ui: &UiConfig, doc_count: usize, menu_bar_visible: bool, total_lines: usize, is_raw: bool) -> (Rect, f32) {
        let (w, h) = self.size_dips();
        let bands = Self::resolve_bands(ui, doc_count, menu_bar_visible);
        let frame = layout::frame(w, h, bands);
        let gutter_w =
            if ui.line_numbers && !is_raw { layout::gutter_width(total_lines, self.digit_width()) } else { 0.0 };
        (frame.body, gutter_w)
    }

    /// El `Frame` completo (todas las franjas, no solo `body`) para el tamaño y
    /// config actuales: lo usa `tour.rs` para saber dónde está cada elemento
    /// señalado, sin repetir la resolución de `Bands` que ya hace `paint`.
    pub fn current_frame(&self, ui: &UiConfig, doc_count: usize, menu_bar_visible: bool) -> layout::Frame {
        let (w, h) = self.size_dips();
        let bands = Self::resolve_bands(ui, doc_count, menu_bar_visible);
        layout::frame(w, h, bands)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        unsafe {
            let _ = self.target.Resize(&D2D_SIZE_U { width, height });
        }
    }

    /// Traduce un punto del cliente (en DIPs, origen arriba-izquierda) al índice de
    /// char del documento más cercano, usando `state.viewport` para saber qué línea es
    /// cada fila. Con `wrap` cada línea puede ocupar más de una fila visual, así que en
    /// vez de dividir `y` entre `line_height()` hay que acumular el alto real de cada
    /// línea desde `first_line` hasta dar con la que contiene el clic (igual que hace
    /// `paint` al dibujar).
    pub fn char_index_at(&self, state: &EditorState, body: Rect, gutter_w: f32, x: f32, y: f32, wrap: bool, md_inline: bool) -> usize {
        if state.md_preview {
            return self.md_char_index_at(state, body, body.left + layout::TEXT_PAD_L, md_inline, x, y);
        }
        let buf = state.doc.buffer();
        let total = buf.len_lines();
        let text_pad = body.left + gutter_w + layout::TEXT_PAD_L;

        if !wrap {
            let range = state.viewport.range(total);
            let row = ((y - body.top - layout::TEXT_PAD_T) / self.line_height()).floor().max(0.0) as usize;
            let line = (state.viewport.first_line + row).min(total.saturating_sub(1)).max(range.start);
            return self.hit_test_line(&buf, line, total, text_pad, x, 0.0, None);
        }

        let wrap_width = (body.right - text_pad).max(60.0);
        let mut row_top = body.top + layout::TEXT_PAD_T;
        let mut line = state.viewport.first_line.min(total.saturating_sub(1));
        loop {
            let start = buf.line_start(line);
            let end = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
            let text: String = buf.slice(start..end).trim_end_matches(['\r', '\n']).to_string();
            let row_h = if text.is_empty() {
                self.line_height()
            } else {
                let w = wide(&text);
                unsafe {
                    self.dwrite
                        .CreateTextLayout(&w, &self.fonts.mono_13, wrap_width, 20_000.0)
                        .ok()
                        .map(|l| {
                            let _ = l.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
                            let mut m: DWRITE_TEXT_METRICS = Default::default();
                            l.GetMetrics(&mut m).ok().map(|_| m.lineCount.max(1)).unwrap_or(1) as f32 * self.line_height()
                        })
                        .unwrap_or(self.line_height())
                }
            };
            if y < row_top + row_h || line + 1 >= total {
                return self.hit_test_line(&buf, line, total, text_pad, x, y - row_top, Some(wrap_width));
            }
            row_top += row_h;
            line += 1;
        }
    }

    /// Índice de char dentro de `line` más cercano a `(x, y_within)` (origen en el
    /// principio de esa línea); `wrap_width`, si hay, se pasa tal cual a `CreateTextLayout`
    /// para que el hit-test caiga en la fila visual correcta de una línea que hace wrap.
    fn hit_test_line(&self, buf: &notty_core::Buffer, line: usize, total: usize, text_pad: f32, x: f32, y_within: f32, wrap_width: Option<f32>) -> usize {
        let start = buf.line_start(line);
        let end = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
        let text: String = buf.slice(start..end).trim_end_matches(['\r', '\n']).to_string();
        if text.is_empty() {
            return start;
        }
        let w = wide(&text);
        let max_w = wrap_width.unwrap_or(f32::MAX);
        let Ok(text_layout) = (unsafe { self.dwrite.CreateTextLayout(&w, &self.fonts.mono_13, max_w, 20_000.0) }) else {
            return start;
        };
        if wrap_width.is_some() {
            let _ = unsafe { text_layout.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP) };
        }
        let mut trailing = windows::core::BOOL(0);
        let mut inside = windows::core::BOOL(0);
        let mut metrics = Default::default();
        if unsafe { text_layout.HitTestPoint(x - text_pad, y_within, &mut trailing, &mut inside, &mut metrics) }.is_ok() {
            let utf16_offset = metrics.textPosition as usize + if trailing.as_bool() { 1 } else { 0 };
            let prefix = String::from_utf16_lossy(&w[..utf16_offset.min(w.len())]);
            start + prefix.chars().count()
        } else {
            start
        }
    }

    /// Fracción visible de cada banda si hay una transición de bandas en curso
    /// (`None` en reposo: se usan las `bands` tal cual).
    fn chrome_frac(view: &ViewState, bands: layout::Bands) -> Option<layout::BandFrac> {
        match (view.chrome_from, view.chrome_fade) {
            (Some(from), Some(t)) if t < 1.0 => Some(from.lerp(layout::BandFrac::of(bands), t)),
            _ => None,
        }
    }

    /// Qué franjas se muestran, resuelto a partir de `ui`, el número de documentos y
    /// si el menú `Alt` está desplegado (maqueta, `tabsVisible()` línea 462).
    /// `pub(crate)`: `window.rs` también la usa, para detectar cuándo un cambio de
    /// ajuste hace aparecer/desaparecer una banda entera y fundirla en vez de que
    /// salte (Task 3 del plan de animaciones, ver `WindowState::last_bands`).
    pub(crate) fn resolve_bands(ui: &UiConfig, doc_count: usize, menu_bar_visible: bool) -> layout::Bands {
        use notty_config::{Files, TabsPosition};
        let tabs_in_title = ui.files != Files::Buffers
            && (ui.tabs_position == TabsPosition::Title
                || (ui.tabs_position == TabsPosition::Auto && doc_count > 1));
        let tabs_below = ui.files != Files::Buffers && ui.tabs_position == TabsPosition::Below;
        layout::Bands {
            tabs_in_title,
            menubar: menu_bar_visible,
            tabs_below,
            hints: ui.hints_bar && !ui.merged_command_line,
            merged_status: ui.merged_command_line,
        }
    }

    /// Dibuja fondo + documento del `Workspace` activo, con las medidas y colores de
    /// la maqueta: barra de título propia (icono, pestañas o título, botones), barra
    /// de menús, pestañas debajo, canal de números, documento y barra de atajos. La
    /// barra de estado / prompts y la vista raw llegan en las Tasks 8-9.
    pub fn paint(
        &mut self,
        ws: &Workspace,
        ui: &UiConfig,
        view: &ViewState,
        ligature_table: &[(String, char)],
    ) {
        self.recover_device();
        self.hits.clear();
        self.pending_dropdown = None;
        let lang = crate::lang::resolve(ui.lang);
        self.set_lang(lang);
        // Tema y acento pueden estar cambiando a la vez (poco probable, pero no hay
        // por qué prohibirlo): cada transición se funde por separado, en cadena, para
        // que las dos acaben siempre en `(view.dark, ui.accent)` sin más lío.
        let (from_dark, t_theme) = view.theme_from.map_or((view.dark, 1.0), |(d, t)| (d, t));
        let (from_accent, t_accent) = view.accent_from.map_or((ui.accent, 1.0), |(a, t)| (a, t));
        let pal_after_theme = theme::palette(from_dark, from_accent).mix(&theme::palette(view.dark, from_accent), t_theme);
        let pal_owned = pal_after_theme.mix(&theme::palette(view.dark, ui.accent), t_accent);
        let pal = &pal_owned;
        let state = ws.active();
        let is_raw = state.raw.is_some();

        let (search_matches, search_current): (std::rc::Rc<Vec<std::ops::Range<usize>>>, Option<usize>) = match &ws.prompt {
            crate::Prompt::Find(s) | crate::Prompt::Replace(s) => {
                (s.matches(&state.doc).unwrap_or_default(), Some(s.current))
            }
            _ => (Default::default(), None),
        };

        let (w, h) = self.size_dips();
        let bands = Self::resolve_bands(ui, ws.len(), view.menu_bar_visible);
        let frame = match Self::chrome_frac(view, bands) {
            Some(f) => layout::frame_frac(w, h, f),
            None => layout::frame(w, h, bands),
        };
        // El botón de la barra de título queda cubierto por las pestañas/botones que
        // se registran después (hit() prioriza lo último registrado).
        self.hits.push((frame.titlebar, Hit::Caption));
        self.hits.push((frame.body, Hit::Body));

        unsafe {
            self.target.BeginDraw();
            self.target.Clear(Some(&color(pal.surface)));

            self.draw_titlebar(ws, ui, view, pal, frame);
            // Bandas que acaban de aparecer por un cambio de ajuste (Ajustes → barra de
            // menús / "Posición de las pestañas" / ...) funden su contenido en vez de
            // saltar directamente a opaco (Task 3 del plan de animaciones): la banda que
            // ya estaba visible antes de la transición (o fuera de una transición, el
            // caso de siempre) se pinta a fundido 1.0, sin coste extra. Desaparecer
            // sigue siendo instantáneo (dejar de reservar espacio en `frame` moviendo el
            // resto del contenido de golpe es harina de otro costal) — ver el módulo.
            // Una banda a medio desplegar se dibuja a su altura completa, anclada por
            // abajo y recortada a lo que ya ocupa: entra como una persiana, con el
            // contenido fundiéndose a la vez.
            if !frame.menubar.is_empty() {
                let k = frame.menubar.height() / layout::MENUBAR_H;
                let mut full = frame;
                full.menubar = Rect::new(0.0, frame.menubar.bottom - layout::MENUBAR_H, w, frame.menubar.bottom);
                self.push_clip(frame.menubar);
                self.set_fade(k);
                self.draw_menubar(view, pal, full, lang);
                self.set_fade(1.0);
                self.pop_clip();
            }
            if !frame.tabs_below.is_empty() {
                let k = frame.tabs_below.height() / layout::TABS_BELOW_H;
                self.push_clip(frame.tabs_below);
                self.fill(frame.tabs_below, pal.chrome);
                self.set_fade(k);
                self.draw_tabs_row(ws, view, pal, layout::TABS_BELOW_PAD_X, w - layout::TABS_BELOW_PAD_X, frame.tabs_below.bottom, ui.tab_icons);
                self.set_fade(1.0);
                self.pop_clip();
            }

            // Un documento por panel (`Files::Splits`); sin paneles, el activo en todo el cuerpo.
            let (pane_docs, pane_focus, weights) = if self.panes.docs.len() > 1 {
                (self.panes.docs.clone(), self.panes.focus, self.panes.weights.clone())
            } else {
                (vec![ws.active_index()], 0, vec![1.0])
            };
            let split = pane_docs.len() > 1;
            let pane_rects = crate::splits::pane_rects(frame.body, &weights);
            for (pi, (&doc_i, &pane)) in pane_docs.iter().zip(&pane_rects).enumerate() {
            let focused = pi == pane_focus;
            let state = ws.iter().nth(doc_i).unwrap_or(state);
            let mut frame = frame;
            frame.body = pane;
            let buf = state.doc.buffer();
            let is_raw = state.raw.is_some();
            let total_lines = buf.len_lines();
            let range = state.viewport.range(total_lines);
            let sel = state.doc.selection();
            let sel_range = sel.range();
            // Sin caret en los paneles sin foco.
            let head = if focused { sel.head } else { usize::MAX };
            let cursor_line = state.doc.line_col().0;
            let (search_matches, search_current) =
                if focused { (search_matches.as_slice(), search_current) } else { (&[][..], None) };
            let gutter_w = if ui.line_numbers && !is_raw && !state.md_preview { layout::gutter_width(total_lines, self.digit_width()) } else { 0.0 };
            let text_pad = frame.body.left + gutter_w + layout::TEXT_PAD_L;
            if split {
                self.push_clip(pane);
            }

            if is_raw {
                if let Some(raw) = &state.raw {
                    self.draw_hex(raw, state.raw_cursor, state.raw_pending_nibble, state.viewport.first_line, pal, frame);
                }
            } else if state.md_preview {
                let inline_mode = ui.md_preview_style == notty_config::MdPreviewStyle::Inline;
                self.paint_md(state, pal, frame.body, text_pad, head, inline_mode, search_matches, search_current);
            } else {
            let spans = if ui.syntax_highlight {
                state.syntax.line_spans(&state.doc, state.path.as_deref(), range.clone(), &self.syntax_disabled)
            } else {
                Vec::new()
            };
            let ligature_table: &[(String, char)] = if ui.ligatures { ligature_table } else { &[] };
            // Un pincel por color de `syntax::NAMES`, creado la primera vez que hace falta.
            let mut syn_brushes: Vec<Option<ID2D1SolidColorBrush>> = vec![None; crate::syntax::NAMES.len()];
            let mut y = frame.body.top + layout::TEXT_PAD_T;
            // Ajustar texto a la ventana (Ajustes → Ventana): DirectWrite reparte la
            // línea en varias filas visuales dentro del mismo layout en vez de salirse
            // por el borde; sin esto, ancho `f32::MAX` es "nunca hagas wrap" (de siempre).
            let wrap_width = if ui.wrap { (frame.body.right - text_pad).max(60.0) } else { f32::MAX };
            for line in range.clone() {
                let start = buf.line_start(line);
                let full_end = if line + 1 < total_lines { buf.line_start(line + 1) } else { buf.len_chars() };
                let text: String = buf.slice(start..full_end).trim_end_matches(['\r', '\n']).to_string();
                let text_end = start + text.chars().count();
                // Ligaduras: mismo número de caracteres que `text` (rellena con espacios),
                // así que las posiciones que calcula `hit_test_x` sobre `text` (cursor,
                // selección, búsqueda) siguen valiendo tal cual sobre este layout.
                let display = if ligature_table.is_empty() { text.clone() } else { crate::ligature::display_text(&text, &ligature_table) };
                let w16 = wide(&display);

                // Con wrap, el alto de verdad de la línea no se sabe hasta crear el layout
                // (depende de cuántas filas visuales le hacen falta): de ahí un `maxHeight`
                // generoso en vez de `line_height()`, y `row_h` calculado con `GetMetrics`
                // después. Se redondea a un múltiplo de `line_height()` (en vez del alto que
                // reporta DirectWrite) para que cada fila visual mida lo mismo que las demás
                // líneas de la interfaz (numeración, cursor, cualquier otro sitio que ya
                // asume `line_height()` por fila).
                let text_layout = if w16.is_empty() {
                    None
                } else {
                    let l = self.dwrite.CreateTextLayout(&w16, &self.fonts.mono_13, wrap_width, if ui.wrap { 20_000.0 } else { self.line_height() }).ok();
                    // `mono_13` se crea con `DWRITE_WORD_WRAPPING_NO_WRAP` (de siempre, para
                    // que un `maxWidth` corto no recorte nada sin querer); con `ui.wrap` hay
                    // que pedirlo explícito en el layout, el ancho por sí solo no alcanza.
                    if ui.wrap {
                        if let Some(l) = &l {
                            let _ = l.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
                        }
                    }
                    l
                };
                let row_h = if ui.wrap {
                    text_layout
                        .as_ref()
                        .and_then(|l| {
                            let mut m: DWRITE_TEXT_METRICS = Default::default();
                            l.GetMetrics(&mut m).ok().map(|_| m.lineCount.max(1))
                        })
                        .unwrap_or(1) as f32
                        * self.line_height()
                } else {
                    self.line_height()
                };

                if ui.line_numbers && !is_raw {
                    let num = (line + 1).to_string();
                    let num_color = if line == cursor_line { pal.text } else { pal.text_3 };
                    self.text_right(
                        &num,
                        &self.fonts.mono_13,
                        Rect::new(frame.body.left, y, frame.body.left + gutter_w - layout::GUTTER_PAD_R, y + self.line_height()),
                        num_color,
                    );
                }

                if !sel.is_empty() && sel_range.start < full_end && sel_range.end > start {
                    let clamp_start = sel_range.start.max(start);
                    let clamp_end = sel_range.end.min(text_end);
                    if ui.wrap {
                        if let Some(l) = text_layout.as_ref() {
                            let rects = hit_test_range_rects(l, &text, clamp_start - start, clamp_end - start, text_pad, y);
                            let n = rects.len();
                            for (i, (rx, ry, rw, rh)) in rects.into_iter().enumerate() {
                                let mut x1 = rx + rw;
                                if sel_range.end > text_end && i + 1 == n {
                                    x1 = x1.max(rx) + 6.0;
                                }
                                self.fill(Rect::new(rx, ry, x1.max(rx + 2.0), ry + rh), pal.accent_soft);
                            }
                        }
                    } else {
                        let x0 = if let (true, Some(l)) = (clamp_end > clamp_start, text_layout.as_ref()) {
                            hit_test_x(l, &text, clamp_start - start, text_pad)
                        } else {
                            text_pad
                        };
                        let mut x1 = if let (true, Some(l)) = (clamp_end > clamp_start, text_layout.as_ref()) {
                            hit_test_x(l, &text, clamp_end - start, text_pad)
                        } else {
                            text_pad
                        };
                        if sel_range.end > text_end {
                            x1 = x1.max(x0) + 6.0;
                        }
                        x1 = x1.max(x0 + 2.0);
                        self.fill(Rect::new(x0, y, x1, y + self.line_height()), pal.accent_soft);
                    }
                }

                for (mi, m) in search_matches.iter().enumerate() {
                    if m.start >= full_end || m.end <= start {
                        continue;
                    }
                    let clamp_start = m.start.max(start);
                    let clamp_end = m.end.min(text_end);
                    if clamp_end <= clamp_start {
                        continue;
                    }
                    let c = if Some(mi) == search_current { pal.mark_cur } else { pal.mark };
                    if ui.wrap {
                        if let Some(l) = text_layout.as_ref() {
                            let rects = hit_test_range_rects(l, &text, clamp_start - start, clamp_end - start, text_pad, y);
                            for (rx, ry, rw, rh) in rects {
                                self.fill_round(Rect::new(rx, ry, (rx + rw).max(rx + 2.0), ry + rh), 2.0, c);
                            }
                        }
                    } else {
                        let x0 = text_layout.as_ref().map(|l| hit_test_x(l, &text, clamp_start - start, text_pad)).unwrap_or(text_pad);
                        let x1 = text_layout.as_ref().map(|l| hit_test_x(l, &text, clamp_end - start, text_pad)).unwrap_or(text_pad);
                        self.fill_round(Rect::new(x0, y, x1.max(x0 + 2.0), y + self.line_height()), 2.0, c);
                    }
                }

                if let Some(l) = &text_layout {
                    for s in spans.get(line - range.start).map(Vec::as_slice).unwrap_or_default() {
                        let Some(c) = crate::syntax::color(pal, s.highlight) else { continue };
                        if syn_brushes[s.highlight].is_none() {
                            syn_brushes[s.highlight] = self.target.CreateSolidColorBrush(&color(c), None).ok();
                        }
                        if let Some(b) = &syn_brushes[s.highlight] {
                            let _ = l.SetDrawingEffect(b, DWRITE_TEXT_RANGE { startPosition: s.start, length: s.len });
                        }
                    }
                    self.brush.SetColor(&color(pal.text));
                    self.target.DrawTextLayout(
                        Vector2 { X: text_pad, Y: y },
                        l,
                        &self.brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                    );
                }

                if head >= start && head <= text_end {
                    let (x, y_off) =
                        if let Some(l) = text_layout.as_ref() { hit_test_xy(l, &text, head - start, text_pad) } else { (text_pad, 0.0) };
                    let cy = y + y_off;
                    // Normal/Visual dibujan un cursor de bloque (como una terminal vim de
                    // verdad) en vez de la misma barrita de 1px que Insert: así se nota de
                    // un vistazo en qué modo estás sin tener que leer "-- NORMAL --" en la
                    // barra de estado, que es lo que hacía que teclear "asd" en Normal
                    // pareciera un bug quieto (entras en Insert con "a" y no se notaba).
                    let block = state
                        .vim
                        .as_ref()
                        .is_some_and(|v| matches!(v.mode, crate::VimMode::Normal | crate::VimMode::Visual));
                    if block {
                        let (x1, y1_off) = if head < text_end {
                            text_layout
                                .as_ref()
                                .map(|l| hit_test_xy(l, &text, head + 1 - start, text_pad))
                                .unwrap_or((x + 8.0, y_off))
                        } else {
                            (x + 8.0, y_off)
                        };
                        // El carácter siguiente cae en otra fila visual (justo en el borde
                        // del wrap): se estira hasta el final de la fila en vez de dibujar
                        // un rectángulo que cruza dos filas.
                        let x1 = if y1_off != y_off { frame.body.right } else { x1 };
                        self.fill(Rect::new(x, cy, x1.max(x + 2.0), cy + self.line_height()), pal.accent_soft);
                        self.stroke_rect(Rect::new(x, cy, x1.max(x + 2.0), cy + self.line_height()), 1.0, pal.accent);
                    } else {
                        self.fill(Rect::new(x, cy, x + 1.0, cy + self.line_height()), pal.text);
                    }
                }

                y += row_h;
                if y > frame.body.bottom {
                    break;
                }
            }
            }

            let total_rows =
                if is_raw { state.raw.as_ref().map(|r| r.len().div_ceil(16).max(1)).unwrap_or(1) } else { total_lines };
            self.draw_editor_scrollbar(frame.body, &state.viewport, total_rows, pal, view);
            if split {
                self.pop_clip();
                if pi > 0 {
                    self.fill(Rect::new(pane.left - crate::splits::DIVIDER_W, pane.top, pane.left, pane.bottom), pal.line);
                }
                if focused {
                    self.fill(Rect::new(pane.left, pane.top, pane.right, pane.top + 2.0), pal.accent);
                    if self.panes.resize_mode {
                        self.stroke_round_rect(Rect::new(pane.left + 1.0, pane.top + 1.0, pane.right - 1.0, pane.bottom - 1.0), 3.0, 2.0, pal.accent);
                    }
                }
            }
            }
            if split && self.panes.resize_mode {
                self.draw_resize_hint(frame.body, pal);
            }

            if !frame.hints.is_empty() {
                let k = frame.hints.height() / layout::HINTS_H;
                let mut full = frame;
                full.hints = Rect::new(0.0, frame.hints.top, w, frame.hints.top + layout::HINTS_H);
                self.push_clip(frame.hints);
                self.set_fade(k);
                self.draw_hints(state, ws, pal, full, ui.md_preview_style);
                self.set_fade(1.0);
                self.pop_clip();
            }

            self.draw_status(ws, state, pal, frame, bands.merged_status, view, ui.suggestion_icons, lang);

            if let (Some((x, top)), Some(i)) = (self.pending_dropdown.take(), view.open_menu) {
                let checks = crate::menu::MenuChecks {
                    line_numbers: ui.line_numbers,
                    hints_bar: ui.hints_bar,
                    vim: state.vim.is_some(),
                    raw: is_raw,
                    md_preview: state.md_preview,
                };
                self.draw_dropdown(crate::menu::MENUS[i].items, x, top, checks, view, pal, lang);
            }

            if let Some(p) = self.path_copied {
                self.draw_toast(pal, frame.status.top, p, self.tr("Ruta copiada al portapapeles"), true);
            }
            if let Some((p, label)) = &self.notice {
                self.draw_toast(pal, frame.status.top, *p, label, false);
            }
            self.draw_update_panel(pal, frame.status.top, w, view);
            self.draw_about(pal, frame.body, view);
            self.draw_whats_new(pal, frame.body, view);

            if let Some(menu) = self.context_menu.take() {
                let rows: Vec<MenuRow> = menu
                    .items
                    .iter()
                    .map(|it| match it {
                        crate::context_menu::CtxItem::Entry { label, shortcut, enabled, .. } => {
                            MenuRow { label, shortcut, enabled: *enabled, sep: false, check: None }
                        }
                        crate::context_menu::CtxItem::Sep => {
                            MenuRow { label: "", shortcut: "", enabled: false, sep: true, check: None }
                        }
                    })
                    .collect();
                self.draw_menu_rows(&rows, menu.x, menu.y, menu.y, 180.0, view, pal, Hit::CtxItem);
                drop(rows);
                self.context_menu = Some(menu);
            }

            if !self.hold_frame.get() {
                let r = self.target.EndDraw(None, None);
                self.note_end_draw(r);
            }
        }
    }

    /// Icono, pestañas o título, y botones de min/max/cerrar (`.titlebar`).
    #[allow(unused_unsafe)]
    unsafe fn draw_titlebar(&mut self, ws: &Workspace, ui: &UiConfig, view: &ViewState, pal: &theme::Palette, frame: layout::Frame) {
        unsafe {
            self.fill(frame.titlebar, pal.chrome);

            // Icono de la app (el "n_" de assets/notty.ico) en los primeros 36px.
            let icon_x = (layout::APPICON_W - layout::APPICON_SIZE) / 2.0;
            let icon_y = (layout::TITLEBAR_H - layout::APPICON_SIZE) / 2.0;
            let icon = layout::Rect::new(icon_x, icon_y, icon_x + layout::APPICON_SIZE, icon_y + layout::APPICON_SIZE);
            self.draw_logo(icon, pal.accent, pal.on_accent, 1.0);

            let bands = Self::resolve_bands(ui, ws.len(), view.menu_bar_visible);
            let tabs_frac = Self::chrome_frac(view, bands).map(|f| f.tabs_in_title).unwrap_or(if bands.tabs_in_title { 1.0 } else { 0.0 });
            if tabs_frac > 0.0 && tabs_frac < 1.0 {
                // Ajustes → "Varios archivos" (Pestañas ↔ Buffers) o "Posición de las
                // pestañas" acaban de cambiar y esto vive en la propia barra de título:
                // ninguno de los dos estados (pestañas o título de archivo) mueve nada
                // (mismo hueco fijo), así que aquí es barato fundir cruzado de verdad —
                // ambos superpuestos con fundidos complementarios — en vez de solo
                // fundir la entrada como con el resto de bandas (Task 3 del plan de
                // animaciones, ver el módulo).
                let target = bands.tabs_in_title;
                let fade_of = |tabs: bool| if tabs { tabs_frac } else { 1.0 - tabs_frac };
                self.set_fade(fade_of(!target));
                self.draw_titlebar_tabs_or_title(ws, ui, frame, !target, view, pal);
                self.set_fade(fade_of(target));
                self.draw_titlebar_tabs_or_title(ws, ui, frame, target, view, pal);
                self.set_fade(1.0);
            } else {
                self.draw_titlebar_tabs_or_title(ws, ui, frame, bands.tabs_in_title, view, pal);
            }

            self.draw_settings_button(view, pal, frame.settings_btn);
            self.draw_caption_button(view, pal, frame.caption_min, Hit::Min, false);
            self.draw_caption_button(view, pal, frame.caption_max, Hit::Max, view.maximized);
            self.draw_caption_button(view, pal, frame.caption_close, Hit::Close, false);

            // Línea de separación bajo la barra de título, como en las apps nativas de
            // Windows 11 (Ajustes, Explorador...): la distingue del resto de la ventana
            // sin depender de que haya pestañas o menú justo debajo. Atenuada a la mitad
            // de su opacidad normal: en esas apps es casi imperceptible, no un trazo duro.
            self.stroke_line(0.0, frame.titlebar.bottom, frame.titlebar.width(), frame.titlebar.bottom, 1.0, pal.line.faded(0.5));
        }
    }

    /// El hueco central de la barra de título (`layout::APPICON_W`..`frame.caption_min.left`):
    /// la fila de pestañas si `tabs`, o el título del archivo activo si no. Extraído
    /// de `draw_titlebar` para poder dibujar los dos estados superpuestos durante su
    /// fundido cruzado (ver ahí, Task 3 del plan de animaciones).
    #[allow(unused_unsafe)]
    unsafe fn draw_titlebar_tabs_or_title(
        &mut self,
        ws: &Workspace,
        ui: &UiConfig,
        frame: layout::Frame,
        tabs: bool,
        view: &ViewState,
        pal: &theme::Palette,
    ) {
        unsafe {
            if tabs {
                // Hasta el engranaje de Ajustes (no hasta los botones de ventana: se
                // montaba encima), dejando siempre un hueco para arrastrar la ventana.
                let max_right = (frame.settings_btn.left - layout::TITLE_DRAG_MIN).max(layout::APPICON_W);
                self.draw_tabs_row(ws, view, pal, layout::APPICON_W, max_right, frame.titlebar.bottom, ui.tab_icons);
            } else {
                let name = crate::doc_name_lang(ws.active().path.as_deref(), self.lang());
                let title = crate::window_title(&name, ws.active().doc.is_dirty(), ui.preset == notty_config::Preset::Zen);
                self.text(
                    &title,
                    &self.fonts.ui_12,
                    Rect::new(layout::APPICON_W, 0.0, frame.settings_btn.left - 8.0, layout::TITLEBAR_H),
                    pal.text_2,
                );
            }
        }
    }

    /// Botón de Ajustes: un engranaje simple (anillo + 6 dientes cortos), para que
    /// Ajustes se pueda encontrar sin saber `Ctrl+,` de memoria.
    #[allow(unused_unsafe)]
    unsafe fn draw_settings_button(&mut self, view: &ViewState, pal: &theme::Palette, r: Rect) {
        unsafe {
            self.hits.push((r, Hit::Settings));
            let hovered = view.hover == Hit::Settings;
            if hovered {
                self.fill(r, pal.hover);
            }
            let cx = r.left + r.width() / 2.0;
            let cy = r.top + r.height() / 2.0;
            // Trazo más fino (1.0 en vez de 1.2) y radios algo más cerrados: junto a la
            // titlebar más baja, un icono igual de grande se veía desproporcionado.
            let c = if hovered { pal.text } else { pal.text_2 };
            self.stroke_circle(cx, cy, 2.8, 1.0, c);
            for i in 0..6 {
                let a = std::f32::consts::PI * 2.0 * (i as f32) / 6.0;
                let (dx, dy) = (a.cos(), a.sin());
                self.stroke_line(cx + dx * 4.2, cy + dy * 4.2, cx + dx * 6.0, cy + dy * 6.0, 1.0, c);
            }
        }
    }

    #[allow(unused_unsafe)]
    unsafe fn draw_caption_button(&mut self, view: &ViewState, pal: &theme::Palette, r: Rect, hit: Hit, maximized: bool) {
        unsafe {
            self.hits.push((r, hit));
            let hovered = view.hover == hit;
            let is_close = hit == Hit::Close;
            if hovered {
                self.fill(r, if is_close { pal.close_hover } else { pal.hover });
            }
            let glyph_color = if hovered && is_close { pal.close_hover_fg } else { pal.text_2 };
            let cx = r.left + (r.width() - 10.0) / 2.0;
            let cy = r.top + (r.height() - 10.0) / 2.0;
            match hit {
                Hit::Min => self.stroke_line(cx, cy + 5.0, cx + 10.0, cy + 5.0, 1.0, glyph_color),
                Hit::Max if maximized => {
                    self.stroke_rect(Rect::new(cx + 2.0, cy, cx + 10.0, cy + 8.0), 1.0, glyph_color);
                    self.stroke_rect(Rect::new(cx, cy + 2.0, cx + 8.0, cy + 10.0), 1.0, glyph_color);
                }
                Hit::Max => self.stroke_rect(Rect::new(cx + 0.5, cy + 0.5, cx + 9.0, cy + 9.0), 1.0, glyph_color),
                Hit::Close => {
                    self.stroke_line(cx, cy, cx + 10.0, cy + 10.0, 1.0, glyph_color);
                    self.stroke_line(cx + 10.0, cy, cx, cy + 10.0, 1.0, glyph_color);
                }
                _ => {}
            }
        }
    }

    /// Barra de menús (`.menubar`): "Archivo Editar Buscar Ver Ayuda".
    #[allow(unused_unsafe)]
    unsafe fn draw_menubar(&mut self, view: &ViewState, pal: &theme::Palette, frame: layout::Frame, lang: notty_config::Lang) {
        unsafe {
            self.fill(frame.menubar, pal.chrome);
            let mut x = layout::MENUBAR_PAD_X;
            let top = frame.menubar.top + (layout::MENUBAR_H - 22.0) / 2.0;
            let mut open_x = None;
            for (i, def) in crate::menu::MENUS.iter().enumerate() {
                let name = crate::strings::tr(lang, def.name);
                let w = self.measure(name, &self.fonts.ui_12_5) + layout::MENU_BTN_PAD_X * 2.0;
                if x + w > frame.menubar.right - layout::MENUBAR_PAD_X {
                    break;
                }
                let r = Rect::new(x, top, x + w, top + 22.0);
                let hovered = view.hover == Hit::Menu(i) || view.open_menu == Some(i);
                if hovered {
                    self.fill_round(r, 4.0, pal.hover);
                }
                // `w` ya incluye MENU_BTN_PAD_X a cada lado: si se dibuja con el rect
                // `r` entero (sin recortar ese margen), el texto queda pegado al borde
                // izquierdo y todo el relleno se amontona a la derecha, en vez de quedar
                // repartido a los dos lados como el resto de botones de la app.
                let label_r = Rect::new(r.left + layout::MENU_BTN_PAD_X, r.top, r.right - layout::MENU_BTN_PAD_X, r.bottom);
                self.text(name, &self.fonts.ui_12_5, label_r, pal.text);
                self.hits.push((r, Hit::Menu(i)));
                if view.open_menu == Some(i) {
                    open_x = Some(x);
                }
                x += w + layout::MENU_BTN_GAP;
            }
            self.pending_dropdown = open_x.map(|x| (x, frame.menubar.bottom - 2.0));
        }
    }

    /// Desplegable de un menú (`.dropdown`/`.menu-item`/`.menu-sep`): ancho mínimo
    /// 250, fondo `chrome_hi`, radio 8, sombra aproximada como en `draw_suggestions`.
    #[allow(unused_unsafe)]
    unsafe fn draw_dropdown(
        &mut self,
        items: &[crate::menu::MenuItem],
        x: f32,
        top: f32,
        checks: crate::menu::MenuChecks,
        view: &ViewState,
        pal: &theme::Palette,
        lang: notty_config::Lang,
    ) {
        use crate::menu::MenuItem;
        let keys = self.menu_keys.clone();
        let rows: Vec<MenuRow> = items
            .iter()
            .map(|it| match it {
                MenuItem::Entry { label, shortcut, cmd } => {
                    let label = crate::strings::tr(lang, label);
                    let shortcut = keys.iter().find(|(c, _)| c == cmd).map(|(_, s)| s.as_str()).unwrap_or(shortcut);
                    MenuRow { label, shortcut, enabled: true, sep: false, check: checks.state_of(*cmd) }
                }
                MenuItem::Sep => MenuRow { label: "", shortcut: "", enabled: false, sep: true, check: None },
            })
            .collect();
        // Sin sitio debajo, se sube lo justo en vez de abrirse por encima de la barra.
        unsafe { self.draw_menu_rows(&rows, x, top, f32::NEG_INFINITY, 250.0, view, pal, Hit::MenuItem) };
    }

    /// Caja de menú compartida por los desplegables de la barra y los menús
    /// contextuales: ancho mínimo `min_w`, fondo `chrome_hi`, radio 8, atajos alineados
    /// a la derecha, elementos desactivados atenuados y sin zona de clic. Se abre con
    /// la esquina en `(x, top)` y se mantiene dentro de la ventana: si no cabe debajo
    /// se abre hacia arriba terminando en `flip_y`, y si no cabe entera en alto, las
    /// filas se compactan.
    #[allow(unused_unsafe, clippy::too_many_arguments)]
    unsafe fn draw_menu_rows(
        &mut self,
        rows: &[MenuRow],
        x: f32,
        top: f32,
        flip_y: f32,
        min_w: f32,
        view: &ViewState,
        pal: &theme::Palette,
        hit_of: fn(usize) -> Hit,
    ) {
        unsafe {
            // Fundido + 4px de desplazamiento vertical al abrirse (ver anim.rs / Task 3
            // del plan de animaciones). `t == 1.0` (o sin animación en curso) dibuja
            // exactamente igual que antes, sin coste extra.
            let t = view.popup_open.unwrap_or(1.0);
            let dy = (1.0 - t) * 4.0;
            let draw_y = |y: f32| y - dy;

            let (win_w, win_h) = self.size_dips();
            let bounds = Rect::new(4.0, 4.0, (win_w - 4.0).max(8.0), (win_h - 4.0).max(8.0));
            let sep_h = 9.0;
            let entries = rows.iter().filter(|r| !r.sep).count().max(1) as f32;
            let seps = rows.iter().filter(|r| r.sep).count() as f32;
            let fixed_h = layout::POPUP_PAD * 2.0 + seps * sep_h;
            let row_h = ((bounds.height() - fixed_h) / entries).clamp(20.0, 28.0);
            // Con alguna opción de encender/apagar, todas las etiquetas dejan sitio a la
            // izquierda para la ✓, para que sigan alineadas entre sí.
            let check_w = if rows.iter().any(|r| r.check.is_some()) { 18.0 } else { 0.0 };
            let content_w = rows
                .iter()
                .filter(|r| !r.sep)
                .map(|r| {
                    check_w
                        + self.measure(r.label, &self.fonts.ui_12_5)
                        + if r.shortcut.is_empty() { 0.0 } else { self.measure(r.shortcut, &self.fonts.mono_11) + 24.0 }
                })
                .fold(0.0f32, f32::max);
            let width = (content_w + 24.0).max(min_w).min(bounds.width());
            let height = fixed_h + entries * row_h;
            // Posición final (sin desplazar): la usada para el hit-testing, que no anima.
            let box_r = layout::clamp_popup(x, top, width, height, flip_y, bounds);
            let draw_r = Rect::new(box_r.left, draw_y(box_r.top), box_r.right, draw_y(box_r.bottom));

            self.draw_popup_shadow(draw_r, layout::POPUP_RADIUS, t, pal.shadow);
            self.fill_round(draw_r, layout::POPUP_RADIUS, pal.chrome_hi.faded(t));
            self.stroke_round_rect(draw_r, layout::POPUP_RADIUS, 1.0, pal.shadow_ring.faded(t));
            self.hits.push((box_r, Hit::PopupBox));

            let mut y = box_r.top + layout::POPUP_PAD;
            for (j, row) in rows.iter().enumerate() {
                if row.sep {
                    let ly = draw_y(y + sep_h / 2.0);
                    self.stroke_line(draw_r.left + 4.0, ly, draw_r.right - 4.0, ly, 1.0, pal.line.faded(t));
                    y += sep_h;
                    continue;
                }
                // El rect de hit-testing se registra en su posición final (sin
                // desplazar): un clic a mitad de los 120ms de animación cae dentro
                // de un margen de error aceptable.
                let r = Rect::new(box_r.left + 4.0, y, box_r.right - 4.0, y + row_h);
                let dr = Rect::new(draw_r.left + 4.0, draw_y(y), draw_r.right - 4.0, draw_y(y + row_h));
                let highlighted = row.enabled && (view.hover == hit_of(j) || view.menu_sel == Some(j));
                if highlighted {
                    self.fill_round(dr, 4.0, pal.hover.faded(t));
                }
                let (label_c, key_c) = if row.enabled { (pal.text, pal.text_3) } else { (pal.text_3.faded(0.7), pal.text_3.faded(0.45)) };
                let inner = Rect::new(dr.left + 6.0, dr.top, dr.right - 6.0, dr.bottom);
                if row.check == Some(true) {
                    let cy = inner.top + inner.height() / 2.0;
                    let cx = inner.left + 2.0;
                    self.stroke_polyline(&[(cx, cy), (cx + 3.5, cy + 3.5), (cx + 10.0, cy - 3.5)], 1.4, pal.accent.faded(t));
                }
                let key_w = if row.shortcut.is_empty() { 0.0 } else { self.measure(row.shortcut, &self.fonts.mono_11) + 16.0 };
                let label_r = Rect::new(inner.left + check_w, inner.top, inner.right - key_w, inner.bottom);
                self.text(row.label, &self.fonts.ui_12_5, label_r, label_c.faded(t));
                if !row.shortcut.is_empty() {
                    self.text_right(row.shortcut, &self.fonts.mono_11, inner, key_c.faded(t));
                }
                if row.enabled {
                    self.hits.push((r, hit_of(j)));
                }
                y += row_h;
            }
        }
    }

    /// Cartel del modo acomodar, arriba en el centro del cuerpo.
    fn draw_resize_hint(&self, body: Rect, pal: &theme::Palette) {
        let label = self.tr("Acomodar paneles");
        let keys = self.tr("←/→ ancho · Shift más rápido · Tab panel · = igualar · Esc salir");
        let lw = self.measure(label, &self.fonts.ui_12_5_semibold);
        let kw = self.measure(keys, &self.fonts.ui_12);
        let w = (lw + 14.0 + kw + 28.0).min(body.width() - 16.0);
        let x = body.left + (body.width() - w) / 2.0;
        let r = Rect::new(x, body.top + 12.0, x + w, body.top + 44.0);
        self.draw_popup_shadow(r, 8.0, 1.0, pal.shadow);
        self.fill_round(r, 8.0, pal.chrome_hi);
        self.stroke_round_rect(r, 8.0, 1.0, pal.accent.faded(0.6));
        self.text(label, &self.fonts.ui_12_5_semibold, Rect::new(r.left + 14.0, r.top, r.right, r.bottom), pal.accent);
        self.text(keys, &self.fonts.ui_12, Rect::new(r.left + 14.0 + lw + 14.0, r.top, r.right - 10.0, r.bottom), pal.text_2);
    }

    /// Fila de pestañas compartida entre la barra de título y `.tabs.below`.
    #[allow(unused_unsafe)]
    unsafe fn draw_tabs_row(&mut self, ws: &Workspace, view: &ViewState, pal: &theme::Palette, x0: f32, max_right: f32, bottom: f32, tab_icons: bool) {
        unsafe {
            // Fila = documentos reales + pestañas recién cerradas que aún encogen
            // (`ghosts`), cada una en la posición que ocupaba.
            enum Slot {
                Doc(usize),
                Ghost(usize),
            }
            let anim = self.tab_anim.clone();
            let doc_count = ws.len();
            // Modo Paneles: una pestaña por grupo, representada por su documento con
            // foco (sin fantasmas de cierre: sus índices son de documentos, no de grupos).
            let groups = self.panes.tabs.clone();
            let mut order: Vec<Slot> = Vec::with_capacity(doc_count + anim.ghosts.len());
            if let Some(groups) = &groups {
                order.extend(groups.iter().map(|g| Slot::Doc(g.0)));
            } else {
                let mut gi = 0;
                for d in 0..doc_count {
                    while gi < anim.ghosts.len() && anim.ghosts[gi].at <= d {
                        order.push(Slot::Ghost(gi));
                        gi += 1;
                    }
                    order.push(Slot::Doc(d));
                }
                while gi < anim.ghosts.len() {
                    order.push(Slot::Ghost(gi));
                    gi += 1;
                }
            }
            let group_of = |d: usize| groups.as_ref().and_then(|g| g.iter().find(|g| g.0 == d)).map(|g| g.1.as_slice());

            let docs: Vec<&EditorState> = ws.iter().collect();
            let names: Vec<String> = order
                .iter()
                .map(|s| match s {
                    Slot::Doc(d) => {
                        let name = crate::doc_name_lang(docs[*d].path.as_deref(), self.lang());
                        match group_of(*d).map(<[usize]>::len) {
                            Some(n) if n > 1 => format!("{name} · {n}"),
                            _ => name,
                        }
                    }
                    Slot::Ghost(g) => anim.ghosts[*g].name.clone(),
                })
                .collect();
            let slots: Vec<layout::TabSlot> = order
                .iter()
                .zip(&names)
                .map(|(s, n)| {
                    let (dirty, scale) = match s {
                        Slot::Doc(d) => {
                            let dirty = match group_of(*d) {
                                Some(g) => g.iter().any(|&i| docs.get(i).is_some_and(|s| s.doc.is_dirty())),
                                None => docs[*d].doc.is_dirty(),
                            };
                            (dirty, anim.open_scale(*d))
                        }
                        Slot::Ghost(g) => (anim.ghosts[*g].dirty, anim.ghosts[*g].scale),
                    };
                    layout::TabSlot { name_w: self.measure(n, &self.fonts.ui_12), dirty, scale }
                })
                .collect();
            let is_active = |d: usize| match group_of(d) {
                Some(g) => g.contains(&ws.active_index()),
                None => d == ws.active_index(),
            };
            let focus = order.iter().position(|s| matches!(s, Slot::Doc(d) if is_active(*d))).unwrap_or(0);
            let dot_w = self.measure("●", &self.fonts.ui_9);
            let icon_w = if tab_icons { layout::TAB_ICON } else { 0.0 };
            let row = layout::tabs_fit(x0, bottom, max_right, &slots, focus, dot_w, icon_w);
            let base_fade = self.fade();
            let first_vis = row.tabs.iter().position(Option::is_some).unwrap_or(0);
            let last_vis = row.tabs.iter().rposition(Option::is_some).unwrap_or(0);
            let doc_at = |si: &usize| match order[*si] {
                Slot::Doc(d) => Some(d),
                Slot::Ghost(_) => None,
            };
            self.tab_scroll_targets = (
                (0..first_vis).rev().find_map(|si| doc_at(&si)),
                (last_vis + 1..order.len()).find_map(|si| doc_at(&si)),
            );

            for (si, geom) in row.tabs.iter().enumerate() {
                let Some(t) = geom else { continue };
                let scale = slots[si].scale.clamp(0.0, 1.0);
                if t.rect.width() < 0.5 {
                    continue;
                }
                let animating = scale < 1.0;
                if animating {
                    self.set_fade(base_fade * scale);
                    self.push_clip(t.rect);
                }
                let i = match order[si] {
                    Slot::Doc(d) => d,
                    Slot::Ghost(_) => {
                        // Pestaña cerrada encogiendo: solo nombre (y ●), sin fondo ni clics.
                        self.text(&names[si], &self.fonts.ui_12, Rect::new(t.name_x, t.rect.top, t.name_x + t.name_w, t.rect.bottom), pal.text_2);
                        if let Some(dot_x) = t.dot_x {
                            self.text("●", &self.fonts.ui_9, Rect::new(dot_x, t.rect.top, dot_x + dot_w, t.rect.bottom), pal.text_2);
                        }
                        self.pop_clip();
                        self.set_fade(base_fade);
                        continue;
                    }
                };
                let active = is_active(i);
                let hovered_tab = view.hover == Hit::Tab(i) || view.hover == Hit::TabClose(i);
                if active {
                    // La pestaña activa funde su fondo desde transparente en vez de
                    // aparecer de golpe al cambiar (Task 4 del plan de animaciones);
                    // `t == None`/`1.0` pinta exactamente igual que antes.
                    let bg = pal.surface.faded(view.tab_switch.unwrap_or(1.0));
                    self.fill_round(t.rect, layout::TAB_RADIUS, bg);
                    self.fill(Rect::new(t.rect.left, t.rect.bottom - layout::TAB_RADIUS, t.rect.right, t.rect.bottom), bg);
                } else if hovered_tab {
                    self.fill_round(t.rect, layout::TAB_RADIUS, pal.hover);
                    self.fill(Rect::new(t.rect.left, t.rect.bottom - layout::TAB_RADIUS, t.rect.right, t.rect.bottom), pal.hover);
                }
                let name_color = if active { pal.text } else { pal.text_2 };
                if let Some(icon_cx) = t.icon_cx {
                    let icon_cy = (t.rect.top + t.rect.bottom) / 2.0;
                    let icon_r = Rect::new(icon_cx - layout::TAB_ICON / 2.0, icon_cy - layout::TAB_ICON / 2.0, icon_cx + layout::TAB_ICON / 2.0, icon_cy + layout::TAB_ICON / 2.0);
                    self.draw_suggestion_icon(false, icon_r, name_color.faded(0.85));
                }
                self.text(&names[si], &self.fonts.ui_12, Rect::new(t.name_x, t.rect.top, t.name_x + t.name_w, t.rect.bottom), name_color);
                if let Some(dot_x) = t.dot_x {
                    self.text(
                        "●",
                        &self.fonts.ui_9,
                        Rect::new(dot_x, t.rect.top, dot_x + dot_w, t.rect.bottom),
                        pal.text_2,
                    );
                }
                self.hits.push((t.rect, Hit::Tab(i)));
                // Mientras crece, la ✕ aún puede caer fuera de la pestaña: no se registra
                // para que no robe el clic a la vecina.
                if (active || hovered_tab) && t.close.right <= t.rect.right + 0.5 {
                    let close_hovered = view.hover == Hit::TabClose(i);
                    if close_hovered {
                        self.fill_round(t.close, 4.0, pal.hover);
                    }
                    let cc = if close_hovered { pal.text } else { pal.text_3 };
                    // El glifo de la ✕ es de 10x10 (xIcon(10) de la maqueta), centrado en
                    // el botón de 18x18: no ocupa el botón entero.
                    let cx = t.close.left + t.close.width() / 2.0;
                    let cy = t.close.top + t.close.height() / 2.0;
                    self.stroke_line(cx - 5.0, cy - 5.0, cx + 5.0, cy + 5.0, 1.1, cc);
                    self.stroke_line(cx + 5.0, cy - 5.0, cx - 5.0, cy + 5.0, 1.1, cc);
                    self.hits.push((t.close, Hit::TabClose(i)));
                }
                if animating {
                    self.pop_clip();
                    self.set_fade(base_fade);
                }
            }

            for (r, hit, left) in [(row.scroll_left, Hit::TabScrollLeft, true), (row.scroll_right, Hit::TabScrollRight, false)] {
                let Some(r) = r else { continue };
                let hovered = view.hover == hit;
                let btn = Rect::new(r.left + 1.0, r.top + 4.0, r.right - 1.0, r.bottom - 2.0);
                if hovered {
                    self.fill_round(btn, 4.0, pal.hover);
                }
                let cx = btn.left + btn.width() / 2.0;
                let cy = btn.top + btn.height() / 2.0;
                let d = if left { 2.0 } else { -2.0 };
                let c = if hovered { pal.text } else { pal.text_3 };
                self.stroke_polyline(&[(cx + d, cy - 4.0), (cx - d, cy), (cx + d, cy + 4.0)], 1.2, c);
                self.hits.push((r, hit));
            }

            let plus = row.plus;
            let plus_hovered = view.hover == Hit::NewTab;
            if plus_hovered {
                self.fill_round(plus, 5.0, pal.hover);
            }
            let pc = pal.text_3;
            let pcx = plus.left + plus.width() / 2.0;
            let pcy = plus.top + plus.height() / 2.0;
            self.stroke_line(pcx - 5.0, pcy, pcx + 5.0, pcy, 1.1, pc);
            self.stroke_line(pcx, pcy - 5.0, pcx, pcy + 5.0, 1.1, pc);
            self.hits.push((plus, Hit::NewTab));
        }
    }

    /// Barra de atajos (`.hints`): fondo `surface_2`, línea superior, pares tecla/acción.
    #[allow(unused_unsafe)]
    unsafe fn draw_hints(&self, state: &EditorState, ws: &Workspace, pal: &theme::Palette, frame: layout::Frame, md_preview_style: notty_config::MdPreviewStyle) {
        unsafe {
            self.fill(frame.hints, pal.surface_2);
            self.stroke_line(0.0, frame.hints.top, frame.hints.width(), frame.hints.top, 1.0, pal.line);

            let ctx = if !matches!(ws.prompt, crate::Prompt::None) {
                match &ws.prompt {
                    crate::Prompt::Path(p) if p.purpose == crate::Purpose::Save => crate::HintsCtx::PathSave,
                    crate::Prompt::Path(_) => crate::HintsCtx::PathOpen,
                    crate::Prompt::Find(_) => crate::HintsCtx::Find,
                    crate::Prompt::Replace(_) => crate::HintsCtx::Replace,
                    _ => crate::HintsCtx::Normal,
                }
            } else if state.raw.is_some() {
                crate::HintsCtx::Raw
            } else if state.md_preview {
                if md_preview_style == notty_config::MdPreviewStyle::ReadOnly { crate::HintsCtx::MdPreviewReadOnly } else { crate::HintsCtx::MdPreviewInline }
            } else if let Some(vim) = &state.vim {
                if vim.mode == crate::VimMode::Insert { crate::HintsCtx::VimInsert } else { crate::HintsCtx::VimNormal }
            } else {
                crate::HintsCtx::Normal
            };

            let mut x = layout::HINTS_PAD_X;
            let items = crate::hints_items(ctx, self.lang());
            let space_w = self.measure(" ", &self.fonts.mono_11);
            for (key, action) in items {
                let key_w = self.measure(key, &self.fonts.mono_11_bold);
                let action_w = self.measure(action, &self.fonts.mono_11);
                // Van de más a menos importante: en una ventana estrecha se dejan de
                // pintar los que no caben enteros en vez de cortarlos a medias.
                if x + key_w + space_w + action_w > frame.hints.right - layout::HINTS_PAD_X {
                    break;
                }
                self.text(key, &self.fonts.mono_11_bold, Rect::new(x, frame.hints.top, x + key_w, frame.hints.bottom), pal.text_hint);
                x += key_w;
                x += space_w;
                self.text(action, &self.fonts.mono_11, Rect::new(x, frame.hints.top, x + action_w, frame.hints.bottom), pal.text_2);
                x += action_w + layout::HINTS_GAP;
            }
        }
    }

    /// Barra de estado (`.status`) o, si hay un prompt activo, lo dibuja en su lugar
    /// (nunca coexisten: mientras hay prompt, la franja inferior es suya por completo).
    #[allow(unused_unsafe)]
    unsafe fn draw_status(&mut self, ws: &Workspace, state: &EditorState, pal: &theme::Palette, frame: layout::Frame, merged: bool, view: &ViewState, suggestion_icons: bool, lang: notty_config::Lang) {
        unsafe {
            let r = frame.status;
            if merged {
                self.fill(r, pal.cmd);
                self.stroke_line(0.0, r.top, r.width(), r.top, 1.0, pal.line);
            } else {
                self.fill(r, pal.chrome);
            }

            // El aviso de actualización va a la derecha y el resto se reparte el hueco
            // que deja (antes se pintaba encima de los campos). Con un prompt abierto
            // en una ventana estrecha, el prompt tiene prioridad y el aviso se oculta.
            let prompt_open = !matches!(ws.prompt, crate::Prompt::None);
            let notice_w = self.update_notice.clone().map(|n| self.measure(&n, &self.fonts.mono_12) + 16.0);
            let notice_w = notice_w.filter(|nw| !prompt_open || r.width() - nw >= 560.0);
            let content_r = match notice_w {
                Some(nw) => Rect::new(r.left, r.top, (r.right - layout::STATUS_PAD_X - nw).max(r.left), r.bottom),
                None => r,
            };

            if prompt_open {
                self.draw_prompt(&ws.prompt, state, pal, content_r, merged, view, suggestion_icons, lang);
            } else {
                self.draw_status_normal(state, pal, content_r, merged, view);
            }

            if notice_w.is_some() {
                self.draw_update_notice(pal, r, view);
            }
        }
    }

    /// Aviso "Actualización X.Y.Z disponible" (Task 4 del plan del actualizador):
    /// se dibuja en la esquina derecha de la barra de estado, si hay una comprobada.
    /// No se muestra nada si `update_notice` está a `None` (comprobación desactivada,
    /// sin actualización más nueva, o sin comprobar todavía).
    unsafe fn draw_update_notice(&mut self, pal: &theme::Palette, r: Rect, view: &ViewState) {
        let Some(notice) = self.update_notice.clone() else { return };
        let font = self.fonts.mono_12.clone();
        let w = self.measure(&notice, &font) + 16.0;
        let notice_r = Rect::new((r.right - layout::STATUS_PAD_X - w).max(r.left), r.top, r.right - layout::STATUS_PAD_X, r.bottom);
        if view.hover == Hit::UpdateNotice {
            self.fill_round(notice_r, 4.0, pal.accent_soft);
        }
        self.text(&notice, &font, Rect::new(notice_r.left + 8.0, r.top, notice_r.right - 8.0, r.bottom), pal.accent);
        self.hits.push((notice_r, Hit::UpdateNotice));
    }

    /// Fija (o borra, con `None`) el texto del aviso de actualización disponible que
    /// se dibuja en la barra de estado. Lo llama `notty-ui`/`notty` cuando cambia el
    /// resultado de la comprobación (Task 4 del plan del actualizador).
    pub fn set_update_notice(&mut self, notice: Option<String>) {
        self.update_notice = notice;
    }

    /// Fija (o borra, con `None`) el contenido del panel de notas de versión.
    pub fn set_update_panel(&mut self, content: Option<UpdatePanelContent>) {
        self.update_panel = content;
    }

    /// Fija (o borra, con `None`) el menú contextual a dibujar en el próximo `paint`.
    pub fn set_context_menu(&mut self, menu: Option<crate::context_menu::ContextMenu>) {
        self.context_menu = menu;
    }

    /// Estado de las animaciones de pestañas para el próximo `paint`.
    pub fn set_tab_anim(&mut self, frame: crate::tab_anim::TabAnimFrame) {
        self.tab_anim = frame;
    }

    pub fn set_panes(&mut self, panes: PaneView) {
        self.panes = panes;
    }

    pub fn set_about(&mut self, about: Option<AboutContent>) {
        self.about = about;
    }

    pub fn set_path_copied(&mut self, progress: Option<f32>) {
        self.path_copied = progress;
    }

    pub fn set_notice(&mut self, notice: Option<(f32, String)>) {
        self.notice = notice;
    }

    /// Aviso "Ruta copiada al portapapeles" (maqueta `Copiado.dc.html`): sube y
    /// aparece, se queda, y se va hacia arriba; el ✓ se dibuja trazo a trazo. Con
    /// `ok == false` (avisos de error) lleva un "!" en vez del ✓.
    fn draw_toast(&self, pal: &theme::Palette, status_top: f32, p: f32, label: &str, ok: bool) {
        let ease = |k: f32| crate::Curve::Out.apply(k.clamp(0.0, 1.0));
        let (alpha, dy, scale) = if p < 0.12 {
            let k = ease(p / 0.12);
            (k, 8.0 * (1.0 - k), 0.96 + 0.04 * k)
        } else if p < 0.82 {
            (1.0, 0.0, 1.0)
        } else {
            let k = ease((p - 0.82) / 0.18);
            (1.0 - k, -4.0 * k, 1.0)
        };
        let tw = self.measure(label, &self.fonts.ui_12);
        let w = 12.0 + 14.0 + 8.0 + tw + 12.0;
        let left = layout::STATUS_PAD_X + 4.0;
        let r = Rect::new(left, status_top - 10.0 - 30.0 + dy, left + w, status_top - 10.0 + dy);
        let c = windows_numerics::Vector2 { X: r.left + w / 2.0, Y: r.top + 15.0 };
        self.set_transform(Matrix3x2::scale_around(scale, scale, c));
        let base = self.fade();
        self.set_fade(base * alpha);
        self.draw_popup_shadow(r, 6.0, 1.0, pal.shadow);
        self.fill_round(r, 6.0, pal.chrome);
        self.stroke_round_rect(r, 6.0, 1.0, pal.shadow_ring);
        // ✓ que se traza de 0,08 s a 0,43 s (`stroke-dashoffset`).
        let draw = ease((p * 1.8 - 0.08) / 0.35);
        let (ix, iy, k) = (r.left + 12.0, r.top + 8.0, 14.0 / 24.0);
        let pts = [(5.0f32, 12.0f32), (10.0, 17.0), (19.0, 7.0)];
        let seg1 = 50f32.sqrt();
        let seg2 = 181f32.sqrt();
        let len = (seg1 + seg2) * draw;
        let mut line = vec![(ix + pts[0].0 * k, iy + pts[0].1 * k)];
        if len <= seg1 {
            let f = len / seg1;
            line.push((ix + (5.0 + 5.0 * f) * k, iy + (12.0 + 5.0 * f) * k));
        } else {
            line.push((ix + pts[1].0 * k, iy + pts[1].1 * k));
            let f = (len - seg1) / seg2;
            line.push((ix + (10.0 + 9.0 * f) * k, iy + (17.0 - 10.0 * f) * k));
        }
        if !ok {
            self.text("!", &self.fonts.ui_12, Rect::new(ix + 4.0, r.top, ix + 14.0, r.bottom), pal.warn);
        } else if draw > 0.0 {
            self.stroke_polyline(&line, 2.0, pal.ok);
        }
        self.text(label, &self.fonts.ui_12, Rect::new(r.left + 34.0, r.top, r.right, r.bottom), pal.text);
        self.set_fade(base);
        self.reset_transform();
    }

    /// Atajos en vigor de los elementos de menú (ver `menu::shortcut_labels`).
    pub fn set_syntax_disabled(&mut self, ids: &[String]) {
        if self.syntax_disabled != ids {
            self.syntax_disabled = ids.to_vec();
        }
    }

    pub fn set_menu_keys(&mut self, keys: Vec<(crate::menu::MenuCmd, String)>) {
        self.menu_keys = keys;
    }

    /// Documento que activa el botón ‹ (`left`) o › de la fila de pestañas: el más
    /// cercano que quedó oculto por ese lado en el último `paint`.
    pub fn tab_scroll_target(&self, left: bool) -> Option<usize> {
        if left { self.tab_scroll_targets.0 } else { self.tab_scroll_targets.1 }
    }

    /// Punto (DIPs) justo debajo del cursor de texto, para abrir ahí el menú
    /// contextual con el teclado (`Shift+F10`, tecla Menú). Si el cursor no está a la
    /// vista, una esquina del cuerpo.
    pub fn caret_point(&self, state: &EditorState, body: Rect, gutter_w: f32) -> (f32, f32) {
        let buf = state.doc.buffer();
        let total = buf.len_lines();
        let head = state.doc.selection().head;
        let (line, _) = buf.line_col(head);
        let text_pad = body.left + gutter_w + layout::TEXT_PAD_L;
        let fallback = (text_pad + 16.0, body.top + layout::TEXT_PAD_T + self.line_height());
        if !state.viewport.range(total).contains(&line) {
            return fallback;
        }
        let y = body.top + layout::TEXT_PAD_T + (line - state.viewport.first_line) as f32 * self.line_height() + self.line_height();
        if y > body.bottom {
            return fallback;
        }
        let start = buf.line_start(line);
        let end = if line + 1 < total { buf.line_start(line + 1) } else { buf.len_chars() };
        let text: String = buf.slice(start..end).trim_end_matches(['\r', '\n']).to_string();
        let w16 = wide(&text);
        let x = if w16.is_empty() {
            text_pad
        } else {
            match unsafe { self.dwrite.CreateTextLayout(&w16, &self.fonts.mono_13, f32::MAX, self.line_height()) } {
                Ok(l) => unsafe { hit_test_x(&l, &text, head.saturating_sub(start), text_pad) },
                Err(_) => text_pad,
            }
        };
        (x, y)
    }

    /// Aviso de versión nueva (maqueta `Popup.dc.html`): columna del icono y columna
    /// del texto (título, `actual → nueva`, novedades), separador y botones del mismo
    /// alto alineados a la derecha. Anclado encima de la barra de estado.
    unsafe fn draw_update_panel(&mut self, pal: &theme::Palette, status_top: f32, w: f32, view: &ViewState) {
        let Some(content) = self.update_panel.clone() else { return };
        let panel_w = 440.0f32.min(w - layout::STATUS_PAD_X * 2.0).max(220.0);
        let pad = 20.0;
        let text_x_off = pad + 40.0 + 14.0;
        let text_w = panel_w - text_x_off - pad;
        let body_f = self.create_format(None, 12.0, false).unwrap_or_else(|| self.fonts.ui_12.clone());
        let title_f = self.create_format(None, 15.0, true).unwrap_or_else(|| self.fonts.ui_13_semibold.clone());
        let bullets = crate::settings_model::release_bullets(&content.body, 5, self.lang());
        let mut bullets_h = 0.0;
        for b in &bullets {
            bullets_h += self.measure_wrapped(b, &body_f, text_w - 16.0, Some(20.0)).max(20.0);
        }
        let status_h = if content.status_line.is_some() { 20.0 } else { 0.0 };
        let head_h = 40.0f32;
        let content_h = head_h + if bullets.is_empty() { 0.0 } else { 14.0 + bullets_h } + status_h;
        let panel_h = (pad + content_h + 14.0 + 1.0 + 14.0 + 32.0 + 16.0).min(status_top - layout::TITLEBAR_H - 16.0);
        let right = w - layout::STATUS_PAD_X;
        let r = Rect::new(right - panel_w, status_top - panel_h - 8.0, right, status_top - 8.0);

        self.draw_popup_shadow(r, 10.0, 1.0, pal.shadow);
        self.fill_round(r, 10.0, pal.surface_2);
        self.stroke_round_rect(r, 10.0, 1.0, pal.shadow_ring);
        self.hits.push((r, Hit::PopupBox));
        self.push_clip(r);

        // Icono.
        let (icx, icy) = (r.left + pad + 20.0, r.top + pad + 20.0);
        self.fill_circle(icx, icy, 20.0, pal.accent_soft);
        self.stroke_svg("M12 4v11M7 10l5 5 5-5M5 20h14", icx, icy, 20.0, 2.0, pal.accent, None);
        // Título y versiones, centrados en el alto del icono.
        let tx = r.left + text_x_off;
        let ty = r.top + pad;
        self.text(self.tr("Hay una versión nueva de notty"), &title_f, Rect::new(tx, ty, tx + text_w, ty + 20.0), pal.text);
        let versions = format!("{} → {}", crate::app_version(), content.version);
        self.text(&versions, &self.fonts.mono_12, Rect::new(tx, ty + 24.0, tx + text_w, ty + 40.0), pal.text_2);
        let mut y = ty + head_h + 14.0;
        let soft = pal.text.mix(pal.text_2, 0.3);
        for b in &bullets {
            let h = self.measure_wrapped(b, &body_f, text_w - 16.0, Some(20.0)).max(20.0);
            self.text("•", &body_f, Rect::new(tx + 2.0, y, tx + 14.0, y + 20.0), soft);
            self.text_wrapped(b, &body_f, Rect::new(tx + 16.0, y, tx + text_w, y + h), soft, Some(20.0));
            y += h;
        }
        if let Some(status) = &content.status_line {
            self.text(status, &body_f, Rect::new(tx, y, tx + text_w, y + 20.0), pal.warn);
        }

        // Separador de lado a lado y botones.
        let sep_y = r.bottom - 16.0 - 32.0 - 14.0;
        self.fill(Rect::new(r.left, sep_y, r.right, sep_y + 1.0), pal.line);
        let btn_top = r.bottom - 16.0 - 32.0;
        let semi = self.create_format(None, 13.0, true).unwrap_or_else(|| self.fonts.ui_13_semibold.clone());
        let normal = self.fonts.ui_13.clone();
        let upd_w = (self.measure(self.tr("Actualizar"), &semi) + 28.0).max(96.0);
        let later_w = (self.measure(self.tr("Más tarde"), &normal) + 28.0).max(96.0);
        let upd = Rect::new(r.right - pad - upd_w, btn_top, r.right - pad, btn_top + 32.0);
        let later_right = if content.show_actualizar { upd.left - 8.0 } else { r.right - pad };
        let later = Rect::new(later_right - later_w, btn_top, later_right, btn_top + 32.0);
        if content.show_actualizar {
            let bg = if view.hover == Hit::UpdatePanelActualizar { pal.accent.mix(pal.text, 0.15) } else { pal.accent };
            self.fill_round(upd, 6.0, bg);
            self.text_center(self.tr("Actualizar"), &semi, upd, pal.on_accent);
            self.hits.push((upd, Hit::UpdatePanelActualizar));
        }
        let bg = if view.hover == Hit::UpdatePanelCerrar { pal.line } else { pal.chrome };
        self.fill_round(later, 6.0, bg);
        self.text_center(self.tr("Más tarde"), &normal, later, pal.text);
        self.hits.push((later, Hit::UpdatePanelCerrar));
        self.pop_clip();
    }

    /// "Acerca de notty": tarjeta centrada sobre el cuerpo, con el mismo aspecto que
    /// los desplegables. Se cierra con clic fuera, Esc o Enter.
    unsafe fn draw_about(&mut self, pal: &theme::Palette, body: Rect, view: &ViewState) {
        let Some(about) = self.about.clone() else { return };
        let (win_w, _) = self.size_dips();
        let w = 320.0f32.min(win_w - 32.0);
        let h = if about.url.is_some() { 150.0 } else { 124.0 };
        let cx = win_w / 2.0;
        let top = (body.top + (body.height() - h) / 2.0).max(body.top + 8.0);
        let r = Rect::new(cx - w / 2.0, top, cx + w / 2.0, top + h);
        self.draw_popup_shadow(r, layout::POPUP_RADIUS, 1.0, pal.shadow);
        self.fill_round(r, layout::POPUP_RADIUS, pal.chrome_hi);
        self.stroke_round_rect(r, layout::POPUP_RADIUS, 1.0, pal.shadow_ring);
        self.hits.push((r, Hit::PopupBox));

        let pad = 18.0;
        let mut y = r.top + pad;
        self.text("notty", &self.fonts.ui_18_semibold, Rect::new(r.left + pad, y, r.right - pad, y + 26.0), pal.text);
        y += 28.0;
        let version = format!("{} {}", self.tr("Versión"), about.version);
        self.text(&version, &self.fonts.ui_12, Rect::new(r.left + pad, y, r.right - pad, y + 18.0), pal.text_2);
        y += 20.0;
        self.text(
            self.tr("Editor de texto nativo para Windows"),
            &self.fonts.ui_11_5,
            Rect::new(r.left + pad, y, r.right - pad, y + 18.0),
            pal.text_3,
        );
        y += 26.0;
        if let Some(url) = &about.url {
            let label = url.trim_start_matches("https://");
            let lw = self.measure(label, &self.fonts.ui_11_5).min(r.width() - pad * 2.0);
            let link_r = Rect::new(r.left + pad, y, r.left + pad + lw, y + 18.0);
            let hovered = view.hover == Hit::AboutLink;
            self.text(label, &self.fonts.ui_11_5, link_r, pal.accent);
            if hovered {
                self.stroke_line(link_r.left, link_r.bottom - 2.0, link_r.right, link_r.bottom - 2.0, 1.0, pal.accent);
            }
            self.hits.push((link_r, Hit::AboutLink));
        }
    }

    #[allow(unused_unsafe)]
    unsafe fn draw_status_normal(&mut self, state: &EditorState, pal: &theme::Palette, r: Rect, merged: bool, view: &ViewState) {
        unsafe {
            let label_font = if merged { self.fonts.mono_12.clone() } else { self.fonts.ui_11.clone() };
            let label_font = &label_font;
            let mut x = r.left + layout::STATUS_PAD_X;

            if let Some(vim) = &state.vim {
                let (label, c) = match vim.mode {
                    crate::VimMode::Normal => ("-- NORMAL --", pal.accent),
                    crate::VimMode::Insert => ("-- INSERT --", pal.ok),
                    crate::VimMode::Visual => ("-- VISUAL --", pal.accent),
                };
                let w = self.measure(label, &self.fonts.mono_11_5_semibold);
                self.text(label, &self.fonts.mono_11_5_semibold, Rect::new(x, r.top, x + w, r.bottom), c);
                x += w + layout::STATUS_L_GAP;
            }

            if let Some(raw) = &state.raw {
                let size_label = crate::raw_size(raw.len());
                let w = self.measure(&size_label, label_font);
                self.text(&size_label, label_font, Rect::new(x, r.top, x + w, r.bottom), pal.text_2);
                x += w + layout::STATUS_L_GAP;

                if !raw.writable_fs() {
                    self.draw_lock(pal, x, r);
                    x += 12.0 + 6.0;
                }
                let word = self.tr(if raw.writable_fs() && raw.is_editing() { "escritura" } else { "solo lectura" });
                let ww = self.measure(word, label_font);
                self.text(word, label_font, Rect::new(x, r.top, x + ww, r.bottom), pal.text_3);
                x += ww + layout::STATUS_L_GAP;

                self.draw_pencil(raw, pal, x, r);
                x += 24.0;
            } else if state.path.is_none() {
                // Documento sin ruta todavía: un enlace discreto que explica qué pasa
                // al pulsarlo (antes ponía "CLICKME", que no decía nada).
                let label = self.tr("Guardar como…");
                let w = self.measure(label, label_font) + 8.0;
                let click_r = Rect::new(x, r.top + 3.0, x + w, r.bottom - 3.0);
                let hovered = view.hover == Hit::Clickme;
                if hovered {
                    self.fill_round(click_r, 4.0, pal.accent_soft);
                }
                let c = if hovered { pal.accent } else { pal.text_2 };
                self.text(label, label_font, Rect::new(x + 4.0, r.top, x + w - 4.0, r.bottom), c);
                let text_w = w - 8.0;
                let uy = r.top + r.height() / 2.0 + 7.0;
                self.stroke_line(x + 4.0, uy, x + 4.0 + text_w, uy, 1.0, c.faded(if hovered { 0.8 } else { 0.45 }));
                self.hits.push((click_r, Hit::Clickme));
                x += w;
            } else if !merged {
                // El nombre del archivo, no el "Texto" genérico de la maqueta: en la
                // barra de estado real de notty tiene más sentido decir qué archivo es.
                let name = crate::doc_name_lang(state.path.as_deref(), self.lang());
                let w = self.measure(&name, label_font).min(r.width() * 0.4);
                // Recién copiada la ruta, el nombre se ilumina y se apaga despacio.
                let hit = self.path_copied.map(|p| if p < 0.5 { 1.0 } else { (1.0 - (p - 0.5) / 0.35).clamp(0.0, 1.0) }).unwrap_or(0.0);
                if hit > 0.0 {
                    self.fill_round(Rect::new(x - 4.0, r.top + 3.0, x + w + 4.0, r.bottom - 3.0), 3.0, pal.accent.faded(0.16 * hit));
                }
                self.text(&name, label_font, Rect::new(x, r.top, x + w, r.bottom), pal.text_2.mix(pal.accent, hit));
                x += w;
            }
            let left_end = x;

            // Derecha: los tres campos de `status_right`, o el desplazamiento en raw.
            // Si no caben todos junto a lo de la izquierda se quitan empezando por el
            // menos importante (el último: fin de línea, luego codificación).
            let mut fields: Vec<String> = if state.raw.is_some() {
                vec![crate::raw_offset(state.raw_cursor)]
            } else {
                crate::status_right(&state.doc, state.encoding, state.eol).to_vec()
            };
            let sep_w = self.measure("· ", label_font);
            let fields_w = |fs: &[String]| -> f32 {
                fs.iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let w = self.measure(f, label_font);
                        let extra = if merged {
                            if i + 1 < fs.len() { sep_w } else { 0.0 }
                        } else {
                            layout::STATUS_FLD_PAD_X * 2.0 + layout::STATUS_R_GAP
                        };
                        w + extra
                    })
                    .sum()
            };
            while !fields.is_empty() && r.right - layout::STATUS_PAD_X - fields_w(&fields) < left_end + layout::STATUS_GAP {
                fields.pop();
            }
            let mut xr = r.right - layout::STATUS_PAD_X;
            for (i, field) in fields.iter().enumerate().rev() {
                let w = self.measure(field, label_font);
                if merged && i + 1 < fields.len() {
                    let sep = "· ";
                    let sw = self.measure(sep, label_font);
                    xr -= sw;
                    self.text(sep, label_font, Rect::new(xr, r.top, xr + sw, r.bottom), pal.text_3);
                }
                xr -= w;
                self.text(field, label_font, Rect::new(xr, r.top, xr + w, r.bottom), pal.text_2);
                if !merged {
                    xr -= layout::STATUS_FLD_PAD_X * 2.0 + layout::STATUS_R_GAP;
                }
            }
        }
    }

    /// Candado (`LOCK` de la maqueta): sin permiso de escritura, junto al lápiz.
    fn draw_lock(&self, pal: &theme::Palette, x: f32, r: Rect) {
        let top = r.top + (r.height() - 12.0) / 2.0;
        let c = theme::Rgba(pal.text_3.0, pal.text_3.1, pal.text_3.2, pal.text_3.3 * 0.5);
        self.stroke_round_rect(Rect::new(x + 3.0, top + 7.0, x + 10.0, top + 12.0), 1.5, 1.3, c);
        self.stroke_line(x + 5.5, top + 7.0, x + 5.5, top + 5.0, 1.3, c);
        self.stroke_line(x + 5.5, top + 5.0, x + 8.0, top + 5.0, 1.3, c);
        self.stroke_line(x + 8.0, top + 5.0, x + 10.5, top + 7.0, 1.3, c);
    }

    /// Botón de lápiz (`PENCIL`): alterna el permiso de escritura de la vista raw.
    #[allow(unused_unsafe)]
    unsafe fn draw_pencil(&mut self, raw: &crate::RawDoc, pal: &theme::Palette, x: f32, r: Rect) {
        unsafe {
            let btn = Rect::new(x, r.top + (r.height() - 22.0) / 2.0, x + 24.0, r.top + (r.height() + 22.0) / 2.0);
            let (bg, fg) = if raw.is_editing() {
                (Some(pal.accent_soft), pal.accent)
            } else if raw.writable_fs() {
                (None, pal.text_2)
            } else {
                (None, theme::Rgba(pal.text_3.0, pal.text_3.1, pal.text_3.2, pal.text_3.3 * 0.5))
            };
            if let Some(bg) = bg {
                self.fill_round(btn, 4.0, bg);
            }
            let px = btn.left + (btn.width() - 14.0) / 2.0;
            let py = btn.top + (btn.height() - 14.0) / 2.0;
            self.stroke_line(px + 11.0, py + 2.5, px + 13.5, py + 5.0, 1.3, fg);
            self.stroke_line(px + 13.5, py + 5.0, px + 6.0, py + 12.5, 1.3, fg);
            self.stroke_line(px + 6.0, py + 12.5, px + 2.8, py + 13.2, 1.3, fg);
            self.stroke_line(px + 2.8, py + 13.2, px + 3.5, py + 10.0, 1.3, fg);
            self.stroke_line(px + 3.5, py + 10.0, px + 11.0, py + 2.5, 1.3, fg);
            self.hits.push((btn, Hit::Pencil));
        }
    }

    /// Prompt activo (ruta, buscar/reemplazar, línea de comandos vim), ocupando la
    /// franja de estado entera.
    #[allow(unused_unsafe)]
    unsafe fn draw_prompt(&mut self, prompt: &crate::Prompt, state: &EditorState, pal: &theme::Palette, r: Rect, merged: bool, view: &ViewState, suggestion_icons: bool, lang: notty_config::Lang) {
        unsafe {
            match prompt {
                crate::Prompt::Path(p) => self.draw_path_prompt(p, pal, r, view, suggestion_icons, lang),
                crate::Prompt::Find(s) => self.draw_search_bar(s, &state.doc, pal, r, false),
                crate::Prompt::Replace(s) => self.draw_search_bar(s, &state.doc, pal, r, true),
                crate::Prompt::VimCmdline(line) => {
                    let label_c = if merged { pal.accent } else { pal.text_3 };
                    let x = r.left + layout::STATUS_PAD_X;
                    let w = self.measure(":", &self.fonts.ui_11_5);
                    self.text(":", &self.fonts.ui_11_5, Rect::new(x, r.top, x + w, r.bottom), label_c);
                    let fx = x + w + 4.0;
                    self.text(line, &self.fonts.mono_12_5, Rect::new(fx, r.top, r.right - layout::STATUS_PAD_X, r.bottom), pal.text);
                }
                crate::Prompt::CloseUnsaved(req) => self.draw_close_prompt(req, pal, r, view, lang),
                crate::Prompt::Conflict(c) => self.draw_conflict_prompt(c, pal, r, lang),
                crate::Prompt::None => {}
            }
        }
    }

    /// "El archivo cambió en disco": franja en rojo suave con un campo donde escribir
    /// SI o NO (ver `ConflictState`).
    fn draw_conflict_prompt(&mut self, c: &crate::ConflictState, pal: &theme::Palette, r: Rect, lang: notty_config::Lang) {
        self.fill(r, pal.danger.faded(0.12));
        let field_w = 70.0;
        let fr = Rect::new(r.right - layout::STATUS_PAD_X - field_w, r.top + 3.0, r.right - layout::STATUS_PAD_X, r.bottom - 3.0);
        self.fill_round(fr, 4.0, pal.surface);
        self.stroke_round_rect(fr, 4.0, 1.0, pal.danger);
        let tw = self.measure(&c.input, &self.fonts.mono_12_5);
        let tx = fr.left + 8.0;
        self.text(&c.input, &self.fonts.mono_12_5, Rect::new(tx, r.top, fr.right - 4.0, r.bottom), pal.text);
        let caret_x = (tx + tw).min(fr.right - 4.0);
        self.fill(Rect::new(caret_x, r.top + 6.0, caret_x + 1.0, r.bottom - 6.0), pal.text);
        let msg = if c.invalid {
            crate::strings::tr(lang, "Escribe SI o NO y pulsa Enter · Esc cancela").to_string()
        } else if lang == notty_config::Lang::En {
            format!("'{}' changed on disk · YES + Enter saves yours over it · NO + Enter loads the one on disk · Esc cancels", c.name)
        } else {
            format!("«{}» cambió en disco · SI + Enter guarda el tuyo encima · NO + Enter carga el del disco · Esc cancela", c.name)
        };
        let x = r.left + layout::STATUS_PAD_X;
        let right = (fr.left - 10.0).max(x);
        self.push_clip(Rect::new(x, r.top, right, r.bottom));
        self.text(&msg, &self.fonts.ui_11_5, Rect::new(x, r.top, right, r.bottom), pal.danger);
        self.pop_clip();
    }

    /// "«nombre» tiene cambios sin guardar" con tres botones, como "Ya existe".
    fn draw_close_prompt(&mut self, req: &crate::CloseRequest, pal: &theme::Palette, r: Rect, view: &ViewState, lang: notty_config::Lang) {
        let mut xr = r.right - layout::STATUS_PAD_X;
        let choices = [("C", crate::strings::tr(lang, "Cancelar"), 2u8), ("N", crate::strings::tr(lang, "No guardar"), 1u8), ("G", crate::strings::tr(lang, "Guardar"), 0u8)];
        for (key, label, idx) in choices {
            let kw = self.measure(key, &self.fonts.mono_11_bold);
            let lw = self.measure(label, &self.fonts.ui_11_5);
            let w = kw + 5.0 + lw + 14.0;
            xr -= w;
            let chip = Rect::new(xr, r.top + 3.0, xr + w, r.bottom - 3.0);
            let hovered = view.hover == Hit::CloseChoice(idx);
            let primary = idx == 0;
            let bg = if hovered { pal.hover } else if primary { pal.accent_soft } else { pal.surface_2 };
            self.fill_round(chip, 4.0, bg);
            let kc = if primary { pal.accent } else { pal.text_hint };
            self.text(key, &self.fonts.mono_11_bold, Rect::new(chip.left + 7.0, r.top, chip.left + 7.0 + kw, r.bottom), kc);
            let tc = if primary { pal.accent } else { pal.text_2 };
            self.text(label, &self.fonts.ui_11_5, Rect::new(chip.left + 7.0 + kw + 5.0, r.top, chip.right - 7.0, r.bottom), tc);
            self.hits.push((chip, Hit::CloseChoice(idx)));
            xr -= 6.0;
        }
        let more = req.ask.len().saturating_sub(1);
        let msg = if lang == notty_config::Lang::En {
            if more > 0 { format!("'{}' has unsaved changes (and {more} more)", req.name) } else { format!("'{}' has unsaved changes", req.name) }
        } else if more > 0 {
            format!("«{}» tiene cambios sin guardar (y {more} más)", req.name)
        } else {
            format!("«{}» tiene cambios sin guardar", req.name)
        };
        let x = r.left + layout::STATUS_PAD_X;
        self.push_clip(Rect::new(x, r.top, (xr - 10.0).max(x), r.bottom));
        self.text(&msg, &self.fonts.ui_11_5, Rect::new(x, r.top, xr - 10.0, r.bottom), pal.warn);
        self.pop_clip();
    }

    #[allow(unused_unsafe)]
    unsafe fn draw_path_prompt(&mut self, p: &crate::PathPromptState, pal: &theme::Palette, r: Rect, view: &ViewState, suggestion_icons: bool, lang: notty_config::Lang) {
        unsafe {
            let x = r.left + layout::STATUS_PAD_X;
            let value_color = if p.is_invalid() { pal.danger } else { pal.text };

            let placeholder = match p.purpose {
                crate::Purpose::Open => crate::strings::tr(lang, "ruta del archivo"),
                crate::Purpose::Save => crate::strings::tr(lang, "ruta donde guardar"),
            };

            // Primero lo de la derecha (pregunta, error o palabra de estado): el valor se
            // pinta después en el hueco que quede, desplazado para que se vea el final.
            let mut xr = r.right - layout::STATUS_PAD_X;
            if p.ask_overwrite {
                // "Ya existe": tres botones pequeños con su tecla, en el color de aviso
                // (no el rojo de error: no ha fallado nada, solo se pregunta).
                let choices = [("C", crate::strings::tr(lang, "Cancelar"), 2u8), ("A", crate::strings::tr(lang, "Abrir"), 1u8), ("S", crate::strings::tr(lang, "Sobrescribir"), 0u8)];
                for (key, label, idx) in choices {
                    let kw = self.measure(key, &self.fonts.mono_11_bold);
                    let lw = self.measure(label, &self.fonts.ui_11_5);
                    let w = kw + 5.0 + lw + 14.0;
                    xr -= w;
                    let chip = Rect::new(xr, r.top + 3.0, xr + w, r.bottom - 3.0);
                    let hovered = view.hover == Hit::Overwrite(idx);
                    let primary = idx == 0;
                    let bg = if hovered { pal.hover } else if primary { pal.accent_soft } else { pal.surface_2 };
                    self.fill_round(chip, 4.0, bg);
                    let kc = if primary { pal.accent } else { pal.text_hint };
                    self.text(key, &self.fonts.mono_11_bold, Rect::new(chip.left + 7.0, r.top, chip.left + 7.0 + kw, r.bottom), kc);
                    let tc = if primary { pal.accent } else { pal.text_2 };
                    self.text(label, &self.fonts.ui_11_5, Rect::new(chip.left + 7.0 + kw + 5.0, r.top, chip.right - 7.0, r.bottom), tc);
                    self.hits.push((chip, Hit::Overwrite(idx)));
                    xr -= 6.0;
                }
                let q = crate::strings::tr(lang, "Ya existe:");
                let qw = self.measure(q, &self.fonts.ui_11_5);
                xr -= qw + 4.0;
                self.text(q, &self.fonts.ui_11_5, Rect::new(xr, r.top, xr + qw, r.bottom), pal.warn);
                self.draw_path_value(p, x, xr - 10.0, r, placeholder, value_color, pal);
                return;
            }

            // Derecha: si el último intento falló, por qué (en rojo, sustituye a la
            // palabra de estado y a "Tab ↹": no tiene sentido completar ni cerrar el
            // prompt en silencio cuando Enter no pudo hacer lo que pedía).
            let (visible_sugs, total_sugs) = p.visible_suggestions();
            if let Some(err) = &p.last_error {
                let w = self.measure(err, &self.fonts.ui_11_5).min(r.width() * 0.6);
                xr -= w;
                self.text(err, &self.fonts.ui_11_5, Rect::new(xr, r.top, xr + w, r.bottom), pal.danger);
            } else {
                if total_sugs > 0 {
                    let tab_label = crate::strings::tr(lang, "Tab ↹");
                    let w = self.measure(tab_label, &self.fonts.ui_11_5);
                    xr -= w;
                    self.text(tab_label, &self.fonts.ui_11_5, Rect::new(xr, r.top, xr + w, r.bottom), pal.text_3);
                    xr -= 10.0;
                }
                let word = crate::strings::tr(lang, p.hint_word());
                if !word.is_empty() {
                    let wc = if p.is_invalid() { pal.danger } else { pal.text_3 };
                    let w = self.measure(word, &self.fonts.ui_11_5);
                    xr -= w;
                    self.text(word, &self.fonts.ui_11_5, Rect::new(xr, r.top, xr + w, r.bottom), wc);
                }
            }
            self.draw_path_value(p, x, xr - 10.0, r, placeholder, value_color, pal);

            if p.last_error.is_none() && !visible_sugs.is_empty() {
                self.draw_suggestions(&visible_sugs, p.scroll, p.selected, total_sugs, pal, r, view, suggestion_icons);
            }
        }
    }

    /// Valor de la línea de ruta (o su texto de ayuda si está vacía) entre `x` y
    /// `right`: si no cabe, se desplaza a la izquierda para que el final (donde se
    /// escribe) siga a la vista, y se recorta en vez de pisar lo de la derecha.
    #[allow(clippy::too_many_arguments)]
    fn draw_path_value(
        &self,
        p: &crate::PathPromptState,
        x: f32,
        right: f32,
        r: Rect,
        placeholder: &str,
        value_color: Rgba,
        pal: &theme::Palette,
    ) {
        let right = right.max(x + 40.0);
        self.push_clip(Rect::new(x - 2.0, r.top, right, r.bottom));
        if p.value.is_empty() {
            let w = self.measure(placeholder, &self.fonts.mono_12_5);
            self.text(placeholder, &self.fonts.mono_12_5, Rect::new(x, r.top, x + w, r.bottom), pal.text_3);
        } else {
            let vw = self.measure(&p.value, &self.fonts.mono_12_5);
            let x = x + (right - x - 2.0 - vw).min(0.0);
            if p.all_selected {
                self.fill_round(Rect::new(x - 1.0, r.top + 4.0, x + vw + 1.0, r.bottom - 4.0), 2.0, pal.accent_soft);
            }
            self.text(&p.value, &self.fonts.mono_12_5, Rect::new(x, r.top, x + vw, r.bottom), value_color);
            let caret_x = x + vw;
            let ghost = if p.all_selected || p.ask_overwrite { String::new() } else { p.ghost() };
            if !ghost.is_empty() {
                let gw = self.measure(&ghost, &self.fonts.mono_12_5);
                self.text(&ghost, &self.fonts.mono_12_5, Rect::new(caret_x, r.top, caret_x + gw, r.bottom), pal.text_3);
            }
            if !p.ask_overwrite {
                self.fill(Rect::new(caret_x, r.top + 4.0, caret_x + 1.0, r.bottom - 4.0), pal.text);
            }
        }
        self.pop_clip();
    }

    /// Lista de sugerencias de ruta (`.psuggest`), hasta `VISIBLE_SUGGESTIONS` filas a
    /// la vez, justo encima de la barra de estado; `scroll` es el índice absoluto del
    /// primer `entry` visible (para que los clics y el resaltado apunten al candidato
    /// real dentro de la lista completa, no a la posición dentro de la ventana visible).
    /// Con más candidatos que caben, un contador "3/12" avisa de que hay más y que se
    /// puede seguir con el scroll. La sombra difusa se aproxima con `draw_popup_shadow`
    /// (varias capas desplazadas hacia abajo) más un anillo de 1 px en `shadow_ring`.
    #[allow(unused_unsafe, clippy::too_many_arguments)]
    unsafe fn draw_suggestions(
        &mut self,
        sugs: &[notty_io::Entry],
        scroll: usize,
        selected: usize,
        total: usize,
        pal: &theme::Palette,
        status: Rect,
        view: &ViewState,
        show_icons: bool,
    ) {
        unsafe {
            // Mismo tratamiento de fundido + desplazamiento que `draw_dropdown` (Task 3).
            let t = view.popup_open.unwrap_or(1.0);
            let dy = (1.0 - t) * 4.0;
            let draw_y = |y: f32| y - dy;

            let row_h = 24.0;
            let has_more = total > sugs.len();
            let counter_h = if has_more { 20.0 } else { 0.0 };
            let icon_w = if show_icons { 20.0 } else { 0.0 };
            let longest = sugs.iter().map(|e| self.measure(&e.name, &self.fonts.mono_12)).fold(0.0f32, f32::max);
            let (win_w, _) = self.size_dips();
            let width = (longest + icon_w + 16.0 + 16.0).clamp(220.0, 360.0).min((win_w - 24.0).max(120.0));
            let height = row_h * sugs.len() as f32 + counter_h + layout::POPUP_PAD * 2.0;
            // Posición final (sin desplazar): la usada para el hit-testing, que no anima.
            let box_r = Rect::new(12.0, status.top - 2.0 - height, 12.0 + width, status.top - 2.0);
            let draw_r = Rect::new(box_r.left, draw_y(box_r.top), box_r.right, draw_y(box_r.bottom));

            self.draw_popup_shadow(draw_r, layout::POPUP_RADIUS, t, pal.shadow);
            self.fill_round(draw_r, layout::POPUP_RADIUS, pal.chrome_hi.faded(t));
            self.stroke_round_rect(draw_r, layout::POPUP_RADIUS, 1.0, pal.shadow_ring.faded(t));

            for (rel, entry) in sugs.iter().enumerate() {
                let i = scroll + rel;
                let row_top = box_r.top + layout::POPUP_PAD + row_h * rel as f32;
                let row = Rect::new(box_r.left + layout::POPUP_PAD, row_top, box_r.right - layout::POPUP_PAD, row_top + row_h);
                let draw_row = Rect::new(draw_r.left + layout::POPUP_PAD, draw_y(row_top), draw_r.right - layout::POPUP_PAD, draw_y(row_top + row_h));
                if i == selected {
                    self.fill_round(draw_row, 5.0, pal.accent_soft.faded(t));
                }
                let label = if entry.is_dir { format!("{}\\", entry.name) } else { entry.name.clone() };
                let c = if i == selected { pal.accent } else if entry.is_dir { pal.text } else { pal.text_2 };
                let text_left = if show_icons {
                    let icon_r = Rect::new(draw_row.left + 8.0, draw_row.top, draw_row.left + 8.0 + 14.0, draw_row.bottom);
                    self.draw_suggestion_icon(entry.is_dir, icon_r, c.faded(t));
                    draw_row.left + 8.0 + icon_w
                } else {
                    draw_row.left + 8.0
                };
                self.text(&label, &self.fonts.mono_12, Rect::new(text_left, draw_row.top, draw_row.right - 8.0, draw_row.bottom), c.faded(t));
                self.hits.push((row, Hit::Suggestion(i)));
            }

            if has_more {
                // "3-7 de 42": qué tramo de la lista completa se está viendo, para que
                // quede claro que hay más candidatos y que el scroll los revela.
                let counter_top = box_r.top + layout::POPUP_PAD + row_h * sugs.len() as f32;
                let label = if self.lang() == notty_config::Lang::En {
                    format!("{}-{} of {}", scroll + 1, scroll + sugs.len(), total)
                } else {
                    format!("{}-{} de {}", scroll + 1, scroll + sugs.len(), total)
                };
                self.text(
                    &label,
                    &self.fonts.ui_11_5,
                    Rect::new(draw_r.left + layout::POPUP_PAD + 8.0, draw_y(counter_top), draw_r.right - layout::POPUP_PAD, draw_y(counter_top + counter_h)),
                    pal.text_3.faded(t),
                );
            }
        }
    }

    /// Icono plano de carpeta o archivo delante de cada sugerencia de ruta (Ajustes →
    /// Archivos → "Iconos en las sugerencias"). Centrado en `r` (14x14 aprox.), del
    /// mismo color que la etiqueta de esa fila.
    fn draw_suggestion_icon(&mut self, is_dir: bool, r: Rect, c: Rgba) {
        let cx = r.left + r.width() / 2.0;
        let cy = r.top + r.height() / 2.0;
        if is_dir {
            let body = Rect::new(cx - 6.0, cy - 3.0, cx + 6.0, cy + 5.0);
            let tab = Rect::new(cx - 6.0, cy - 5.0, cx - 1.0, cy - 2.0);
            self.fill_round(tab, 1.0, c);
            self.fill_round(body, 1.5, c);
        } else {
            let page = Rect::new(cx - 4.5, cy - 6.0, cx + 4.5, cy + 6.0);
            self.stroke_round_rect(page, 1.0, 1.2, c);
            self.stroke_line(cx - 2.5, cy - 1.5, cx + 2.5, cy - 1.5, 1.0, c);
            self.stroke_line(cx - 2.5, cy + 1.5, cx + 2.5, cy + 1.5, 1.0, c);
        }
    }

    #[allow(unused_unsafe)]
    unsafe fn draw_search_bar(&mut self, s: &crate::SearchState, doc: &notty_core::Document, pal: &theme::Palette, r: Rect, is_replace: bool) {
        unsafe {
            // Cluster de la derecha: contador + Aa/ab/.* , calculado primero para saber
            // cuánto espacio le queda al campo de la izquierda.
            let count = s.count_label(doc);
            let count_color = if count == "regex ✕" { pal.danger } else { pal.text_3 };
            let count_w = self.measure(&count, &self.fonts.mono_11_5).max(44.0);

            let mut xr = r.right - layout::STATUS_PAD_X;
            let opts = [(".*", s.opts.regex, 2u8), ("ab", s.opts.whole_word, 1u8), ("Aa", s.opts.case_sensitive, 0u8)];
            let mut opt_rects = Vec::new();
            for (label, on, idx) in opts {
                let w = self.measure(label, &self.fonts.mono_11) + 10.0;
                xr -= w;
                let rr = Rect::new(xr, r.top + 3.0, xr + w, r.bottom - 3.0);
                opt_rects.push((rr, label, on, idx));
                xr -= 4.0;
            }
            xr -= 10.0;
            let count_r = Rect::new(xr - count_w, r.top, xr, r.bottom);
            self.text_right(&count, &self.fonts.mono_11_5, count_r, count_color);
            let right_cluster_start = count_r.left - 10.0;

            for (rr, label, on, idx) in opt_rects {
                let (bg, fg) = if on { (Some(pal.accent_soft), pal.accent) } else { (None, pal.text_3) };
                if let Some(bg) = bg {
                    self.fill_round(rr, 3.0, bg);
                }
                self.text(label, &self.fonts.mono_11, rr, fg);
                self.hits.push((rr, Hit::SearchOpt(idx)));
            }

            // Izquierda: etiqueta(s) + campo(s), en el espacio libre hasta `right_cluster_start`.
            let mut x = r.left + layout::STATUS_PAD_X;
            let verb = self.tr("buscar");
            let vw = self.measure(verb, &self.fonts.ui_11_5);
            self.text(verb, &self.fonts.ui_11_5, Rect::new(x, r.top, x + vw, r.bottom), pal.text_3);
            x += vw + 8.0;

            if is_replace {
                let field_w = ((right_cluster_start - x) / 2.0 - 40.0).max(40.0);
                let field1 = Rect::new(x, r.top, x + field_w, r.bottom);
                self.draw_search_field(&s.query, field1, pal, !s.editing_replacement);
                self.hits.push((field1, Hit::SearchField(0)));
                x = field1.right + 10.0;

                let por = self.tr("por");
                let pw = self.measure(por, &self.fonts.ui_11_5);
                self.text(por, &self.fonts.ui_11_5, Rect::new(x, r.top, x + pw, r.bottom), pal.text_3);
                x += pw + 8.0;

                let field2 = Rect::new(x, r.top, right_cluster_start, r.bottom);
                self.draw_search_field(&s.replacement, field2, pal, s.editing_replacement);
                self.hits.push((field2, Hit::SearchField(1)));
            } else {
                let field1 = Rect::new(x, r.top, right_cluster_start, r.bottom);
                self.draw_search_field(&s.query, field1, pal, true);
                self.hits.push((field1, Hit::SearchField(0)));
            }
        }
    }

    fn draw_search_field(&self, value: &str, r: Rect, pal: &theme::Palette, active_caret: bool) {
        let vw = self.measure(value, &self.fonts.mono_12_5);
        self.text(value, &self.fonts.mono_12_5, r, pal.text);
        if active_caret {
            let cx = r.left + vw.min(r.width().max(0.0));
            self.fill(Rect::new(cx, r.top + 4.0, cx + 1.0, r.bottom - 4.0), pal.text);
        }
    }

    /// Cuadrícula hexadecimal (`renderHex`): origen `(body.left+16, body.top+10)`,
    /// `mono_13`, interlineado `LINE_H`. Como `mono_13` es monoespacial, cada columna
    /// cae en un múltiplo exacto del ancho de un carácter (`digit_width`), así que las
    /// posiciones se calculan aritméticamente en vez de con `HitTestTextPosition` —
    /// más simple y evita necesitar un `IDWriteTextRenderer` a medida para colorear
    /// rangos dentro de un único layout por fila (desviación de tiempo respecto al
    /// plan, que pedía `SetDrawingEffect`; el resultado visual es el mismo).
    /// Pista y pulgar de la barra de scroll del editor (vacía si el documento cabe
    /// entero: `total_rows <= viewport.visible_lines`, como en la maqueta no hay barra
    /// si no hace falta). Sirve igual para texto normal (una "fila" = una línea) que
    /// para la vista raw (una "fila" = 16 bytes), según lo que le pase `paint`.
    pub(crate) fn editor_scrollbar_geom(body: layout::Rect, viewport: &Viewport, total_rows: usize) -> Option<(Rect, Rect)> {
        if total_rows <= viewport.visible_lines {
            return None;
        }
        let track = Rect::new(body.right - 10.0, body.top + 4.0, body.right - 4.0, body.bottom - 4.0);
        let thumb_h = (track.height() * viewport.visible_lines as f32 / total_rows as f32).clamp(24.0, track.height());
        let max_first = (total_rows - viewport.visible_lines) as f32;
        let free = (track.height() - thumb_h).max(1.0);
        let top = track.top + free * (viewport.first_line as f32 / max_first).clamp(0.0, 1.0);
        Some((track, Rect::new(track.left, top, track.right, top + thumb_h)))
    }

    fn draw_editor_scrollbar(&mut self, body: layout::Rect, viewport: &Viewport, total_rows: usize, pal: &theme::Palette, view: &ViewState) {
        let Some((track, thumb)) = Self::editor_scrollbar_geom(body, viewport, total_rows) else { return };
        let active = view.pressed == Hit::ScrollThumb || view.hover == Hit::ScrollThumb;
        let c = if active { pal.text_2 } else { pal.text_3 };
        self.fill_round(thumb, thumb.width() / 2.0, c);
        self.hits.push((track, Hit::ScrollTrack));
        self.hits.push((Rect::new(thumb.left - 2.0, thumb.top, thumb.right + 2.0, thumb.bottom), Hit::ScrollThumb));
    }

    fn draw_hex(&mut self, raw: &crate::RawDoc, cursor: usize, pending_nibble: Option<u8>, first_row: usize, pal: &theme::Palette, frame: layout::Frame) {
        const PREFIX_COLS: f32 = 11.0; // "XXXXXXXX   " (8 dígitos de offset + 3 espacios)
        const ASCII_COL: f32 = 61.0; // PREFIX_COLS + 16*3 + 1 (espacio extra tras el 8º) + 1 (espacio literal)

        let char_w = self.digit_width();
        let origin_x = frame.body.left + layout::HEX_PAD_L;
        let origin_y = frame.body.top + layout::TEXT_PAD_T;
        let total = raw.len();
        let rows = total.div_ceil(16).max(1);
        let sel_row = cursor / 16;
        let sel_col = cursor % 16;

        let mut y = origin_y;
        for row in first_row..rows {
            if y > frame.body.bottom {
                break;
            }
            let row_start = row * 16;
            let row_len = (total - row_start).min(16);

            let offset_str = format!("{row_start:08x}");
            self.text(&offset_str, &self.fonts.mono_13, Rect::new(origin_x, y, origin_x + 8.0 * char_w, y + self.line_height()), pal.text_3);

            for col in 0..row_len {
                let i = row_start + col;
                let b = raw.byte(i);
                let is_sel = row == sel_row && col == sel_col;
                let modified = raw.is_modified(i);
                let col_x = origin_x + (PREFIX_COLS + col as f32 * 3.0 + if col >= 8 { 1.0 } else { 0.0 }) * char_w;
                let cell = Rect::new(col_x, y, col_x + 2.0 * char_w, y + self.line_height());

                if is_sel {
                    self.fill_round(cell, 2.0, pal.accent);
                }
                let (label, font, color) = if is_sel {
                    if let Some(hi) = pending_nibble {
                        (format!("{hi:X}"), &self.fonts.mono_13_bold, pal.on_accent)
                    } else {
                        (format!("{b:02X}"), &self.fonts.mono_13, pal.on_accent)
                    }
                } else if modified {
                    (format!("{b:02X}"), &self.fonts.mono_13_bold, pal.accent)
                } else if b == 0 {
                    (format!("{b:02X}"), &self.fonts.mono_13, pal.text_3)
                } else {
                    (format!("{b:02X}"), &self.fonts.mono_13, pal.text)
                };
                self.text(&label, font, cell, color);

                let ascii_x = origin_x + (ASCII_COL + col as f32) * char_w;
                let ascii_cell = Rect::new(ascii_x, y, ascii_x + char_w, y + self.line_height());
                let ch = if (0x20..=0x7E).contains(&b) { b as char } else { '.' };
                if is_sel {
                    self.fill(ascii_cell, pal.accent_soft);
                    self.text(&ch.to_string(), &self.fonts.mono_13, ascii_cell, pal.text);
                } else {
                    self.text(&ch.to_string(), &self.fonts.mono_13, ascii_cell, pal.text_2);
                }
            }

            y += self.line_height();
        }
    }
}

/// Coordenada X del punto de inserción para el char `local_char_offset` (relativo al
/// inicio de `text`) dentro de `layout`.
unsafe fn hit_test_x(
    text_layout: &windows::Win32::Graphics::DirectWrite::IDWriteTextLayout,
    text: &str,
    local_char_offset: usize,
    pad: f32,
) -> f32 {
    unsafe { hit_test_xy(text_layout, text, local_char_offset, pad).0 }
}

/// Como `hit_test_x`, pero también da el desplazamiento vertical dentro de la línea
/// (0 si no hace wrap): con `ui.wrap`, un carácter puede caer en una fila visual
/// distinta de la primera.
unsafe fn hit_test_xy(
    text_layout: &windows::Win32::Graphics::DirectWrite::IDWriteTextLayout,
    text: &str,
    local_char_offset: usize,
    pad: f32,
) -> (f32, f32) {
    unsafe {
        let utf16_offset = char_offset_to_utf16(text, local_char_offset);
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        let mut metrics = Default::default();
        if text_layout.HitTestTextPosition(utf16_offset, false, &mut x, &mut y, &mut metrics).is_ok() {
            (pad + x, y)
        } else {
            (pad, 0.0)
        }
    }
}

/// Rectángulos (uno por fila visual) que ocupa `[local_start, local_end)` de `text`
/// dentro de `text_layout`, ya en coordenadas absolutas (`pad`/`row_top` sumados).
/// Sin wrap siempre devuelve como mucho uno; con wrap, una selección o coincidencia de
/// búsqueda a caballo entre dos filas visuales de la misma línea de buffer da varios.
unsafe fn hit_test_range_rects(
    text_layout: &windows::Win32::Graphics::DirectWrite::IDWriteTextLayout,
    text: &str,
    local_start: usize,
    local_end: usize,
    pad: f32,
    row_top: f32,
) -> Vec<(f32, f32, f32, f32)> {
    unsafe {
        let start16 = char_offset_to_utf16(text, local_start);
        let end16 = char_offset_to_utf16(text, local_end);
        let len16 = end16.saturating_sub(start16);
        let mut count = 0u32;
        let mut buf = [windows::Win32::Graphics::DirectWrite::DWRITE_HIT_TEST_METRICS::default(); 8];
        let _ = text_layout.HitTestTextRange(start16, len16, pad, row_top, Some(&mut buf), &mut count);
        buf[..(count as usize).min(buf.len())].iter().map(|m| (m.left, m.top, m.width, m.height)).collect()
    }
}

#[cfg(test)]
mod scrollbar_tests {
    use super::*;

    fn vp(first_line: usize, visible_lines: usize) -> Viewport {
        Viewport { first_line, visible_lines }
    }

    #[test]
    fn no_scrollbar_when_document_fits() {
        let body = Rect::new(0.0, 0.0, 200.0, 400.0);
        assert!(Renderer::editor_scrollbar_geom(body, &vp(0, 20), 20).is_none());
        assert!(Renderer::editor_scrollbar_geom(body, &vp(0, 20), 10).is_none());
    }

    #[test]
    fn thumb_at_top_when_first_line_is_zero() {
        let body = Rect::new(0.0, 0.0, 200.0, 400.0);
        let (track, thumb) = Renderer::editor_scrollbar_geom(body, &vp(0, 10), 100).unwrap();
        assert_eq!(thumb.top, track.top);
    }

    #[test]
    fn thumb_at_bottom_when_scrolled_to_the_end() {
        let body = Rect::new(0.0, 0.0, 200.0, 400.0);
        let (track, thumb) = Renderer::editor_scrollbar_geom(body, &vp(90, 10), 100).unwrap();
        assert!((thumb.bottom - track.bottom).abs() < 0.01);
    }
}
