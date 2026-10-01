//! Modelo puro de la ventana de Ajustes (maqueta `Prototipo.dc.html`): qué páginas
//! hay (y cuáles son subpáginas de otra), las opciones de cada selector y qué le
//! pasa a `Config` cuando se toca algo. No sabe dibujar ni de Win32/Direct2D.

use notty_config::{
    AccentColor, Config, Files, FontFamily, HotkeyMechanism, Lang, MenuBar, OnCloseUnsaved, Preset, TabsPosition, TempMode,
    Theme,
};
use notty_input::Command;

/// Una página de Ajustes. Las de la barra lateral están en `Page::RAIL`; `Fuentes`,
/// `Ligaduras` y `Sintaxis` son subpáginas de `Apariencia` (como las rutas de la Configuración de
/// Windows: cabecera con miga de pan y la barra sigue marcando a la madre).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Apariencia,
    Fuentes,
    Ligaduras,
    Sintaxis,
    Ventana,
    Teclado,
    Archivos,
    AtajoGlobal,
    Actualizaciones,
    AcercaDe,
    Ayuda,
}

impl Page {
    pub const RAIL: [Page; 8] = [
        Page::Apariencia,
        Page::Ventana,
        Page::Teclado,
        Page::Archivos,
        Page::AtajoGlobal,
        Page::Actualizaciones,
        Page::AcercaDe,
        Page::Ayuda,
    ];

    const ALL: [Page; 11] = [
        Page::Apariencia,
        Page::Fuentes,
        Page::Ligaduras,
        Page::Sintaxis,
        Page::Ventana,
        Page::Teclado,
        Page::Archivos,
        Page::AtajoGlobal,
        Page::Actualizaciones,
        Page::AcercaDe,
        Page::Ayuda,
    ];

    /// Lo que se pasa a `open_settings_at` para abrir directamente en esta página.
    pub fn id(self) -> &'static str {
        match self {
            Page::Apariencia => "apariencia",
            Page::Fuentes => "fuentes",
            Page::Ligaduras => "ligaduras",
            Page::Sintaxis => "sintaxis",
            Page::Ventana => "ventana",
            Page::Teclado => "teclado",
            Page::Archivos => "archivos",
            Page::AtajoGlobal => "atajo_global",
            Page::Actualizaciones => "actualizaciones",
            Page::AcercaDe => "acerca_de",
            Page::Ayuda => "ayuda",
        }
    }

    pub fn from_id(id: &str) -> Option<Page> {
        Page::ALL.iter().copied().find(|p| p.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Page::Apariencia => "Apariencia",
            Page::Fuentes => "Fuentes",
            Page::Ligaduras => "Ligaduras",
            Page::Sintaxis => "Sintaxis",
            Page::Ventana => "Ventana",
            Page::Teclado => "Teclado",
            Page::Archivos => "Archivos",
            Page::AtajoGlobal => "Atajo global",
            Page::Actualizaciones => "Actualizaciones",
            Page::AcercaDe => "Acerca de",
            Page::Ayuda => "Ayuda",
        }
    }

    /// Página madre de una subpágina.
    pub fn parent(self) -> Option<Page> {
        match self {
            Page::Fuentes | Page::Ligaduras | Page::Sintaxis => Some(Page::Apariencia),
            _ => None,
        }
    }

    /// La que se marca en la barra lateral.
    pub fn rail_page(self) -> Page {
        self.parent().unwrap_or(self)
    }

    pub fn rail_index(self) -> usize {
        Page::RAIL.iter().position(|p| *p == self.rail_page()).unwrap_or(0)
    }
}

/// Qué ajuste toca un control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingKey {
    Preset,
    Theme,
    LineNumbers,
    Files,
    TabsPosition,
    MenuBar,
    HintsBar,
    StatusBar,
    MergedCommandLine,
    VimAlways,
    TempMode,
    Autosave,
    HotkeyMechanism,
    StartWithWindows,
    UpdatesCheck,
    SuggestionIcons,
    NativeFileDialog,
    FontFamily,
    Ligatures,
    SyntaxHighlight,
    OnCloseUnsaved,
    OpenInExistingWindow,
    AccentColor,
    ReopenPrevious,
    TabIcons,
    Lang,
    Wrap,
    SaveDir,
    MdOpenMode,
    MdPreviewStyle,
    AnimHz,
    ReducedMotion,
    FollowEssentials,
}

/// El valor elegido; `apply` decide qué campo de `Config` toca según `SettingKey`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingValue {
    Preset(Preset),
    Theme(Theme),
    Files(Files),
    TabsPosition(TabsPosition),
    MenuBar(MenuBar),
    Bool(bool),
    TempMode(TempMode),
    HotkeyMechanism(HotkeyMechanism),
    FontFamily(FontFamily),
    OnCloseUnsaved(OnCloseUnsaved),
    AccentColor(AccentColor),
    Lang(Lang),
    SaveDir(SaveDirChoice),
    MdOpenMode(notty_config::MdOpenMode),
    MdPreviewStyle(notty_config::MdPreviewStyle),
    AnimHz(notty_config::AnimHz),
}

