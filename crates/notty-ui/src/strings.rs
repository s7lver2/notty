//! Traducción de la interfaz (Ajustes → Idioma): español siempre disponible sin
//! tocar nada (es el texto que ya escribe cada función de dibujo), inglés como
//! tabla de búsqueda sobre ese mismo texto. `tr(lang, "Nuevo")` devuelve "New" si
//! `lang` es `En` y "Nuevo" está en la tabla; si no está (todavía no traducido, o
//! `lang` es `Es`), devuelve el español tal cual — nunca un hueco en blanco.
//!
//! No es la única forma de hacerlo (un `struct` con un campo por texto lo
//! comprobaría en tiempo de compilación), pero sobre ~800 cadenas ya escritas en
//! docenas de funciones de dibujo, envolver cada una en `tr(lang, "...")` es el
//! cambio más pequeño que no obliga a rehacer cómo están escritas.

use notty_config::Lang;

/// (español, inglés). Buscar es O(n) sobre esta tabla, pero se llama pintando la
/// interfaz (decenas de veces por fotograma como mucho, nunca en un bucle sobre el
/// texto del documento), así que no hace falta nada más rápido.
const TABLE: &[(&str, &str)] = &[
    // --- menu.rs: barra de menús ---
    ("Archivo", "File"),
    ("Editar", "Edit"),
    ("Buscar", "Search"),
    ("Ver", "View"),
    ("Ayuda", "Help"),
    ("Nuevo", "New"),
    ("Nuevo temporal", "New scratch file"),
    ("Abrir", "Open"),
    ("Guardar", "Save"),
    ("Guardar como", "Save As"),
    ("Ajustes", "Settings"),
    ("Cerrar pestaña", "Close Tab"),
    ("Deshacer", "Undo"),
    ("Rehacer", "Redo"),
    ("Reemplazar", "Replace"),
    ("Siguiente", "Next"),
    ("Anterior", "Previous"),
    ("Números de línea", "Line Numbers"),
    ("Barra de atajos", "Hints Bar"),
    ("Modo vim", "Vim Mode"),
    ("Ver como raw", "View as Raw"),
    ("Atajos de teclado", "Keyboard Shortcuts"),
    ("Acerca de notty", "About notty"),
    // --- settings_model.rs: Page::name ---
    ("Apariencia", "Appearance"),
    ("Fuentes", "Fonts"),
    ("Ligaduras", "Ligatures"),
    ("Sintaxis", "Syntax"),
    ("Idioma", "Language"),
    ("Ventana", "Window"),
    ("Teclado", "Keyboard"),
    ("Archivos", "Files"),
    ("Atajo global", "Global Shortcut"),
    ("Actualizaciones", "Updates"),
    ("Acerca de", "About"),
    // "Ayuda" ya está arriba (menú Ayuda / página Ayuda comparten texto).
    // --- settings_pages.rs: cabeceras y grupos de cada página ---
    ("Los cambios se ven al instante", "Changes take effect instantly"),
    ("Personalizado · los cambios se ven al instante", "Custom · changes take effect instantly"),
    ("Preset", "Preset"),
    ("Tema", "Theme"),
    ("Color de acento", "Accent color"),
    ("Sistema", "System"),
    ("Claro", "Light"),
    ("Oscuro", "Dark"),
    ("Moderna", "Modern"),
    ("Clásica", "Classic"),
    ("Zen", "Zen"),
    ("Fuente", "Font"),
    ("Desactivadas", "Off"),
    ("Resaltado de sintaxis", "Syntax highlighting"),
    ("Así queda la ventana principal", "This is how the main window looks"),
    ("Varios archivos", "Multiple files"),
    ("Pestañas", "Tabs"),
    ("Buffers", "Buffers"),
    ("Paneles", "Panes"),
    ("Posición de las pestañas", "Tab position"),
    ("Iconos en las pestañas", "Tab icons"),
    ("Un icono de archivo genérico delante del nombre.", "A generic file icon before the name."),
    ("En el título", "In the title bar"),
    ("Bajo el menú", "Below the menu"),
    ("Si hay más de 1", "If there's more than 1"),
    ("Ocultas", "Hidden"),
    ("Barra de menús", "Menu bar"),
    ("Visible", "Visible"),
    ("Con Alt", "With Alt"),
    ("Estilo nano. Cambia según lo que estés haciendo.", "Nano-style. Changes with what you're doing."),
    ("Barra de estado", "Status bar"),
    ("Si la ocultas, reaparece para rutas, búsquedas y avisos.", "If you hide it, it reappears for paths, searches and notices."),
    ("Línea de comandos fusionada", "Merged command line"),
    ("Una sola línea abajo para estado y comandos, como en Zen.", "A single line at the bottom for status and commands, like Zen."),
    ("Así se abre y guarda", "This is how files open and save"),
    ("Archivos temporales", "Scratch files"),
    ("Borrador", "Draft"),
    ("Se guarda solo; se borra al cerrar si no le das ruta.", "Saves itself; deleted on close if you never give it a path."),
    ("Volátil", "Volatile"),
    ("Nunca toca el disco. Al cerrar, desaparece.", "Never touches disk. Gone when you close it."),
    ("Diálogo de Windows", "Windows dialog"),
    ("Selector nativo de Windows", "Native Windows picker"),
    ("Abrir y Guardar como usan el diálogo de Windows en vez de la línea de ruta.", "Open and Save As use the Windows dialog instead of the path line."),
    ("Iconos en las sugerencias", "Icons in suggestions"),
    ("Carpeta o archivo delante de cada sugerencia al escribir una ruta.", "Folder or file before each suggestion while typing a path."),
    ("Autoguardado", "Autosave"),
    ("Guarda sola tras dejar de escribir. Solo si el archivo ya tiene ruta.", "Saves itself after you stop typing. Only if the file already has a path."),
    ("Abrir archivos en la ventana ya abierta", "Open files in the already-open window"),
    ("Si lo apagas, cada archivo que abras desde el Explorador sale en una ventana nueva.", "If you turn this off, every file you open from Explorer opens in a new window."),
    ("Reabrir archivos anteriores", "Reopen previous files"),
    ("Al abrir notty sin darle ningún archivo, vuelve a abrir los que quedaron abiertos la última vez.", "Opening notty with no file reopens whatever was still open last time."),
    ("Al cerrar con cambios sin guardar", "When closing with unsaved changes"),
    ("Preguntar", "Ask"),
    ("Recuperar al abrir", "Recover on open"),
    ("Auto", "Auto"),
    ("Español", "Spanish"),
    ("English", "English"),
    ("De Windows", "From Windows"),
    ("Palabra clave", "Keyword"),
    ("Cadena", "String"),
    ("Número", "Number"),
    ("Tipo", "Type"),
    ("Función", "Function"),
    ("Comentario", "Comment"),
    ("El idioma de la interfaz. \"Auto\" sigue el de Windows.", "The interface language. \"Auto\" follows Windows' own language."),
    // --- settings_pages.rs: Teclado ---
    ("Atajos · haz clic en uno para cambiarlo", "Shortcuts · click one to change it"),
    ("Para moverte · fijos", "To move around · fixed"),
    ("Atajos globales · funcionan aunque notty no tenga el foco", "Global shortcuts · work even without notty focused"),
    ("Pestaña siguiente / anterior", "Next / previous tab"),
    ("Ir a la pestaña 1…9 (9 = la última)", "Go to tab 1…9 (9 = the last one)"),
    ("Ir al panel 1…9 (modo Paneles)", "Go to pane 1…9 (Panes mode)"),
    ("En Acomodar: ancho · panel · igualar · salir", "In Arrange: width · pane · equalize · exit"),
    ("Nuevo permanente", "New permanent file"),
    ("Todos los atajos", "All shortcuts"),
    ("Cada acción es un comando con nombre; también en config.toml, sección [keys].", "Every action is a named command; also in config.toml, section [keys]."),
    ("Abrir [keys]", "Open [keys]"),
    ("Modo vim siempre", "Always vim mode"),
    ("Cada ventana arranca en modo vim. ", "Every window starts in vim mode. "),
    (" para insertar, ", " to insert, "),
    (" para salir.", " to exit."),
    ("i para insertar, Esc para salir.", "i to insert, Esc to exit."),
    ("sin atajo", "no shortcut"),
    ("Abrir Ajustes", "Open Settings"),
    ("Nueva pestaña", "New tab"),
    ("Nueva pestaña temporal", "New scratch tab"),
    ("Pestaña siguiente", "Next tab"),
    ("Pestaña anterior", "Previous tab"),
    ("Alternar vim en esta ventana", "Toggle vim in this window"),
    ("Aumentar tamaño del texto", "Increase text size"),
    ("Reducir tamaño del texto", "Decrease text size"),
    ("Restablecer tamaño del texto", "Reset text size"),
    ("Dividir panel", "Split pane"),
    ("Cerrar panel", "Close pane"),
    ("Panel de la izquierda", "Pane to the left"),
    ("Panel de la derecha", "Pane to the right"),
    ("Mover el panel a la izquierda", "Move pane left"),
    ("Mover el panel a la derecha", "Move pane right"),
    ("Acomodar paneles (ajustar anchos)", "Arrange panes (adjust widths)"),
    ("Mover la pestaña a la izquierda", "Move tab left"),
    ("Mover la pestaña a la derecha", "Move tab right"),
    // --- settings_pages.rs: Atajo global ---
    ("Cómo se escucha el atajo", "How the shortcut is heard"),
    ("Segundo plano", "Background"),
    ("~1 MB de RAM · cualquier combinación · instantáneo", "~1 MB of RAM · any combination · instant"),
    ("Acceso directo", "Shortcut file"),
    ("Nada residente · solo Ctrl+Alt+letra", "Nothing resident · only Ctrl+Alt+letter"),
    ("Iniciar con Windows", "Start with Windows"),
    ("sin título", "untitled"),
    // --- settings_pages.rs: Actualizaciones ---
    ("notty está al día", "notty is up to date"),
    ("Tienes la última versión publicada.", "You have the latest published version."),
    ("Sin comprobar en esta sesión", "Not checked this session"),
    ("Busca cuando quieras: se consultan las releases de GitHub.", "Check whenever you like: this queries GitHub Releases."),
    ("Buscando…", "Checking…"),
    ("Consultando las releases de GitHub.", "Querying GitHub Releases."),
    ("Hay una versión nueva", "There's a new version"),
    ("Descargando", "Downloading"),
    ("Se verifica la firma al terminar.", "The signature is verified once it's done."),
    ("No se pudo actualizar", "Couldn't update"),
    ("No se pudo comprobar", "Couldn't check"),
    ("Descargar e instalar", "Download and install"),
    ("Buscar actualizaciones", "Check for updates"),
    ("Novedades de", "What's new in"),
    ("Esta versión no trae notas.", "This version has no release notes."),
    ("Ver todas las notas en GitHub", "See all notes on GitHub"),
    ("Buscar al abrir notty", "Check when notty opens"),
    ("Una vez al día contra GitHub Releases. Nunca se activa solo.", "Once a day against GitHub Releases. Never turns itself on."),
    ("Cada instalador se comprueba con Ed25519 antes de ejecutarse.", "Every installer is verified with Ed25519 before it runs."),
    ("Firma verificada", "Signature verified"),
    ("Siempre", "Always"),
    ("Versión instalada ", "Installed version "),
    ("Última comprobación", "Last checked"),
    // --- settings_pages.rs: Acerca de ---
    ("El Bloc de notas, pero rápido y con teclado.", "Notepad, but fast and keyboard-driven."),
    ("Buscando actualizaciones…", "Checking for updates…"),
    ("Al día", "Up to date"),
    ("Información", "Information"),
    ("Versión", "Version"),
    ("Plataforma", "Platform"),
    ("Configuración", "Config"),
    ("Instalado en", "Installed in"),
    ("Código fuente", "Source code"),
    ("Informar de un problema", "Report a problem"),
    ("Abre una incidencia en GitHub", "Opens an issue on GitHub"),
    ("Novedades", "What's new"),
    ("Historial de cambios de cada versión", "Changelog of every version"),
    ("Carpeta de configuración", "Config folder"),
    ("Hecho con Rust, Direct2D, DirectWrite y tree-sitter.", "Made with Rust, Direct2D, DirectWrite and tree-sitter."),
    // --- settings_pages.rs: Ayuda ---
    ("El recorrido guiado sobre la ventana principal, paso a paso.", "The guided tour of the main window, step by step."),
    ("Repetir tutorial", "Repeat tutorial"),
    ("Lo básico", "The basics"),
    ("Abrir archivo", "Open file"),
    ("Tamaño del texto", "Text size"),
    // --- settings_pages.rs: Fuentes ---
    ("Tamaño", "Size"),
    ("Texto de muestra", "Sample text"),
    ("Buscar fuente", "Search font"),
    ("Buscando fuentes…", "Looking for fonts…"),
    ("Ninguna fuente monoespaciada coincide con la búsqueda.", "No monospace font matches the search."),
    // --- settings_pages.rs: Ligaduras ---
    ("Activadas", "On"),
    ("Tal como lo escribes", "As you type it"),
    ("Tal como lo ves", "As you see it"),
    ("Añadir ligadura", "Add ligature"),
    ("Se guarda en config.toml", "Saved to config.toml"),
    ("De serie", "Built-in"),
    ("Propia", "Custom"),
    // --- settings_pages.rs: Sintaxis ---
    ("Activado", "On"),
    ("Desactivar todos", "Turn all off"),
    ("Activar todos", "Turn all on"),
    ("Desactivado", "Off"),
    // --- tour.rs / welcome_window.rs: bienvenida y recorrido ---
    ("Atrás", "Back"),
    ("Empezar", "Get started"),
    ("Saltar", "Skip"),
    ("Terminar", "Finish"),
];