/// Carpeta de partida para "Guardar como" cuando el documento no tiene ruta todavía
/// (Ajustes → Archivos → "Guardar por defecto en"). Se guarda como ruta resuelta
/// (`FilesConfig::default_save_dir`), no como esta elección: si el usuario mueve o
/// renombra Escritorio/Documentos/Descargas, `selected_index` simplemente deja de
/// marcar ninguna opción, en vez de guardar una ruta que ya no significa lo mismo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SaveDirChoice {
    UserFolder,
    Desktop,
    Documents,
    Downloads,
}

/// Opciones de un selector: `(etiqueta, valor)`.
pub type Options = &'static [(&'static str, SettingValue)];

pub const PRESET_OPTS: Options = &[
    ("Moderna", SettingValue::Preset(Preset::Moderna)),
    ("Clásica", SettingValue::Preset(Preset::Clasica)),
    ("Zen", SettingValue::Preset(Preset::Zen)),
];

pub const THEME_OPTS: Options = &[
    ("Sistema", SettingValue::Theme(Theme::System)),
    ("Claro", SettingValue::Theme(Theme::Light)),
    ("Oscuro", SettingValue::Theme(Theme::Dark)),
];

pub const FILES_OPTS: Options = &[
    ("Pestañas", SettingValue::Files(Files::Tabs)),
    ("Buffers", SettingValue::Files(Files::Buffers)),
    ("Paneles", SettingValue::Files(Files::Splits)),
];

/// Descripción de cada opción de `FILES_OPTS`, en el mismo orden.
pub const FILES_DESC: [&str; 3] = [
    "Una pestaña por archivo, visibles arriba.",
    "Estilo vim: sin pestañas, cambias con Ctrl+Tab o :b.",
    "Pestañas que se dividen en hasta 4 paneles, como kitty. Ctrl+Shift+Enter divide, Ctrl+Shift+R acomoda.",
];

pub const TABS_POSITION_OPTS: Options = &[
    ("En el título", SettingValue::TabsPosition(TabsPosition::Title)),
    ("Bajo el menú", SettingValue::TabsPosition(TabsPosition::Below)),
    ("Si hay más de 1", SettingValue::TabsPosition(TabsPosition::Auto)),
    ("Ocultas", SettingValue::TabsPosition(TabsPosition::Hidden)),
];

pub const MENU_BAR_OPTS: Options = &[
    ("Oculta", SettingValue::MenuBar(MenuBar::Hidden)),
    ("Visible", SettingValue::MenuBar(MenuBar::Visible)),
    ("Con Alt", SettingValue::MenuBar(MenuBar::Alt)),
];

pub const TEMP_MODE_OPTS: Options = &[
    ("Borrador", SettingValue::TempMode(TempMode::Draft)),
    ("Volátil", SettingValue::TempMode(TempMode::Volatile)),
];

pub const ON_CLOSE_OPTS: Options = &[
    ("Preguntar", SettingValue::OnCloseUnsaved(OnCloseUnsaved::Preguntar)),
    ("Recuperar al abrir", SettingValue::OnCloseUnsaved(OnCloseUnsaved::Recuperar)),
];

pub const ACCENT_OPTS: Options = &[
    ("Azul", SettingValue::AccentColor(AccentColor::Azul)),
    ("Verde", SettingValue::AccentColor(AccentColor::Verde)),
    ("Turquesa", SettingValue::AccentColor(AccentColor::Turquesa)),
    ("Morado", SettingValue::AccentColor(AccentColor::Morado)),
    ("Rosa", SettingValue::AccentColor(AccentColor::Rosa)),
    ("Rojo", SettingValue::AccentColor(AccentColor::Rojo)),
    ("Naranja", SettingValue::AccentColor(AccentColor::Naranja)),
    ("Amarillo", SettingValue::AccentColor(AccentColor::Amarillo)),
];

pub const LANG_OPTS: Options = &[
    ("Auto", SettingValue::Lang(Lang::Auto)),
    ("Español", SettingValue::Lang(Lang::Es)),
    ("English", SettingValue::Lang(Lang::En)),
];

pub const HOTKEY_OPTS: Options = &[
    ("Segundo plano", SettingValue::HotkeyMechanism(HotkeyMechanism::Daemon)),
    ("Acceso directo", SettingValue::HotkeyMechanism(HotkeyMechanism::Lnk)),
];

pub const SAVE_DIR_OPTS: Options = &[
    ("Carpeta de usuario", SettingValue::SaveDir(SaveDirChoice::UserFolder)),
    ("Escritorio", SettingValue::SaveDir(SaveDirChoice::Desktop)),
    ("Documentos", SettingValue::SaveDir(SaveDirChoice::Documents)),
    ("Descargas", SettingValue::SaveDir(SaveDirChoice::Downloads)),
];

/// Ruta de verdad de una elección de `SAVE_DIR_OPTS` (`None` para `UserFolder`, que
/// no fija ninguna y deja el comportamiento de siempre).
fn save_dir_path(choice: SaveDirChoice) -> Option<std::path::PathBuf> {
    use crate::native_dialog::{KnownFolder, known_folder_path};
    match choice {
        SaveDirChoice::UserFolder => None,
        SaveDirChoice::Desktop => known_folder_path(KnownFolder::Desktop),
        SaveDirChoice::Documents => known_folder_path(KnownFolder::Documents),
        SaveDirChoice::Downloads => known_folder_path(KnownFolder::Downloads),
    }
}

pub const MD_OPEN_MODE_OPTS: Options = &[
    ("Texto", SettingValue::MdOpenMode(notty_config::MdOpenMode::Texto)),
    ("Previsualización", SettingValue::MdOpenMode(notty_config::MdOpenMode::Preview)),
];

pub const MD_PREVIEW_STYLE_OPTS: Options = &[
    ("Solo lectura", SettingValue::MdPreviewStyle(notty_config::MdPreviewStyle::ReadOnly)),
    ("En línea", SettingValue::MdPreviewStyle(notty_config::MdPreviewStyle::Inline)),
];

pub const ANIM_HZ_OPTS: Options = &[
    ("30 Hz", SettingValue::AnimHz(notty_config::AnimHz::Hz30)),
    ("60 Hz", SettingValue::AnimHz(notty_config::AnimHz::Hz60)),
    ("120 Hz", SettingValue::AnimHz(notty_config::AnimHz::Hz120)),
];

/// Atajos reasignables de Teclado, en el orden en que se listan.
pub const BINDINGS: &[Command] = Command::ALL;

/// Qué hace un enlace o botón de acción (no un ajuste).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkAction {
    /// Abre config.toml como documento en la ventana principal.
    OpenConfig,
    /// Ajustes → Ayuda → "Repetir tutorial": arranca el recorrido guiado directamente
    /// (no vuelve a mostrar la ventana de bienvenida).
    RepeatTutorial,
    /// Actualizaciones → "Buscar actualizaciones": el chequeo ya mismo, sin esperar
    /// al de una vez al día.
    CheckUpdatesNow,
    /// Actualizaciones → "Descargar e instalar".
    DownloadUpdate,
    /// Notas completas de la versión nueva en GitHub.
    ReleaseNotes,
    /// Acerca de → Código fuente.
    OpenRepo,
    /// Acerca de → Informar de un problema (incidencias de GitHub).
    OpenIssues,
    /// Acerca de → Novedades (el popup de lo nuevo).
    OpenChangelog,
    /// Acerca de → Carpeta de configuración.
    OpenConfigFolder,
    /// Ventana → "Restablecer tamaño de ventana": vuelve a `DEFAULT_WIN_W/H`, sin
    /// maximizar, para la próxima vez que se abra notty (no mueve la ventana actual:
    /// Ajustes es su propia ventana, no puede redimensionar la principal desde aquí).
    ResetWindowSize,
    /// Actualizaciones gestionadas → «Abrir en essentials» (`essentials.exe --app notty`).
    OpenEssentials,
    /// Actualizaciones sin essentials → «Instalar essentials» / «Reintentar».
    InstallEssentials,
    /// Cancela la descarga del instalador de essentials.
    CancelEssentials,
}

/// Tamaño de letra base del editor (`layout::FONT_MONO`), para traducir el
/// deslizador "Tamaño" (px) al zoom que guarda `ui.font_scale`.
const BASE_PX: f32 = crate::layout::FONT_MONO;
pub const FONT_PX_MIN: u32 = 10;
pub const FONT_PX_MAX: u32 = 24;

/// Tamaño del texto del editor en px, tal como se enseña en Fuentes.
pub fn font_px(cfg: &Config) -> u32 {
    ((BASE_PX * cfg.ui.font_scale).round() as u32).clamp(FONT_PX_MIN, FONT_PX_MAX)
}

pub fn set_font_px(cfg: &mut Config, px: u32) {
    cfg.ui.font_scale = px.clamp(FONT_PX_MIN, FONT_PX_MAX) as f32 / BASE_PX;
}

/// Enciende/apaga una ligadura suelta (de serie o propia).
pub fn toggle_ligature(cfg: &mut Config, seq: &str) {
    if let Some(i) = cfg.ligature_disabled.iter().position(|s| s == seq) {
        cfg.ligature_disabled.remove(i);
    } else {
        cfg.ligature_disabled.push(seq.to_string());
    }
}

/// Por qué no se puede añadir una ligadura (el botón se queda desactivado).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddLigatureError {
    Empty,
    /// Una secuencia de un solo carácter sustituiría ese carácter en todo el texto.
    TooShort,
    /// Tiene espacios: nunca casaría con lo que se escribe como una sola "palabra".
    Whitespace,
}

pub fn can_add_ligature(seq: &str, glyph: &str) -> Result<(), AddLigatureError> {
    let seq = seq.trim();
    if seq.is_empty() || glyph.trim().is_empty() {
        return Err(AddLigatureError::Empty);
    }
    if seq.chars().any(char::is_whitespace) {
        return Err(AddLigatureError::Whitespace);
    }
    if seq.chars().count() < 2 {
        return Err(AddLigatureError::TooShort);
    }
    Ok(())
}