/// Traduce `es` a `lang`. Si `lang` es `Es`, o no hay traducción en la tabla,
/// devuelve `es` sin tocar (por eso acepta cualquier `&str`, no solo `&'static`:
/// también hay que poder traducir texto generado en tiempo de dibujo, como "5 de 21
/// lenguajes activos", que se queda tal cual porque no está en la tabla).
pub fn tr<'a>(lang: Lang, es: &'a str) -> &'a str {
    if lang != Lang::En {
        return es;
    }
    TABLE.iter().find(|(s, _)| *s == es).map(|(_, e)| *e).unwrap_or(es)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_is_always_the_literal_text() {
        assert_eq!(tr(Lang::Es, "Archivo"), "Archivo");
        assert_eq!(tr(Lang::Es, "algo sin traducir"), "algo sin traducir");
    }

    #[test]
    fn english_translates_known_strings() {
        assert_eq!(tr(Lang::En, "Archivo"), "File");
        assert_eq!(tr(Lang::En, "Guardar como"), "Save As");
    }

    #[test]
    fn english_falls_back_to_spanish_for_unknown_strings() {
        assert_eq!(tr(Lang::En, "algo sin traducir"), "algo sin traducir");
    }

    #[test]
    fn table_has_no_duplicate_spanish_keys() {
        let mut seen = std::collections::HashSet::new();
        for (es, _) in TABLE {
            assert!(seen.insert(*es), "clave repetida en la tabla: {es}");
        }
    }
}