/// Añade (o pisa) una ligadura propia `seq → glyph` en `[ligature_overrides]`. Solo
/// cuenta el primer carácter de `glyph` (ver `ligature::resolve`).
pub fn add_ligature(cfg: &mut Config, seq: &str, glyph: &str) -> Result<(), AddLigatureError> {
    can_add_ligature(seq, glyph)?;
    let seq = seq.trim().to_string();
    let glyph: String = glyph.trim().chars().take(1).collect();
    cfg.ligature_disabled.retain(|s| *s != seq);
    cfg.ligature_overrides.insert(seq, glyph);
    Ok(())
}

/// Borra una ligadura propia (o devuelve una de serie a su carácter original).
pub fn remove_ligature(cfg: &mut Config, seq: &str) {
    cfg.ligature_overrides.remove(seq);
    cfg.ligature_disabled.retain(|s| s != seq);
}

/// Aplica el ajuste `key`/`value` a `cfg`, como `setSetting` de la maqueta: elegir un
/// preset aplica sus piezas fijas; tocar cualquier otra pieza de Apariencia/Ventana
/// deja el preset en `Custom`.
pub fn apply(cfg: &mut Config, key: SettingKey, value: SettingValue) {
    if let (SettingKey::Preset, SettingValue::Preset(p)) = (key, value) {
        notty_config::apply_preset(&mut cfg.ui, p);
        return;
    }
    // Archivos/Atajo global (y lo que ningún preset fija: fuente, ligaduras,
    // resaltado) no son piezas de un preset: se resuelven aparte y no tocan
    // `cfg.ui.preset`.
    match (key, value) {
        (SettingKey::SyntaxHighlight, SettingValue::Bool(b)) => {
            cfg.ui.syntax_highlight = b;
            return;
        }
        (SettingKey::FontFamily, SettingValue::FontFamily(f)) => {
            cfg.ui.font_family = f;
            return;
        }
        (SettingKey::Ligatures, SettingValue::Bool(b)) => {
            cfg.ui.ligatures = b;
            return;
        }
        (SettingKey::TempMode, SettingValue::TempMode(m)) => {
            cfg.files.temp_mode = m;
            return;
        }
        (SettingKey::Autosave, SettingValue::Bool(b)) => {
            cfg.files.autosave = b;
            return;
        }
        (SettingKey::OnCloseUnsaved, SettingValue::OnCloseUnsaved(v)) => {
            cfg.files.on_close_unsaved = v;
            return;
        }
        (SettingKey::OpenInExistingWindow, SettingValue::Bool(b)) => {
            cfg.files.open_in_existing_window = b;
            return;
        }
        (SettingKey::HotkeyMechanism, SettingValue::HotkeyMechanism(m)) => {
            cfg.hotkey.mechanism = m;
            return;
        }
        (SettingKey::StartWithWindows, SettingValue::Bool(b)) => {
            cfg.hotkey.start_with_windows = b;
            return;
        }
        (SettingKey::UpdatesCheck, SettingValue::Bool(b)) => {
            cfg.updates.check = b;
            return;
        }
        (SettingKey::SuggestionIcons, SettingValue::Bool(b)) => {
            cfg.ui.suggestion_icons = b;
            return;
        }
        (SettingKey::NativeFileDialog, SettingValue::Bool(b)) => {
            cfg.ui.native_file_dialog = b;
            return;
        }
        (SettingKey::AccentColor, SettingValue::AccentColor(c)) => {
            cfg.ui.accent = c;
            return;
        }
        (SettingKey::ReopenPrevious, SettingValue::Bool(b)) => {
            cfg.ui.reopen_previous = b;
            return;
        }
        (SettingKey::TabIcons, SettingValue::Bool(b)) => {
            cfg.ui.tab_icons = b;
            return;
        }
        (SettingKey::Lang, SettingValue::Lang(l)) => {
            cfg.ui.lang = l;
            return;
        }
        (SettingKey::Wrap, SettingValue::Bool(b)) => {
            cfg.ui.wrap = b;
            return;
        }
        (SettingKey::SaveDir, SettingValue::SaveDir(choice)) => {
            cfg.files.default_save_dir = save_dir_path(choice).map(|p| p.display().to_string());
            return;
        }
        (SettingKey::MdOpenMode, SettingValue::MdOpenMode(m)) => {
            cfg.ui.md_open_mode = m;
            return;
        }
        (SettingKey::MdPreviewStyle, SettingValue::MdPreviewStyle(s)) => {
            cfg.ui.md_preview_style = s;
            return;
        }
        (SettingKey::AnimHz, SettingValue::AnimHz(h)) => {
            cfg.ui.anim_hz = h;
            return;
        }
        (SettingKey::ReducedMotion, SettingValue::Bool(b)) => {
            cfg.ui.reduced_motion = b;
            return;
        }
        (SettingKey::FollowEssentials, SettingValue::Bool(b)) => {
            cfg.ui.follow_essentials = b;
            return;
        }
        _ => {}
    }
    match (key, value) {
        (SettingKey::Theme, SettingValue::Theme(t)) => cfg.ui.theme = t,
        (SettingKey::LineNumbers, SettingValue::Bool(b)) => cfg.ui.line_numbers = b,
        (SettingKey::Files, SettingValue::Files(f)) => cfg.ui.files = f,
        (SettingKey::TabsPosition, SettingValue::TabsPosition(t)) => cfg.ui.tabs_position = t,
        (SettingKey::MenuBar, SettingValue::MenuBar(m)) => cfg.ui.menubar = m,
        (SettingKey::HintsBar, SettingValue::Bool(b)) => cfg.ui.hints_bar = b,
        (SettingKey::StatusBar, SettingValue::Bool(b)) => cfg.ui.status_bar = b,
        (SettingKey::MergedCommandLine, SettingValue::Bool(b)) => cfg.ui.merged_command_line = b,
        (SettingKey::VimAlways, SettingValue::Bool(b)) => cfg.ui.vim_always = b,
        _ => return, // combinación key/value que no tiene sentido: no hace nada
    }
    cfg.ui.preset = Preset::Custom;
}

/// Valor actual de un ajuste booleano (`false` si `key` no lo es).
pub fn current_bool(cfg: &Config, key: SettingKey) -> bool {
    match key {
        SettingKey::LineNumbers => cfg.ui.line_numbers,
        SettingKey::HintsBar => cfg.ui.hints_bar,
        SettingKey::StatusBar => cfg.ui.status_bar,
        SettingKey::MergedCommandLine => cfg.ui.merged_command_line,
        SettingKey::VimAlways => cfg.ui.vim_always,
        SettingKey::Autosave => cfg.files.autosave,
        SettingKey::OpenInExistingWindow => cfg.files.open_in_existing_window,
        SettingKey::SuggestionIcons => cfg.ui.suggestion_icons,
        SettingKey::NativeFileDialog => cfg.ui.native_file_dialog,
        SettingKey::Ligatures => cfg.ui.ligatures,
        SettingKey::SyntaxHighlight => cfg.ui.syntax_highlight,
        SettingKey::ReopenPrevious => cfg.ui.reopen_previous,
        SettingKey::TabIcons => cfg.ui.tab_icons,
        SettingKey::StartWithWindows => cfg.hotkey.start_with_windows,
        SettingKey::UpdatesCheck => cfg.updates.check,
        SettingKey::Wrap => cfg.ui.wrap,
        SettingKey::ReducedMotion => cfg.ui.reduced_motion,
        SettingKey::FollowEssentials => cfg.ui.follow_essentials,
        _ => false,
    }
}

/// Qué opción de `options` está elegida ahora (`None` si ninguna, p.ej. el preset
/// `Custom`).
pub fn selected_index(cfg: &Config, key: SettingKey, options: Options) -> Option<usize> {
    let current: SettingValue = match key {
        SettingKey::Preset => SettingValue::Preset(cfg.ui.preset),
        SettingKey::Theme => SettingValue::Theme(crate::shared_theme::effective(&cfg.ui).0),
        SettingKey::Files => SettingValue::Files(cfg.ui.files),
        SettingKey::TabsPosition => SettingValue::TabsPosition(cfg.ui.tabs_position),
        SettingKey::MenuBar => SettingValue::MenuBar(cfg.ui.menubar),
        SettingKey::TempMode => SettingValue::TempMode(cfg.files.temp_mode),
        SettingKey::OnCloseUnsaved => SettingValue::OnCloseUnsaved(cfg.files.on_close_unsaved),
        SettingKey::HotkeyMechanism => SettingValue::HotkeyMechanism(cfg.hotkey.mechanism),
        SettingKey::FontFamily => SettingValue::FontFamily(cfg.ui.font_family),
        SettingKey::AccentColor => SettingValue::AccentColor(crate::shared_theme::effective(&cfg.ui).1),
        SettingKey::Lang => SettingValue::Lang(cfg.ui.lang),
        SettingKey::SaveDir => {
            let current = cfg.files.default_save_dir.as_deref();
            let choice = [SaveDirChoice::UserFolder, SaveDirChoice::Desktop, SaveDirChoice::Documents, SaveDirChoice::Downloads]
                .into_iter()
                .find(|&c| match (c, current) {
                    (SaveDirChoice::UserFolder, None) => true,
                    (SaveDirChoice::UserFolder, Some(_)) => false,
                    (c, Some(cur)) => save_dir_path(c).is_some_and(|p| p.display().to_string() == cur),
                    (_, None) => false,
                })?;
            SettingValue::SaveDir(choice)
        }
        SettingKey::MdOpenMode => SettingValue::MdOpenMode(cfg.ui.md_open_mode),
        SettingKey::MdPreviewStyle => SettingValue::MdPreviewStyle(cfg.ui.md_preview_style),
        SettingKey::AnimHz => SettingValue::AnimHz(cfg.ui.anim_hz),
        _ => return None,
    };
    options.iter().position(|(_, v)| *v == current)
}

/// Las opciones de cada selector de `key`.
pub fn options_for(key: SettingKey) -> Options {
    match key {
        SettingKey::Preset => PRESET_OPTS,
        SettingKey::Theme => THEME_OPTS,
        SettingKey::Files => FILES_OPTS,
        SettingKey::TabsPosition => TABS_POSITION_OPTS,
        SettingKey::MenuBar => MENU_BAR_OPTS,
        SettingKey::TempMode => TEMP_MODE_OPTS,
        SettingKey::OnCloseUnsaved => ON_CLOSE_OPTS,
        SettingKey::HotkeyMechanism => HOTKEY_OPTS,
        SettingKey::AccentColor => ACCENT_OPTS,
        SettingKey::Lang => LANG_OPTS,
        SettingKey::SaveDir => SAVE_DIR_OPTS,
        SettingKey::MdOpenMode => MD_OPEN_MODE_OPTS,
        SettingKey::MdPreviewStyle => MD_PREVIEW_STYLE_OPTS,
        SettingKey::AnimHz => ANIM_HZ_OPTS,
        _ => &[],
    }
}

/// "Última comprobación: hace 3 horas" a partir de dos instantes Unix (segundos).
pub fn relative_time(now: u64, then: u64, lang: notty_config::Lang) -> String {
    if then == 0 {
        return crate::strings::tr(lang, "nunca").to_string();
    }
    let en = lang == notty_config::Lang::En;
    let d = now.saturating_sub(then);
    let plural = |n: u64, one_es: &str, many_es: &str, one_en: &str, many_en: &str| {
        if en {
            if n == 1 { format!("{n} {one_en} ago") } else { format!("{n} {many_en} ago") }
        } else if n == 1 {
            format!("hace 1 {one_es}")
        } else {
            format!("hace {n} {many_es}")
        }
    };
    match d {
        0..60 => crate::strings::tr(lang, "ahora mismo").to_string(),
        60..3600 => plural(d / 60, "minuto", "minutos", "minute", "minutes"),
        3600..86400 => plural(d / 3600, "hora", "horas", "hour", "hours"),
        _ => plural(d / 86400, "día", "días", "day", "days"),
    }
}

/// Viñetas de las notas de una release (el `body` de GitHub, en Markdown): las
/// líneas de lista sin su marcador, sin encabezados ni líneas vacías.
/// Las notas de una release vienen del CHANGELOG, en español. Una versión puede traer
/// también su versión en inglés debajo de un encabezado `English` (p.ej. `#### English`):
/// en inglés se usa ese bloque (hasta el siguiente `## `), y en español lo que va antes.
/// Sin ese encabezado, las mismas notas para los dos idiomas.
pub fn release_bullets(body: &str, max: usize, lang: Lang) -> Vec<String> {
    let is_marker = |l: &str| l.starts_with('#') && l.trim_start_matches('#').trim().eq_ignore_ascii_case("english");
    let lines: Vec<&str> = body.lines().map(str::trim).collect();
    // Solo cuenta el encabezado de la primera versión, no el de una anterior.
    let first = lines.iter().position(|l| l.starts_with("## ")).map_or(0, |i| i + 1);
    let section_end = lines[first..].iter().position(|l| l.starts_with("## ")).map_or(lines.len(), |i| first + i);
    let lines: &[&str] = match lines[..section_end].iter().position(|l| is_marker(l)) {
        Some(m) if lang == Lang::En => {
            let rest = &lines[m + 1..];
            &rest[..rest.iter().position(|l| l.starts_with("## ")).unwrap_or(rest.len())]
        }
        Some(m) => &lines[..m],
        None => &lines,
    };
    lines
        .iter()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.trim_start_matches(['-', '*', '+']).trim().replace("**", "").replace('`', ""))
        .filter(|l| !l.is_empty())
        .take(max)
        .collect()
}

/// Enciende o apaga el resaltado de un lenguaje (`syntax::LangInfo::id`).
pub fn toggle_syntax_lang(cfg: &mut Config, id: &str) {
    if let Some(i) = cfg.syntax_disabled.iter().position(|d| d == id) {
        cfg.syntax_disabled.remove(i);
    } else {
        cfg.syntax_disabled.push(id.to_string());
    }
}

/// "Activar todos" / "Desactivar todos" en Ajustes → Sintaxis.
pub fn set_all_syntax_langs(cfg: &mut Config, on: bool) {
    cfg.syntax_disabled = if on { Vec::new() } else { crate::syntax::LANGS.iter().map(|l| l.id.to_string()).collect() };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rail_has_updates_between_global_hotkey_and_about() {
        let names: Vec<&str> = Page::RAIL.iter().map(|p| p.name()).collect();
        assert_eq!(
            names,
            ["Apariencia", "Ventana", "Teclado", "Archivos", "Atajo global", "Actualizaciones", "Acerca de", "Ayuda"]
        );
    }

    #[test]
    fn subpages_belong_to_apariencia() {
        assert_eq!(Page::Fuentes.parent(), Some(Page::Apariencia));
        assert_eq!(Page::Ligaduras.rail_page(), Page::Apariencia);
        assert_eq!(Page::Ligaduras.rail_index(), 0);
        assert_eq!(Page::Ventana.parent(), None);
        assert_eq!(Page::Actualizaciones.rail_index(), 5);
    }

    #[test]
    fn page_ids_round_trip() {
        for p in Page::ALL {
            assert_eq!(Page::from_id(p.id()), Some(p));
        }
        assert_eq!(Page::from_id("teclado"), Some(Page::Teclado));
        assert_eq!(Page::from_id("nada"), None);
    }

    #[test]
    fn teclado_lists_every_remappable_command() {
        for cmd in Command::ALL {
            assert!(BINDINGS.contains(cmd), "{cmd:?}");
        }
    }

    #[test]
    fn varios_archivos_offers_paneles_with_a_description_each() {
        assert!(FILES_OPTS.iter().any(|(_, v)| *v == SettingValue::Files(Files::Splits)));
        assert_eq!(FILES_OPTS.len(), FILES_DESC.len());
    }

    #[test]
    fn applying_updates_check_touches_updates_not_ui() {
        let mut cfg = Config::default();
        let preset_before = cfg.ui.preset;
        apply(&mut cfg, SettingKey::UpdatesCheck, SettingValue::Bool(true));
        assert!(cfg.updates.check);
        assert_eq!(cfg.ui.preset, preset_before);
    }

    #[test]
    fn toggling_syntax_highlight_font_or_ligatures_keeps_the_preset() {
        let mut cfg = Config::default();
        let preset_before = cfg.ui.preset;
        apply(&mut cfg, SettingKey::SyntaxHighlight, SettingValue::Bool(false));
        apply(&mut cfg, SettingKey::Ligatures, SettingValue::Bool(true));
        apply(&mut cfg, SettingKey::FontFamily, SettingValue::FontFamily(FontFamily::named("Consolas")));
        assert!(!cfg.ui.syntax_highlight);
        assert!(cfg.ui.ligatures);
        assert_eq!(cfg.ui.font_family.primary_name(), "Consolas");
        assert_eq!(cfg.ui.preset, preset_before);
    }

    #[test]
    fn applying_a_preset_sets_its_fixed_pieces_and_keeps_the_preset_itself() {
        let mut cfg = Config::default();
        apply(&mut cfg, SettingKey::Preset, SettingValue::Preset(Preset::Zen));
        assert_eq!(cfg.ui.preset, Preset::Zen);
        assert!(!cfg.ui.line_numbers);
        assert!(cfg.ui.merged_command_line);
    }

    #[test]
    fn touching_a_loose_piece_marks_the_preset_as_custom() {
        let mut cfg = Config::default();
        assert_eq!(cfg.ui.preset, Preset::Moderna);
        apply(&mut cfg, SettingKey::TabsPosition, SettingValue::TabsPosition(TabsPosition::Below));
        assert_eq!(cfg.ui.tabs_position, TabsPosition::Below);
        assert_eq!(cfg.ui.preset, Preset::Custom);
        assert_eq!(selected_index(&cfg, SettingKey::Preset, PRESET_OPTS), None);
    }

    #[test]
    fn toggle_applies_the_boolean_and_marks_custom() {
        let mut cfg = Config::default();
        apply(&mut cfg, SettingKey::HintsBar, SettingValue::Bool(false));
        assert!(!cfg.ui.hints_bar);
        assert!(!current_bool(&cfg, SettingKey::HintsBar));
        assert_eq!(cfg.ui.preset, Preset::Custom);
    }

    #[test]
    fn mismatched_key_and_value_does_nothing() {
        let mut cfg = Config::default();
        let before = cfg.clone();
        apply(&mut cfg, SettingKey::Theme, SettingValue::Bool(true));
        assert_eq!(cfg, before);
    }

    #[test]
    fn on_close_unsaved_is_a_files_setting() {
        let mut cfg = Config::default();
        assert_eq!(selected_index(&cfg, SettingKey::OnCloseUnsaved, ON_CLOSE_OPTS), Some(0));
        apply(&mut cfg, SettingKey::OnCloseUnsaved, ON_CLOSE_OPTS[1].1);
        assert_eq!(cfg.files.on_close_unsaved, OnCloseUnsaved::Recuperar);
        assert_eq!(cfg.ui.preset, Config::default().ui.preset);
    }

    #[test]
    fn open_in_existing_window_toggles() {
        let mut cfg = Config::default();
        assert!(current_bool(&cfg, SettingKey::OpenInExistingWindow));
        apply(&mut cfg, SettingKey::OpenInExistingWindow, SettingValue::Bool(false));
        assert!(!cfg.files.open_in_existing_window);
    }

    #[test]
    fn applying_temp_mode_and_hotkey_touch_their_section_only() {
        let mut cfg = Config::default();
        let preset_before = cfg.ui.preset;
        apply(&mut cfg, SettingKey::TempMode, SettingValue::TempMode(TempMode::Volatile));
        apply(&mut cfg, SettingKey::Autosave, SettingValue::Bool(true));
        apply(&mut cfg, SettingKey::HotkeyMechanism, SettingValue::HotkeyMechanism(HotkeyMechanism::Lnk));
        apply(&mut cfg, SettingKey::StartWithWindows, SettingValue::Bool(false));
        assert_eq!(cfg.files.temp_mode, TempMode::Volatile);
        assert!(cfg.files.autosave);
        assert_eq!(cfg.hotkey.mechanism, HotkeyMechanism::Lnk);
        assert!(!cfg.hotkey.start_with_windows);
        assert_eq!(cfg.ui.preset, preset_before);
        assert_eq!(selected_index(&cfg, SettingKey::HotkeyMechanism, HOTKEY_OPTS), Some(1));
    }

    #[test]
    fn font_px_maps_onto_font_scale() {
        let mut cfg = Config::default();
        assert_eq!(font_px(&cfg), 13);
        set_font_px(&mut cfg, 20);
        assert_eq!(font_px(&cfg), 20);
        set_font_px(&mut cfg, 99);
        assert_eq!(font_px(&cfg), FONT_PX_MAX);
    }

    #[test]
    fn ligatures_can_be_added_toggled_and_removed() {
        let mut cfg = Config::default();
        assert_eq!(add_ligature(&mut cfg, "|>", "▷x"), Ok(()));
        assert_eq!(cfg.ligature_overrides.get("|>").map(String::as_str), Some("▷"));
        toggle_ligature(&mut cfg, "|>");
        assert_eq!(cfg.ligature_disabled, vec!["|>".to_string()]);
        toggle_ligature(&mut cfg, "|>");
        assert!(cfg.ligature_disabled.is_empty());
        toggle_ligature(&mut cfg, "->");
        remove_ligature(&mut cfg, "|>");
        assert!(cfg.ligature_overrides.is_empty());
        assert_eq!(cfg.ligature_disabled, vec!["->".to_string()]);
    }

    #[test]
    fn relative_times_in_spanish() {
        assert_eq!(relative_time(1000, 0, Lang::Es), "nunca");
        assert_eq!(relative_time(1000, 990, Lang::Es), "ahora mismo");
        assert_eq!(relative_time(10_000, 10_000 - 120, Lang::Es), "hace 2 minutos");
        assert_eq!(relative_time(10_000, 10_000 - 3600, Lang::Es), "hace 1 hora");
        assert_eq!(relative_time(1_000_000, 1_000_000 - 3 * 86400, Lang::Es), "hace 3 días");
        assert_eq!(relative_time(10_000, 10_000 - 120, Lang::En), "2 minutes ago");
    }

    #[test]
    fn release_bullets_strip_markdown() {
        let b = release_bullets("## Novedades

- Paneles lado a lado
* **Ligaduras** en el editor
Arreglos varios
", 5, Lang::Es);
        assert_eq!(b, vec!["Paneles lado a lado", "Ligaduras en el editor", "Arreglos varios"]);
        assert_eq!(release_bullets("- a
- b
- c", 2, Lang::Es).len(), 2);
    }

    #[test]
    fn release_bullets_pick_the_english_block() {
        let body = "# Changelog

## v2.0.0

- Hola

#### English

- Hello

## v1.0.0

- Viejo
";
        assert_eq!(release_bullets(body, 5, Lang::Es), vec!["Hola"]);
        assert_eq!(release_bullets(body, 5, Lang::En), vec!["Hello"]);
        assert_eq!(release_bullets("- Solo
", 5, Lang::En), vec!["Solo"], "sin bloque en inglés, el español");
        let old_en = "## v3.0.0\n- Nuevo\n## v2.0.0\n- Hola\n#### English\n- Hello\n";
        assert_eq!(release_bullets(old_en, 1, Lang::En), vec!["Nuevo"], "el inglés de una versión anterior no cuenta");
        assert_eq!(crate::strings::tr_msg(Lang::En, "sin red o error HTTP: 12029"), "no connection or HTTP error: 12029");
    }

    #[test]
    fn bad_ligatures_are_rejected() {
        assert_eq!(can_add_ligature("", "x"), Err(AddLigatureError::Empty));
        assert_eq!(can_add_ligature("|>", " "), Err(AddLigatureError::Empty));
        assert_eq!(can_add_ligature("a", "x"), Err(AddLigatureError::TooShort));
        assert_eq!(can_add_ligature("a b", "x"), Err(AddLigatureError::Whitespace));
        assert_eq!(can_add_ligature("~>", "↝"), Ok(()));
    }

    #[test]
    fn syntax_langs_toggle_and_bulk() {
        let mut cfg = Config::default();
        toggle_syntax_lang(&mut cfg, "go");
        assert_eq!(cfg.syntax_disabled, vec!["go".to_string()]);
        toggle_syntax_lang(&mut cfg, "go");
        assert!(cfg.syntax_disabled.is_empty());
        set_all_syntax_langs(&mut cfg, false);
        assert_eq!(cfg.syntax_disabled.len(), crate::syntax::LANGS.len());
        set_all_syntax_langs(&mut cfg, true);
        assert!(cfg.syntax_disabled.is_empty());
        assert_eq!(Page::Sintaxis.parent(), Some(Page::Apariencia));
        assert_eq!(Page::from_id("sintaxis"), Some(Page::Sintaxis));
    }
}
