# notty · Plan 5b: La app, idéntica a la maqueta

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Que `notty.exe` se vea **exactamente** como `docs/mockups/notty-ui.html`: misma barra de título propia con las pestañas dentro, mismas fuentes y tamaños, mismos colores (tema oscuro y claro), mismas franjas (menú, pestañas, atajos, estado), mismos prompts (ruta con lista de sugerencias, buscar/reemplazar con opciones), misma vista raw y misma ventana de Ajustes. La maqueta es la fuente de verdad visual; si este plan y la maqueta discrepan, gana la maqueta.

**Por qué ahora se ve tan distinto (diagnóstico hecho comparando capturas reales):**

| Maqueta | App real hoy |
|---|---|
| Barra de título propia de 34 px, color `--chrome`, icono de la app, pestañas **dentro** de la barra, botones min/max/cerrar dibujados en el tema | Barra de título nativa de Windows con la ruta completa; las pestañas van en una franja aparte debajo |
| Interfaz en Segoe UI Variable Text 12 px; solo el documento en Cascadia Mono 13 px, interlineado 20 px | Todo, incluidas pestañas y barras, en Cascadia Mono 16 px |
| Pestaña activa del mismo color que el editor, inactivas en gris `--text-2`, con ✕ y botón `+` | Pestaña activa rellena con el azul de la **selección de texto**, sin ✕ ni `+` |
| Jerarquía de color: documento `--text`, barras `--text-2`, números de línea `--text-3` (el de la línea actual en `--text`) | Todo del mismo blanco |
| Barra de atajos con fondo `--surface-2`, línea superior, teclas en negrita | Texto suelto sobre el fondo |
| Barra de estado de 24 px en `--chrome`, «Texto» a la izquierda, «Ln 1, Col 1   UTF-8   CRLF» a la derecha | «Texto · Ln 1, Col 1 · UTF-8 · LF» en mono 16 px, sin fondo |
| Canal de números de 52 px, texto a 4 px del canal y 10 px de margen superior | Números pegados al texto |
| Sin reescalado borroso (la ventana sigue los DPI) | La app no declara soporte de DPI: con escalado de Windows se ve borrosa |

**Architecture:** Tres piezas nuevas y puras en `notty-ui` (se pueden testear): `theme.rs` (paleta de la maqueta convertida de OKLCH a sRGB), `layout.rs` (medidas de cada franja y de cada pestaña en DIPs, sacadas del CSS de la maqueta) y `chrome_text.rs` (qué texto va en la barra de atajos y en la de estado según el contexto). Encima, se reescribe `render.rs` (dibuja con una sola brocha a la que se le cambia el color, varios formatos de texto, y apunta en una lista qué rectángulo es qué para el ratón), `window.rs` gana barra de título propia (sin la nativa), DPI por monitor y seguimiento del ratón, y `settings_window.rs` pasa de controles nativos a dibujarse con Direct2D como en la maqueta.

**Tech Stack:** Rust stable (MSVC), `windows` 0.62.2 (Direct2D, DirectWrite, Dwm, HiDpi, WindowsAndMessaging, KeyboardAndMouse), `windows-numerics`. No se añaden dependencias.

---

## Material de referencia (léelo antes de empezar)

1. **La maqueta:** `docs/mockups/notty-ui.html`. El CSS (líneas 8-240) da todas las medidas; el JS de `renderChrome`/`renderHints`/`renderStatus`/`renderPrompt`/`renderHex`/`renderSettings` da los textos exactos.
2. **Capturas de referencia a tamaño real (920×600, escala 1):** `docs/mockups/ref/*.png`. Se regeneran con `pwsh tools/shot-mockup.ps1` (Chrome sin ventana + el modo `?shot=` de la maqueta). Hay una por escenario:
   `moderna-oscuro`, `moderna-claro`, `sucio`, `clasica`, `zen`, `clickme`, `ruta-sugerencias`, `ruta-nueva`, `buscar`, `reemplazar`, `vim-normal`, `vim-insert`, `raw`, `raw-solo-lectura`, `ajustes` (esta última es el escritorio completo, 1000×656, con la ventana de Ajustes encima).
   Notas al compararlas: Chrome dibuja Cascadia un poco más gruesa que DirectWrite; **no** pongas el texto en negrita para imitarlo. El recuadro blanco de 1 px alrededor del hex en `raw*.png` es el foco del navegador, no es diseño. La maqueta muestra una segunda pestaña `presupuesto.txt` porque arranca con dos archivos; la app, con uno.
3. **Captura de la app real:** `pwsh tools/shot-app.ps1 -Out <png> [-File <ruta>] [-Keys <teclas SendKeys>...]`. Abre `target/release/notty.exe`, le manda las teclas (`^` Ctrl, `+` Shift, `%` Alt, `{TAB}`, `{ESC}`...), captura la ventana y la cierra. Ejemplo: `pwsh tools/shot-app.ps1 -Out $env:TEMP\buscar.png -File docs\mockups\ref\notas.txt -Keys '^f','Juan'`. Después **lee el PNG con la herramienta Read** y compáralo con la referencia.
4. **Texto de prueba:** crea `docs/mockups/ref/notas.txt` con exactamente el contenido de la constante `NOTAS` de la maqueta (líneas 334-346), fin de línea CRLF, UTF-8 sin BOM. Úsalo en todas las capturas de la app.

## Global Constraints

- **La maqueta manda.** Cada medida de este plan sale de su CSS; si al comparar capturas algo no coincide, corrige hasta que coincida a ojo (misma posición ±1 px, mismo color, mismo tamaño de letra). No inventes elementos que la maqueta no tiene.
- **Todas las coordenadas en DIPs** (1 DIP = 1 px CSS de la maqueta). El render target se pone a los DPI de la ventana con `SetDpi`, así Direct2D escala solo; el ratón llega en píxeles físicos y se divide por `dpi / 96`.
- **Fuentes:** interfaz `Segoe UI Variable Text` (si `IDWriteFontCollection::FindFamilyName` no la encuentra, `Segoe UI`); monoespaciada `Cascadia Mono` (si no está, `Consolas`). Peso normal salvo donde se indique.
- **Colores:** solo los de `theme.rs`. Nada de constantes de color sueltas en `render.rs` ni en `settings_window.rs`.
- **Fuera de alcance** (no lo hagas aunque la maqueta lo muestre): el escritorio, barra de tareas y bandeja de la maqueta; el menú contextual de codificación (`encpop`) y el cambio de EOL al hacer clic en la barra de estado; los avisos temporales (`flash`); los temporales (pestaña en cursiva, «Temporal · borrador»), el prompt de conflicto y las secciones Archivos / Atajo global de Ajustes (llegan en el Plan 6); el icono de la app en la barra de tareas (necesita un recurso `.ico`, Plan 7); barras de desplazamiento.
- **No rompas nada que ya funciona:** los 204 tests actuales siguen pasando; teclado, vim, raw, prompts y guardado se comportan igual. Si cambias el texto de una función ya testeada (p. ej. `status_line`, `hints_text`), actualiza su test en el mismo commit.
- **Sobre el código Win32/Direct2D de este plan:** se da con los nombres de API reales, pero las firmas exactas de `windows` 0.62.2 pueden variar (envoltorios `Some(..)`, `BOOL` vs `bool`, `Option<*const T>`...). Ajusta la firma consultando el error del compilador o `cargo doc -p windows --open`, sin cambiar el comportamiento descrito.
- Un commit por tarea, con el mensaje indicado y la línea `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Marca los checkboxes de este archivo al completar cada paso.

---

### Task 1: Paleta de la maqueta (`theme.rs`)

**Files:**
- Create: `crates/notty-ui/src/theme.rs`
- Modify: `crates/notty-ui/src/lib.rs` (añadir `mod theme; pub use theme::{Palette, Rgba, palette, is_dark};`)

Los valores son la conversión exacta OKLCH→sRGB de los tokens de la maqueta (tono de acento 250), hecha con la fórmula de Björn Ottosson. No los «ajustes a ojo».

- [x] **Step 1: Escribe `theme.rs`**

```rust
//! Paleta de la maqueta (`docs/mockups/notty-ui.html`, bloques `:root[data-theme=...]`),
//! convertida de OKLCH a sRGB. Cada campo se llama como su variable CSS.

use notty_config::Theme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba(pub f32, pub f32, pub f32, pub f32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub chrome: Rgba,
    pub chrome_hi: Rgba,
    pub surface: Rgba,
    pub surface_2: Rgba,
    pub cmd: Rgba,
    pub hover: Rgba,
    pub press: Rgba,
    pub line: Rgba,
    pub text: Rgba,
    pub text_2: Rgba,
    pub text_3: Rgba,
    pub text_hint: Rgba,
    pub accent: Rgba,
    pub accent_soft: Rgba,
    pub on_accent: Rgba,
    pub danger: Rgba,
    pub warn: Rgba,
    pub ok: Rgba,
    pub mark: Rgba,
    pub mark_cur: Rgba,
    /// Fondo del botón cerrar al pasar el ratón (`.caption button.close:hover`).
    pub close_hover: Rgba,
    pub close_hover_fg: Rgba,
    /// Sombra de los desplegables (`--shadow`): color de la sombra difusa y del anillo de 1 px.
    pub shadow: Rgba,
    pub shadow_ring: Rgba,
}

pub const DARK: Palette = Palette {
    chrome: Rgba(0.1654, 0.1702, 0.1755, 1.0),
    chrome_hi: Rgba(0.1848, 0.1898, 0.1951, 1.0),
    surface: Rgba(0.1150, 0.1181, 0.1214, 1.0),
    surface_2: Rgba(0.1382, 0.1413, 0.1447, 1.0),
    cmd: Rgba(0.0880, 0.0910, 0.0942, 1.0),
    hover: Rgba(0.8644, 0.8708, 0.8777, 0.07),
    press: Rgba(0.8644, 0.8708, 0.8777, 0.12),
    line: Rgba(0.1947, 0.1997, 0.2050, 1.0),
    text: Rgba(0.8984, 0.9027, 0.9074, 1.0),
    text_2: Rgba(0.5914, 0.5974, 0.6038, 1.0),
    text_3: Rgba(0.3401, 0.3455, 0.3514, 1.0),
    text_hint: Rgba(0.8534, 0.8576, 0.8623, 1.0),
    accent: Rgba(0.4521, 0.7150, 0.9819, 1.0),
    accent_soft: Rgba(0.4521, 0.7150, 0.9819, 0.16),
    on_accent: Rgba(0.0561, 0.0706, 0.0859, 1.0),
    danger: Rgba(0.9471, 0.4447, 0.4008, 1.0),
    warn: Rgba(0.8946, 0.6737, 0.3482, 1.0),
    ok: Rgba(0.4496, 0.7659, 0.5214, 1.0),
    mark: Rgba(0.5174, 0.4408, 0.1236, 0.55),
    mark_cur: Rgba(0.7999, 0.4713, 0.0000, 0.8),
    close_hover: Rgba(0.8592, 0.1733, 0.1703, 1.0),
    close_hover_fg: Rgba(0.99, 0.99, 0.99, 1.0),
    shadow: Rgba(0.0, 0.0, 0.0, 0.6),
    shadow_ring: Rgba(0.40, 0.41, 0.42, 0.18),
};

pub const LIGHT: Palette = Palette {
    chrome: Rgba(0.9095, 0.9160, 0.9230, 1.0),
    chrome_hi: Rgba(0.9634, 0.9678, 0.9725, 1.0),
    surface: Rgba(0.9831, 0.9875, 0.9922, 1.0),
    surface_2: Rgba(0.9355, 0.9420, 0.9490, 1.0),
    cmd: Rgba(0.9634, 0.9678, 0.9725, 1.0),
    hover: Rgba(0.3839, 0.3894, 0.3954, 0.08),
    press: Rgba(0.3839, 0.3894, 0.3954, 0.14),
    line: Rgba(0.8388, 0.8452, 0.8521, 1.0),
    text: Rgba(0.0988, 0.1048, 0.1113, 1.0),
    text_2: Rgba(0.3277, 0.3349, 0.3427, 1.0),
    text_3: Rgba(0.5422, 0.5501, 0.5586, 1.0),
    text_hint: Rgba(0.1262, 0.1324, 0.1391, 1.0),
    accent: Rgba(0.0000, 0.4173, 0.7526, 1.0),
    accent_soft: Rgba(0.0000, 0.4173, 0.7526, 0.12),
    on_accent: Rgba(0.9813, 0.9878, 0.9949, 1.0),
    danger: Rgba(0.7729, 0.1718, 0.1628, 1.0),
    warn: Rgba(0.6673, 0.4153, 0.0000, 1.0),
    ok: Rgba(0.1584, 0.4845, 0.2576, 1.0),
    mark: Rgba(0.9501, 0.8414, 0.4235, 0.75),
    mark_cur: Rgba(0.9884, 0.6233, 0.1868, 0.85),
    close_hover: Rgba(0.8592, 0.1733, 0.1703, 1.0),
    close_hover_fg: Rgba(0.99, 0.99, 0.99, 1.0),
    shadow: Rgba(0.02, 0.03, 0.05, 0.28),
    shadow_ring: Rgba(0.02, 0.03, 0.05, 0.12),
};

/// `Theme::System` sigue al modo de Windows (`system_dark`); los otros dos lo fuerzan.
pub fn is_dark(theme: Theme, system_dark: bool) -> bool {
    match theme {
        Theme::System => system_dark,
        Theme::Dark => true,
        Theme::Light => false,
    }
}

pub fn palette(dark: bool) -> &'static Palette {
    if dark { &DARK } else { &LIGHT }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_theme_follows_windows() {
        assert!(is_dark(Theme::System, true));
        assert!(!is_dark(Theme::System, false));
    }

    #[test]
    fn explicit_theme_ignores_windows() {
        assert!(is_dark(Theme::Dark, false));
        assert!(!is_dark(Theme::Light, true));
    }

    #[test]
    fn dark_surface_is_darker_than_chrome() {
        // En la maqueta el editor (--surface) es más oscuro que la barra de título (--chrome).
        assert!(DARK.surface.0 < DARK.chrome.0);
        assert!(LIGHT.surface.0 > LIGHT.chrome.0);
    }

    #[test]
    fn text_hierarchy_is_ordered() {
        assert!(DARK.text.0 > DARK.text_2.0 && DARK.text_2.0 > DARK.text_3.0);
        assert!(LIGHT.text.0 < LIGHT.text_2.0 && LIGHT.text_2.0 < LIGHT.text_3.0);
    }
}
```

- [x] **Step 2:** `cargo test -p notty-ui theme` → 4 tests pasan.
- [x] **Step 3: Commit** `feat(ui): paleta de la maqueta en theme.rs`

---

### Task 2: Medidas de la maqueta (`layout.rs`)

**Files:**
- Create: `crates/notty-ui/src/layout.rs`
- Modify: `crates/notty-ui/src/lib.rs` (`pub mod layout;`)

Todas las constantes salen del CSS de la maqueta; al lado de cada una va el selector del que sale, para poder comprobarlo.

- [x] **Step 1: Escribe `layout.rs`**

```rust
//! Geometría de la ventana principal en DIPs (= px CSS de la maqueta).

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }
    pub fn width(&self) -> f32 {
        self.right - self.left
    }
    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
    pub fn is_empty(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }
}

pub const TITLEBAR_H: f32 = 34.0; // .titlebar{height:34px}
pub const APPICON_W: f32 = 36.0; // .appicon{width:36px}
pub const APPICON_SIZE: f32 = 16.0; // ICON() 16x16
pub const CAPTION_BTN_W: f32 = 46.0; // .caption button{width:46px}
pub const TAB_H: f32 = 28.0; // .tab{height:28px}
pub const TAB_GAP: f32 = 2.0; // .tabs{gap:2px}
pub const TAB_PAD_L: f32 = 12.0; // .tab{padding:0 6px 0 12px}
pub const TAB_PAD_R: f32 = 6.0;
pub const TAB_INNER_GAP: f32 = 6.0; // .tab{gap:6px}
pub const TAB_CLOSE: f32 = 18.0; // .tab .x{width:18px;height:18px}
pub const TAB_MAX_W: f32 = 190.0; // .tab{max-width:190px}
pub const TAB_RADIUS: f32 = 6.0; // border-radius:6px 6px 0 0
pub const PLUS_W: f32 = 28.0; // .tabs .plus{width:28px;height:24px;margin-left:2px;margin-top:2px}
pub const PLUS_H: f32 = 24.0;
pub const PLUS_ML: f32 = 2.0;
pub const MENUBAR_H: f32 = 30.0; // .menubar{padding:2px 6px} + button{padding:4px 10px} a 12.5px
pub const MENUBAR_PAD_X: f32 = 6.0;
pub const MENU_BTN_PAD_X: f32 = 10.0;
pub const MENU_BTN_GAP: f32 = 2.0;
pub const TABS_BELOW_H: f32 = 32.0; // .tabs.below{padding:4px 6px 0} + 28px
pub const TABS_BELOW_PAD_X: f32 = 6.0;
pub const HINTS_H: f32 = 22.0; // .hints{padding:4px 12px;border-top:1px} a 11px mono
pub const HINTS_PAD_X: f32 = 12.0;
pub const HINTS_GAP: f32 = 14.0; // .hints{gap:14px}
pub const STATUS_H: f32 = 24.0; // .status{height:24px}
pub const STATUS_MERGED_H: f32 = 26.0; // .win.merged .status{height:26px}
pub const STATUS_PAD_X: f32 = 12.0; // .status{padding:0 12px}
pub const STATUS_GAP: f32 = 16.0; // .status{gap:16px}
pub const STATUS_L_GAP: f32 = 10.0; // .status .l{gap:10px}
pub const STATUS_R_GAP: f32 = 6.0; // .status .r{gap:6px}
pub const STATUS_FLD_PAD_X: f32 = 5.0; // .status .fld{padding:2px 5px}
pub const GUTTER_W: f32 = 52.0; // .gutter{width:52px}
pub const GUTTER_PAD_R: f32 = 14.0; // .gutter div{padding-right:14px}
pub const TEXT_PAD_L: f32 = 4.0; // .ta{padding:10px 16px 10px 4px}
pub const TEXT_PAD_T: f32 = 10.0;
pub const LINE_H: f32 = 20.0; // --lh:20px
pub const HEX_PAD_L: f32 = 16.0; // .hex{padding:10px 16px}
pub const POPUP_RADIUS: f32 = 8.0; // .psuggest / .dropdown{border-radius:8px}
pub const POPUP_PAD: f32 = 4.0;

pub const FONT_UI: f32 = 12.0; // tabs, título
pub const FONT_MENU: f32 = 12.5; // .menubar > button
pub const FONT_STATUS: f32 = 11.0; // .status
pub const FONT_MONO: f32 = 13.0; // .ta / .gutter / .hex
pub const FONT_HINTS: f32 = 11.0; // .hints (mono)
pub const FONT_PROMPT: f32 = 12.5; // .pfield (mono)
pub const FONT_PROMPT_LABEL: f32 = 11.5; // .plabel / .pword / .count
pub const FONT_SUGGEST: f32 = 12.0; // .prow (mono)
pub const FONT_OPT: f32 = 11.0; // .opt (mono)

/// Qué franjas se muestran, ya resuelto a partir de `UiConfig`, el número de
/// documentos y si el menú `Alt` está desplegado.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Bands {
    pub tabs_in_title: bool,
    pub menubar: bool,
    pub tabs_below: bool,
    pub hints: bool,
    pub merged_status: bool,
}

/// Rectángulos de cada franja para una ventana de `w`×`h` DIPs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Frame {
    pub titlebar: Rect,
    pub caption_min: Rect,
    pub caption_max: Rect,
    pub caption_close: Rect,
    pub menubar: Rect,
    pub tabs_below: Rect,
    pub body: Rect,
    pub hints: Rect,
    pub status: Rect,
}

pub fn frame(w: f32, h: f32, bands: Bands) -> Frame {
    let titlebar = Rect::new(0.0, 0.0, w, TITLEBAR_H);
    let close = Rect::new(w - CAPTION_BTN_W, 0.0, w, TITLEBAR_H);
    let max = Rect::new(close.left - CAPTION_BTN_W, 0.0, close.left, TITLEBAR_H);
    let min = Rect::new(max.left - CAPTION_BTN_W, 0.0, max.left, TITLEBAR_H);

    let mut y = TITLEBAR_H;
    let menubar = if bands.menubar {
        let r = Rect::new(0.0, y, w, y + MENUBAR_H);
        y += MENUBAR_H;
        r
    } else {
        Rect::default()
    };
    let tabs_below = if bands.tabs_below {
        let r = Rect::new(0.0, y, w, y + TABS_BELOW_H);
        y += TABS_BELOW_H;
        r
    } else {
        Rect::default()
    };

    let status_h = if bands.merged_status { STATUS_MERGED_H } else { STATUS_H };
    let status = Rect::new(0.0, (h - status_h).max(y), w, h);
    let hints = if bands.hints && !bands.merged_status {
        Rect::new(0.0, (status.top - HINTS_H).max(y), w, status.top)
    } else {
        Rect::default()
    };
    let body_bottom = if hints.is_empty() { status.top } else { hints.top };
    let body = Rect::new(0.0, y, w, body_bottom.max(y));

    Frame { titlebar, caption_min: min, caption_max: max, caption_close: close, menubar, tabs_below, body, hints, status }
}

/// Ancho del canal de números: 52 DIPs como la maqueta, o más si el número más largo
/// no cabe con su margen derecho de 14 (a partir de 5 dígitos con Cascadia 13 px).
pub fn gutter_width(total_lines: usize, digit_w: f32) -> f32 {
    let digits = total_lines.max(1).to_string().len() as f32;
    GUTTER_W.max(digits * digit_w + GUTTER_PAD_R + TEXT_PAD_L * 2.0)
}

/// Líneas de texto que caben en el cuerpo (con su margen superior de 10 DIPs).
pub fn visible_lines(body: Rect) -> usize {
    (((body.height() - TEXT_PAD_T).max(0.0)) / LINE_H).floor().max(1.0) as usize
}

/// Geometría de una pestaña ya colocada.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabGeom {
    pub rect: Rect,
    /// Dónde empieza el nombre y cuánto ancho tiene disponible (se recorta con «…»).
    pub name_x: f32,
    pub name_w: f32,
    /// Posición del punto ● de «sin guardar», si lo hay.
    pub dot_x: Option<f32>,
    pub close: Rect,
}

/// Coloca las pestañas de izquierda a derecha empezando en `x0`, con su borde
/// inferior en `bottom`. `name_w[i]` es el ancho medido del nombre, `dirty[i]` si
/// lleva ●, `dot_w` el ancho medido de «●». Se detiene (sin dibujar a medias) al
/// llegar a `max_right`. Devuelve también el rectángulo del botón `+`.
pub fn tabs(x0: f32, bottom: f32, max_right: f32, name_w: &[f32], dirty: &[bool], dot_w: f32) -> (Vec<TabGeom>, Rect) {
    let mut out = Vec::with_capacity(name_w.len());
    let mut x = x0;
    let top = bottom - TAB_H;
    for (i, &nw) in name_w.iter().enumerate() {
        let dot_extra = if dirty.get(i).copied().unwrap_or(false) { TAB_INNER_GAP + dot_w } else { 0.0 };
        let fixed = TAB_PAD_L + dot_extra + TAB_INNER_GAP + TAB_CLOSE + TAB_PAD_R;
        let width = (fixed + nw).min(TAB_MAX_W);
        if x + width > max_right {
            break;
        }
        let rect = Rect::new(x, top, x + width, bottom);
        let close_left = rect.right - TAB_PAD_R - TAB_CLOSE;
        let close_top = top + (TAB_H - TAB_CLOSE) / 2.0;
        let close = Rect::new(close_left, close_top, close_left + TAB_CLOSE, close_top + TAB_CLOSE);
        let name_x = rect.left + TAB_PAD_L;
        let name_w = (width - fixed).max(0.0);
        let dot_x = if dot_extra > 0.0 { Some(name_x + name_w + TAB_INNER_GAP) } else { None };
        out.push(TabGeom { rect, name_x, name_w, dot_x, close });
        x = rect.right + TAB_GAP;
    }
    let plus_left = (x - TAB_GAP) + PLUS_ML;
    // .plus{align-self:center;margin-top:2px}: centrado en la fila de 34 px, 1 px más abajo.
    let row_top = bottom - TITLEBAR_H.max(TAB_H);
    let plus_top = row_top + (TITLEBAR_H - PLUS_H) / 2.0 + 1.0;
    let plus = Rect::new(plus_left, plus_top, plus_left + PLUS_W, plus_top + PLUS_H);
    (out, plus)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moderna_bands_match_mockup() {
        let f = frame(920.0, 600.0, Bands { tabs_in_title: true, hints: true, ..Default::default() });
        assert_eq!(f.body.top, 34.0);
        assert_eq!(f.status, Rect::new(0.0, 576.0, 920.0, 600.0));
        assert_eq!(f.hints, Rect::new(0.0, 554.0, 920.0, 576.0));
        assert_eq!(f.body.bottom, 554.0);
        assert_eq!(f.caption_close.left, 874.0);
        assert_eq!(f.caption_min.left, 782.0);
    }

    #[test]
    fn clasica_stacks_menubar_and_tabs_below() {
        let f = frame(920.0, 600.0, Bands { menubar: true, tabs_below: true, ..Default::default() });
        assert_eq!(f.menubar, Rect::new(0.0, 34.0, 920.0, 64.0));
        assert_eq!(f.tabs_below, Rect::new(0.0, 64.0, 920.0, 96.0));
        assert_eq!(f.body.top, 96.0);
        assert!(f.hints.is_empty());
        assert_eq!(f.body.bottom, 576.0);
    }

    #[test]
    fn merged_status_is_taller_and_hides_hints() {
        let f = frame(920.0, 600.0, Bands { hints: true, merged_status: true, ..Default::default() });
        assert_eq!(f.status.top, 574.0);
        assert!(f.hints.is_empty());
    }

    #[test]
    fn tiny_window_never_inverts_rects() {
        let f = frame(100.0, 40.0, Bands { menubar: true, tabs_below: true, hints: true, ..Default::default() });
        assert!(f.body.height() >= 0.0);
        assert!(f.status.top >= f.body.top);
    }

    #[test]
    fn gutter_is_52_until_numbers_do_not_fit() {
        assert_eq!(gutter_width(13, 7.8), 52.0);
        assert_eq!(gutter_width(9999, 7.8), 52.0);
        assert!(gutter_width(100_000, 7.8) > 52.0);
    }

    #[test]
    fn tab_width_is_padding_plus_name_plus_close() {
        let (t, _) = tabs(36.0, 34.0, 700.0, &[50.0], &[false], 6.0);
        assert_eq!(t[0].rect, Rect::new(36.0, 6.0, 36.0 + 12.0 + 50.0 + 6.0 + 18.0 + 6.0, 34.0));
        assert_eq!(t[0].name_x, 48.0);
        assert_eq!(t[0].close.width(), 18.0);
        assert_eq!(t[0].close.right, t[0].rect.right - 6.0);
        assert!(t[0].dot_x.is_none());
    }

    #[test]
    fn dirty_tab_reserves_room_for_the_dot() {
        let (t, _) = tabs(36.0, 34.0, 700.0, &[50.0], &[true], 6.0);
        assert_eq!(t[0].rect.width(), 12.0 + 50.0 + 6.0 + 6.0 + 6.0 + 18.0 + 6.0);
        assert_eq!(t[0].dot_x, Some(48.0 + 50.0 + 6.0));
    }

    #[test]
    fn long_names_are_capped_at_190() {
        let (t, _) = tabs(36.0, 34.0, 700.0, &[400.0], &[false], 6.0);
        assert_eq!(t[0].rect.width(), 190.0);
        assert_eq!(t[0].name_w, 190.0 - (12.0 + 6.0 + 18.0 + 6.0));
    }

    #[test]
    fn tabs_stop_before_max_right_and_plus_follows_last() {
        let (t, plus) = tabs(36.0, 34.0, 250.0, &[50.0, 50.0, 50.0], &[false; 3], 6.0);
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].rect.left, t[0].rect.right + 2.0);
        assert_eq!(plus.left, t[1].rect.right + 2.0);
        assert_eq!(plus.height(), 24.0);
    }
}
```

- [x] **Step 2:** `cargo test -p notty-ui layout` → 9 tests pasan. (Se corrigió `gutter_width`: el margen era `TEXT_PAD_L` una vez, no `*2`, para cuadrar con el propio test del plan y el comentario "a partir de 5 dígitos".)
- [x] **Step 3:** Borra `crates/notty-ui/src/gutter.rs` (lo sustituye `layout::gutter_width`), quita su `mod`/`pub use` de `lib.rs` y cambia las llamadas a `crate::gutter_width` por `crate::layout::gutter_width`. `cargo test --workspace` pasa (3 tests menos, los del gutter viejo).
- [x] **Step 4: Commit** `feat(ui): medidas de la maqueta en layout.rs`

---

### Task 3: Textos de las barras (`chrome_text.rs`)

**Files:**
- Create: `crates/notty-ui/src/chrome_text.rs`
- Delete: `crates/notty-ui/src/hints.rs`, `crates/notty-ui/src/status.rs` (su contenido pasa aquí con el formato de la maqueta)
- Modify: `crates/notty-ui/src/lib.rs`, cualquier llamada a `hints_text`/`status_line`

Reproduce **literalmente** `renderHints()` (maqueta, líneas 514-523) y la parte de texto de `renderStatus()` (líneas 529-551).

- [x] **Step 1: Escribe `chrome_text.rs`**

```rust
//! Qué pone en la barra de atajos y en la barra de estado, según lo que se esté haciendo.

use notty_core::Document;
use notty_io::{LineEnding, TextEncoding};

/// Contexto que decide la barra de atajos (el primero que aplique, en este orden).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintsCtx {
    PathOpen,
    PathSave,
    Find,
    Replace,
    Raw,
    VimNormal,
    VimInsert,
    Normal,
}

/// Pares (tecla, acción). La tecla va en negrita `--text-hint`, la acción en `--text-2`.
pub fn hints_items(ctx: HintsCtx) -> &'static [(&'static str, &'static str)] {
    match ctx {
        HintsCtx::PathOpen => &[("Tab", "Completar"), ("Enter", "Abrir"), ("Esc", "Cancelar"), ("^O", "Diálogo de Windows")],
        HintsCtx::PathSave => &[("Tab", "Completar"), ("Enter", "Crear"), ("Esc", "Cancelar"), ("^O", "Diálogo de Windows")],
        HintsCtx::Find => &[("Enter", "Siguiente"), ("⇧Enter", "Anterior"), ("Alt+C", "Aa"), ("Alt+W", "Palabra"), ("Alt+R", "Regex"), ("Esc", "Cerrar")],
        HintsCtx::Replace => &[("Enter", "Reemplazar"), ("^Alt+Enter", "Todas"), ("Alt+C", "Aa"), ("Alt+W", "Palabra"), ("Alt+R", "Regex"), ("Esc", "Cerrar")],
        HintsCtx::Raw => &[("^S", "Guardar"), ("^Shift+H", "Ver como texto"), ("←→↑↓", "Moverse"), ("0-F", "Escribir byte")],
        HintsCtx::VimNormal => &[("i", "Insertar"), ("hjkl", "Moverse"), ("dd", "Borrar línea"), ("/", "Buscar"), (":w", "Guardar"), ("^Alt+V", "Salir de vim")],
        HintsCtx::VimInsert => &[("Esc", "Modo normal"), ("^S", "Guardar")],
        HintsCtx::Normal => &[("^S", "Guardar"), ("^F", "Buscar"), ("^H", "Reemplazar"), ("^O", "Abrir"), ("^Alt+V", "Vim")],
    }
}

/// Campos de la derecha de la barra de estado en modo texto: `Ln 1, Col 1`, codificación, EOL.
pub fn status_right(doc: &Document, encoding: TextEncoding, eol: LineEnding) -> [String; 3] {
    let (line, col) = doc.line_col();
    [format!("Ln {}, Col {}", line + 1, col + 1), encoding.label().to_string(), eol.label().to_string()]
}

/// Campo de la derecha en vista raw: desplazamiento del byte seleccionado, `0x0000000a`.
pub fn raw_offset(offset: usize) -> String {
    format!("0x{offset:08x}")
}

/// Texto de la izquierda en vista raw: `Raw · 384 B`.
pub fn raw_size(len: usize) -> String {
    format!("Raw · {len} B")
}

/// Nombre que se enseña de un documento (pestaña y título): el nombre del archivo, o
/// `sin ruta` si todavía no tiene (lo que la maqueta llama `CLICKME`).
pub fn doc_name(path: Option<&std::path::Path>) -> String {
    path.and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "sin ruta".into())
}

/// Título de la ventana (barra de tareas, y barra de título cuando no hay pestañas en ella):
/// `notas.txt · notty`, con ` •` tras el nombre si hay cambios sin guardar.
pub fn window_title(name: &str, dirty: bool, zen: bool) -> String {
    let dot = if dirty { " •" } else { "" };
    if zen { format!("{name}{dot}") } else { format!("{name}{dot} · notty") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_hints_match_mockup() {
        assert_eq!(hints_items(HintsCtx::Normal)[0], ("^S", "Guardar"));
        assert_eq!(hints_items(HintsCtx::Normal).len(), 5);
    }

    #[test]
    fn path_hints_depend_on_purpose() {
        assert_eq!(hints_items(HintsCtx::PathOpen)[1].1, "Abrir");
        assert_eq!(hints_items(HintsCtx::PathSave)[1].1, "Crear");
    }

    #[test]
    fn status_right_is_three_separate_fields() {
        let mut doc = Document::new("hola\nmundo", "\n");
        doc.set_cursor(7);
        assert_eq!(status_right(&doc, TextEncoding::Utf8, LineEnding::Lf), ["Ln 2, Col 3".to_string(), "UTF-8".into(), "LF".into()]);
    }

    #[test]
    fn raw_labels() {
        assert_eq!(raw_offset(10), "0x0000000a");
        assert_eq!(raw_size(384), "Raw · 384 B");
    }

    #[test]
    fn names_and_titles() {
        assert_eq!(doc_name(None), "sin ruta");
        assert_eq!(doc_name(Some(std::path::Path::new(r"C:\a\notas.txt"))), "notas.txt");
        assert_eq!(window_title("notas.txt", true, false), "notas.txt • · notty");
        assert_eq!(window_title("notas.txt", false, true), "notas.txt");
    }
}
```

- [x] **Step 2:** Borra `hints.rs` y `status.rs`, ajusta `lib.rs` (`mod chrome_text; pub use chrome_text::*;`), y sustituye sus usos en `render.rs` provisionalmente (se reescribe en la Task 5). `cargo test --workspace` pasa.
- [x] **Step 3: Commit** `feat(ui): textos de atajos y estado con el formato de la maqueta`

---

### Task 4: Zen como en la maqueta (config)

**Files:**
- Modify: `crates/notty-config/src/model.rs`, `crates/notty-config/src/preset.rs`, sus tests

La maqueta define los presets así (línea 362): Moderna y Clásica con números de línea y sin línea fusionada; **Zen sin números de línea y con la línea de comandos fusionada** (estado y prompts en una sola franja de 26 px, fondo `--cmd`, mono 12 px).

- [ ] **Step 1:** Añade `pub merged_command_line: bool` a `UiConfig` (default `false`, `#[serde(default)]` ya cubre los `config.toml` viejos).
- [ ] **Step 2:** En `apply_preset`: Moderna y Clásica ponen `line_numbers = true` y `merged_command_line = false`; Zen pone `line_numbers = false` y `merged_command_line = true`. Actualiza el comentario de la función (ya no deja `line_numbers` fuera). Añade al test de Zen `assert!(!ui.line_numbers); assert!(ui.merged_command_line);` y a los otros dos lo contrario.
- [ ] **Step 3:** `cargo test -p notty-config` pasa.
- [ ] **Step 4: Commit** `feat(config): Zen sin números y con línea de comandos fusionada, como la maqueta`

---

### Task 5: Renderer nuevo — formatos de texto, brocha única y lista de zonas

**Files:**
- Modify (reescritura): `crates/notty-ui/src/render.rs`

Esta tarea deja la base; las Tasks 6-9 dibujan cada parte encima.

- [ ] **Step 1: Estado del renderer.** Sustituye los 8 pinceles por **una** `ID2D1SolidColorBrush` y un ayudante que le cambia el color antes de cada uso:

```rust
fn color(c: Rgba) -> D2D1_COLOR_F { D2D1_COLOR_F { r: c.0, g: c.1, b: c.2, a: c.3 } }
fn fill(&self, r: Rect, c: Rgba)                  // FillRectangle
fn fill_round(&self, r: Rect, radius: f32, c: Rgba) // FillRoundedRectangle
fn stroke_line(&self, x0, y0, x1, y1, width, c)    // DrawLine
fn text(&self, s: &str, fmt: &IDWriteTextFormat, x: f32, y: f32, max_w: f32, h: f32, c: Rgba) // DrawTextLayout, alineado verticalmente al centro de h
fn measure(&self, s: &str, fmt: &IDWriteTextFormat) -> f32 // GetMetrics().widthIncludingTrailingWhitespace
```

  Formatos de texto (crea todos en `new`, con `SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)`; los que se recortan con «…» llevan `SetTrimming` con `DWRITE_TRIMMING_GRANULARITY_CHARACTER` y `CreateEllipsisTrimmingSign`):
  `ui_12` (Segoe UI Variable Text 12, tabs y título), `ui_12_5` (menú), `ui_11` (estado), `ui_11_5` (etiquetas de prompt, contador), `ui_13` / `ui_20_semibold` / `ui_11_5_semibold` (Ajustes, Task 11), `mono_13` (documento, canal, hex), `mono_13_bold` (bytes modificados), `mono_11` (atajos, opciones `Aa`), `mono_11_bold` (teclas de atajos), `mono_11_5_semibold` (`-- NORMAL --`), `mono_12` (sugerencias, CLICKME), `mono_12_5` (campo de prompt).

  Fallback de familia: al crear el renderer, busca `Segoe UI Variable Text` en `GetSystemFontCollection` con `FindFamilyName`; si `exists` es falso, usa `Segoe UI`. Igual con `Cascadia Mono` → `Consolas`.

- [ ] **Step 2: DPI.** `Renderer::new(hwnd, dpi: u32)` crea el render target con `pixelSize` = tamaño real del cliente (`GetClientRect`) y llama a `SetDpi(dpi as f32, dpi as f32)`. Añade `pub fn set_dpi(&mut self, dpi: u32)` y `pub fn scale(&self) -> f32` (= `dpi / 96`). `resize(w, h)` sigue recibiendo píxeles físicos. `GetSize()` ya devuelve DIPs.

- [ ] **Step 3: Zonas para el ratón.** Sustituye `tab_rects` y `pencil_rect` por una lista que se rellena durante cada `paint`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    None,
    /// Zona libre de la barra de título: arrastrar mueve la ventana.
    Caption,
    Min, Max, Close,
    Tab(usize), TabClose(usize), NewTab,
    Menu(usize), MenuItem(usize),
    Clickme, Pencil,
    Suggestion(usize),
    /// 0 = Aa, 1 = ab, 2 = .*
    SearchOpt(u8),
    /// En el prompt de reemplazar: 0 = campo buscar, 1 = campo «por».
    SearchField(u8),
    Body,
}
// en Renderer:
hits: Vec<(Rect, Hit)>,
pub fn hit(&self, x_dip: f32, y_dip: f32) -> Hit  // recorre `hits` de atrás adelante (lo último dibujado queda encima); si nada coincide y y < TITLEBAR_H → Caption; si no → None
```

  Adapta `window.rs` para que siga compilando con la nueva API (el clic en pestaña y en el lápiz pasan por `renderer.hit`); el resto de `window.rs` se toca en la Task 8.

- [ ] **Step 4: Nuevo `paint(ws, ui, view: &ViewState)`** donde `ViewState` (definido en `render.rs`, lo rellena `window.rs`) lleva lo que no está en el `Workspace`:

```rust
pub struct ViewState {
    pub dark: bool,
    pub hover: Hit,
    pub pressed: Hit,
    pub maximized: bool,
    pub active_window: bool,
    /// Menú `Alt` desplegado, o índice del menú abierto de la barra de menús.
    pub menu_bar_visible: bool,
    pub open_menu: Option<usize>,
}
```

  Orden de dibujo: `Clear(surface)` → barra de título → barra de menús → pestañas debajo → cuerpo (canal, texto o hex) → barra de atajos → barra de estado / prompt → desplegables (sugerencias de ruta, menú abierto), que van al final para quedar encima.

  Las franjas se calculan con `layout::frame(size.width, size.height, bands)`, con `bands` resuelto así (maqueta, `tabsVisible()` línea 462):
  `tabs_in_title` = `files == Tabs` y (`tabs_position == Title` o (`Auto` y `ws.len() > 1`)); `tabs_below` = `files == Tabs` y `tabs_position == Below`; `menubar` = `Visible`, o `Alt` con el menú desplegado; `hints` = `hints_bar` (y no fusionada); `merged_status` = `merged_command_line`.

- [ ] **Step 5:** Deja dibujado, como mínimo, el fondo y el documento con las medidas nuevas (`mono_13`, interlineado `LINE_H`, origen del texto en `body.left + gutter_w + TEXT_PAD_L`, `body.top + TEXT_PAD_T`), para poder compilar y capturar. `char_index_at` usa ese mismo origen, en DIPs.
- [ ] **Step 6:** `cargo build --release`, `cargo test --workspace`, captura con `tools/shot-app.ps1`: el texto debe caer en la misma posición y tamaño que en `ref/moderna-oscuro.png` (aunque todavía falte la barra de título propia).
- [ ] **Step 7: Commit** `refactor(ui): renderer con paleta, formatos de texto de la maqueta y zonas de ratón`

---

### Task 6: Barra de título propia y DPI por monitor (`window.rs`)

**Files:**
- Modify: `crates/notty-ui/src/window.rs`, `crates/notty-ui/Cargo.toml` (features si faltan: `Win32_UI_HiDpi` ya está; añade `Win32_UI_Controls` ya está; comprueba `Win32_Graphics_Dwm`)

Objetivo: que desaparezca la barra de título de Windows y la dibujemos nosotros, **conservando** todo lo nativo: arrastrar, doble clic para maximizar, Aero Snap, el menú de Alt+Espacio, la sombra y las esquinas redondeadas de Windows 11, y el desplegable de Snap Layouts al pasar por el botón maximizar.

- [ ] **Step 1: DPI.** Al principio de `run`, antes de crear ventanas: `SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)` (ignora el error si ya estaba puesto). Crea la ventana con tamaño `920×600` DIPs convertido con `GetDpiForWindow` (crea la ventana, lee su DPI, y `SetWindowPos` a `920*scale × 600*scale` antes de mostrarla). Maneja `WM_DPICHANGED`: `renderer.set_dpi(HIWORD(wparam))` y `SetWindowPos` al `RECT` sugerido en `lparam`.
- [ ] **Step 2: Quitar la barra nativa** conservando los bordes de redimensionar de los lados y de abajo (el mismo enfoque que Windows Terminal):

```rust
WM_NCCALCSIZE if wparam.0 != 0 => {
    let params = &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS);
    let original_top = params.rgrc[0].top;
    let r = DefWindowProcW(hwnd, msg, wparam, lparam);   // calcula bordes izq/der/abajo
    params.rgrc[0].top = original_top;                    // pero nada de barra de título
    if IsZoomed(hwnd).as_bool() {
        // Maximizada, Windows la saca `frame` px por arriba: los recuperamos.
        let dpi = GetDpiForWindow(hwnd);
        params.rgrc[0].top += GetSystemMetricsForDpi(SM_CYFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
    }
    r
}
```

  Tras crear la ventana: `SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER)` para que se aplique, y `DwmExtendFrameIntoClientArea(hwnd, &MARGINS { cyTopHeight: 1, ..Default::default() })` para que DWM siga dibujando sombra y borde. Pon `DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND`. **Quita Mica** (`DWMWA_SYSTEMBACKDROP_TYPE`): la maqueta usa colores sólidos. Mantén `DWMWA_USE_IMMERSIVE_DARK_MODE`, pero con el valor de `theme::is_dark(cfg.ui.theme, system_uses_dark_mode())`, y vuelve a ponerlo cuando cambie el tema en Ajustes.

- [ ] **Step 3: `WM_NCHITTEST`.** Primero `DefWindowProcW`; si no devuelve `HTCLIENT`, devuelve eso (bordes). Si devuelve `HTCLIENT`, pasa el punto a coordenadas de cliente en DIPs y:
  - no maximizada y `y < borde superior` (`GetSystemMetricsForDpi(SM_CYFRAME) + SM_CXPADDEDBORDER`, en píxeles físicos) → `HTTOP`;
  - `renderer.hit(..) == Hit::Max` → `HTMAXBUTTON` (así Windows 11 enseña Snap Layouts);
  - `Hit::Caption` → `HTCAPTION`;
  - cualquier otra cosa → `HTCLIENT`.
- [ ] **Step 4: Botones de la barra.** Min y Cerrar se reciben como clic normal de cliente (`WM_LBUTTONDOWN`/`UP` sobre `Hit::Min`/`Hit::Close`, se ejecuta al soltar si el ratón sigue encima): `ShowWindow(SW_MINIMIZE)` / `PostMessageW(WM_CLOSE)`. Maximizar llega como `WM_NCLBUTTONDOWN` con `HTMAXBUTTON`: devuelve 0 (no pases a `DefWindowProc`, o pintaría botones antiguos) y en `WM_NCLBUTTONUP` con `HTMAXBUTTON` alterna `SW_MAXIMIZE`/`SW_RESTORE`. Para el hover del botón maximizar usa `WM_NCMOUSEMOVE` + `WM_NCMOUSELEAVE` (con `TrackMouseEvent(TME_LEAVE | TME_NONCLIENT)`); para el resto `WM_MOUSEMOVE` + `WM_MOUSELEAVE` (`TrackMouseEvent(TME_LEAVE)`). Guarda `hover: Hit` y `pressed: Hit` en `WindowState` y repinta solo si cambian.
- [ ] **Step 5:** `WM_ACTIVATE` y `WM_SIZE` repintan (el glifo de maximizar cambia a «restaurar» cuando la ventana está maximizada). `WM_SIZE` recalcula `visible_lines` con `layout::visible_lines(frame.body)` (no con `height - line_height` como ahora).
- [ ] **Step 6:** Todas las coordenadas del ratón (`WM_LBUTTONDOWN`, `WM_MOUSEMOVE`...) se dividen por `renderer.scale()` antes de usarlas. Un clic en la barra de título, las pestañas o las barras **no** mueve el cursor del texto; solo `Hit::Body`.
- [ ] **Step 7:** Captura la app: ya no debe verse la barra de Windows, y la ventana se tiene que poder arrastrar, maximizar con doble clic, redimensionar por los cuatro lados y encajar con Win+flechas. Comprueba a mano lo que puedas (arrastre con `SendKeys` no se puede; déjalo anotado para el usuario).
- [ ] **Step 8: Commit** `feat(ui): barra de título propia y DPI por monitor`

---

### Task 7: Barra de título, pestañas, menú y cuerpo como la maqueta

**Files:**
- Modify: `crates/notty-ui/src/render.rs`, `crates/notty-ui/src/workspace.rs` (añadir `pub fn close(&mut self, idx: usize) -> bool`, igual que `close_active` pero con índice; con test)

Cada punto cita el CSS de la maqueta que reproduce. Compara con `ref/moderna-oscuro.png`, `ref/sucio.png`, `ref/clasica.png`, `ref/zen.png`, `ref/moderna-claro.png`.

- [ ] **Step 1: Barra de título** (`.titlebar`): relleno `chrome` en `frame.titlebar`.
  - **Icono** (`ICON()`, 16×16 centrado en los primeros 36 px): rectángulo redondeado `x 2.5 y 1.5 w 11 h 13 r 2` sin relleno, trazo 1.4 `accent`; tres líneas de trazo 1.3 con extremos redondeados (`CreateStrokeStyle` con `D2D1_CAP_STYLE_ROUND`) color `text_2`: `(5,5.5)-(11,5.5)`, `(5,8)-(11,8)`, `(5,10.5)-(8.5,10.5)`, todo relativo a la esquina del icono.
  - **Pestañas** si `tabs_in_title`: `layout::tabs(APPICON_W, TITLEBAR_H, caption_min.left, ...)`. Activa: `fill_round` con `surface` y radio 6 **solo arriba** (dibuja el rectángulo redondeado 6 px más alto por abajo y deja que el cuerpo, que se pinta después, tape el sobrante; o usa una `ID2D1PathGeometry`), nombre en `ui_12` `text`. Inactiva: sin relleno, nombre `text_2`; con hover, `fill_round` `hover`. Punto «●» de sin guardar: `ui_12` a 9 px (formato propio `ui_9`), color `text_2`. **✕** (10×10, dos líneas de trazo 1.1 `currentColor`) solo en la activa o con hover: color `text_3`, y con hover sobre la ✕, fondo `hover` redondeado 4 y color `text`. **+** (12×12, dos líneas de trazo 1.1) en `text_3`, hover con fondo `hover` redondeado 5. Registra `Hit::Tab(i)`, `Hit::TabClose(i)`, `Hit::NewTab`.
  - **Título** si no hay pestañas en la barra (`.wtitle`): `window_title(doc_name, dirty, preset == Zen)` en `ui_12` `text_2`, empezando en x = 36, centrado en vertical.
  - **Botones** (`.caption button`, 46 px cada uno, glifos 10×10 trazo 1 `text_2`): minimizar = línea horizontal en y 5; maximizar = cuadrado `0.5,0.5,9,9`; restaurar (maximizada) = dos cuadrados de 8×8 desplazados 2 px; cerrar = dos diagonales. Hover: fondo `hover`; hover en cerrar: fondo `close_hover` y glifo `close_hover_fg`. Registra `Hit::Min/Max/Close`.
  - Al final registra `(frame.titlebar, Hit::Caption)` **antes** que el resto de zonas de la barra, para que las pestañas y botones queden encima en `hit()`.
- [ ] **Step 2: Barra de menús** (`.menubar`, si `bands.menubar`): fondo `chrome`; «Archivo Editar Buscar Ver Ayuda» en `ui_12_5` `text`, cada botón con 10 px de relleno a cada lado, separación 2, empezando en x = 6; hover o abierto: fondo `hover` redondeado 4. Registra `Hit::Menu(i)`. (El desplegable, Task 9.)
- [ ] **Step 3: Pestañas debajo** (`.tabs.below`): fondo `chrome` en `frame.tabs_below`; pestañas con `layout::tabs(TABS_BELOW_PAD_X, frame.tabs_below.bottom, w - 6, ...)`, mismo dibujo que en la barra.
- [ ] **Step 4: Cuerpo** (`.body`, `.gutter`, `.ta`): fondo `surface`. Canal si `line_numbers` y no es raw: ancho `layout::gutter_width`, números en `mono_13` alineados a la derecha con margen 14, color `text_3`; **el de la línea del cursor en `text`**. Texto del documento en `mono_13` `text`. Selección: `accent_soft`. Caret: 1 DIP de ancho, color `text` (en vim NORMAL, `accent`, como `.win.vim-normal .ta{caret-color:var(--accent)}`). Coincidencias de búsqueda: `mark` y la actual `mark_cur`, redondeadas 2.
- [ ] **Step 5: Barra de atajos** (`.hints`): fondo `surface_2`, línea de 1 px `line` en su borde superior. Elige `HintsCtx` según prompt → raw → vim (normal/insert) → normal, y dibuja los pares con relleno izquierdo 12 y separación 14 entre pares: la tecla en `mono_11_bold` `text_hint`, un espacio, la acción en `mono_11` `text_2`.
- [ ] **Step 6:** `window.rs`: el título de la ventana (`SetWindowTextW`, lo que sale en la barra de tareas) pasa a ser `window_title(...)`, no la ruta completa. Clic en `Hit::Tab(i)` activa; en `Hit::TabClose(i)` llama a `ws.close(i)`; en `Hit::NewTab` abre un documento vacío (como `Ctrl+N`).
- [ ] **Step 7:** Capturas y comparación (oscuro, claro, sucio, clásica, zen). Corrige hasta que coincidan.
- [ ] **Step 8: Commit** `feat(ui): barra de título, pestañas, menú, canal y atajos como la maqueta`

---

### Task 8: Barra de estado y prompts como la maqueta

**Files:**
- Modify: `crates/notty-ui/src/render.rs`, `crates/notty-ui/src/search_prompt.rs`, `crates/notty-ui/src/window.rs`

Compara con `ref/moderna-oscuro.png`, `ref/clickme.png`, `ref/vim-normal.png`, `ref/vim-insert.png`, `ref/ruta-sugerencias.png`, `ref/ruta-nueva.png`, `ref/buscar.png`, `ref/reemplazar.png`, `ref/zen.png`.

- [ ] **Step 1: Barra de estado normal** (`.status`): fondo `chrome` (fusionada: `cmd` con línea superior `line`), relleno 12 a cada lado, `ui_11` `text_2` (fusionada: `mono_12`).
  Izquierda, en este orden y separados 10: `-- NORMAL --` / `-- INSERT --` / `-- VISUAL --` si vim está activo (`mono_11_5_semibold`, `accent`; INSERT en `ok`); luego, si es raw → `raw_size` + candado y «solo lectura»/«escritura» en `text_3` + lápiz (Step 2); si no tiene ruta → **CLICKME** (`mono_12`, `accent`, espaciado entre letras 0.04 em — con DirectWrite, `IDWriteTextLayout1::SetCharacterSpacing` de 0.5 DIP, o dibuja letra a letra; hover: fondo `accent_soft` redondeado 4; registra `Hit::Clickme`); si no → «Texto» (no en la fusionada).
  Derecha, alineados al borde derecho, separados 6, cada uno con 5 de relleno: los tres de `status_right`, o `raw_offset` en raw. En la fusionada, separados por «· » en `text_3`.
- [ ] **Step 2: Candado y lápiz** (`LOCK`, `PENCIL` de la maqueta, líneas 322-323, trazo 1.3): candado 12×12 = rectángulo redondeado `3,7,10,7 r1.5` + arco `M5.5 7V5a2.5 2.5 0 015 0v2` (`ID2D1PathGeometry` con `AddArc`); lápiz 14×14 dentro de un botón 24×22 = polígono `11,2.5 13.5,5 6,12.5 2.8,13.2 3.5,10` cerrado. Colores: sin permiso → `text_3` al 50 % de opacidad; solo lectura con permiso → `text_2` (hover: fondo `hover`, color `text`); escribiendo → fondo `accent_soft`, color `accent`. Registra `Hit::Pencil`. El candado solo sale cuando no hay permiso de escritura.
- [ ] **Step 3: Prompt de ruta** (`.prompt`, `.pfield`, `.pword`, `.psuggest`): ocupa la barra de estado entera (relleno 12). Campo en `mono_12_5`: el valor en `text` (en `danger` si es inválido), seguido del fantasma en `text_3`; si está vacío, el texto de ayuda «ruta del archivo» / «ruta donde guardar» en `text_3`. Caret de 1 DIP `text` al final del valor. A la derecha, separado 10: la palabra de estado en `ui_11_5` `text_3` («carpeta», «existe», «nuevo», «carpeta nueva», «no válido» en `danger`; saca la palabra de `PathPromptState::hint()`, añadiendo un `fn hint_word(&self) -> &'static str` en `path_prompt.rs` con test si no existe), y «Tab ↹» en `text_3` si hay sugerencias.
  **Lista de sugerencias** (máx. 5, como la maqueta): caja con esquina inferior izquierda en `(12, status.top - 2)`, fondo `chrome_hi`, radio 8, relleno 4, ancho entre 220 y 360 según la fila más larga + 16; sombra (`--shadow`: difusa 40 px desplazada 10 hacia abajo + anillo de 1 px — aproxímala con 4 rectángulos redondeados concéntricos cada vez más grandes y transparentes en `shadow`, más un borde de 1 px `shadow_ring`); filas de 24 (4 arriba y abajo + 16 de texto) en `mono_12` con relleno 8 a los lados: carpetas en `text` con «\» final, archivos en `text_2`; la seleccionada con fondo `accent_soft` redondeado 5 y texto `accent`; hover, fondo `hover`. Registra `Hit::Suggestion(i)`; clic = seleccionarla y aceptarla (como Tab).
- [ ] **Step 4: Buscar / reemplazar** (`renderPrompt` líneas 691-700): «buscar» (`ui_11_5`, `text_3`), campo `mono_12_5` `text` que ocupa el espacio libre; en reemplazar, además «por» y un segundo campo. A la derecha: contador en `mono_11_5` `text_3`, ancho mínimo 44 alineado a la derecha («2/5», «0/0», vacío si no hay búsqueda; si la regex no es válida, «regex ✕» en `danger`); luego `Aa`, `ab`, `.*` en `mono_11` con relleno 5×1 y radio 3: apagado `text_3`, encendido fondo `accent_soft` y texto `accent`. Registra `Hit::SearchOpt(0..3)` y `Hit::SearchField(0|1)`. Caret en el campo activo.
- [ ] **Step 5: Arreglo — el campo «por» no se podía escribir.** `SearchState` no tiene campo activo: todo lo tecleado va a la búsqueda. Añade `pub editing_replacement: bool`, que `Tab` alterne en el prompt de reemplazar (y un clic en `Hit::SearchField`), y que `handle_search_char`/Backspace escriban en `replacement` cuando esté activo. Test en `search_prompt.rs`. Backspace tampoco borraba en el prompt de búsqueda: compruébalo y arréglalo si hace falta.
- [ ] **Step 6: Línea de comandos vim** (`p.type === 'cmd'`): «:» en `ui_11_5` `text_3` (fusionada: `accent`), campo `mono_12_5`.
- [ ] **Step 7:** Clic en `Hit::Clickme` abre el prompt de ruta para guardar; en `Hit::Pencil`, alterna escritura (como ya hacía); en `Hit::SearchOpt(k)`, alterna esa opción.
- [ ] **Step 8:** Capturas de cada escenario y comparación. Para la de sugerencias: `-Keys '^o', '~\Documentos\p'` (si no tienes esa carpeta, crea antes `%USERPROFILE%\Documentos\proyectos\` o usa una ruta equivalente de tu equipo y compara solo el estilo).
- [ ] **Step 9: Commit** `feat(ui): barra de estado y prompts como la maqueta; se puede escribir en «por»`

---

### Task 9: Vista raw y menús desplegables

**Files:**
- Modify: `crates/notty-ui/src/render.rs`, `crates/notty-ui/src/raw_doc.rs`, `crates/notty-ui/src/window.rs`

- [ ] **Step 1: Hex** (`renderHex`, líneas 625-640 y CSS 148-157), compara con `ref/raw.png` y `ref/raw-solo-lectura.png`: origen `(body.left + 16, body.top + 10)`, `mono_13`, interlineado 20, sin canal de números. Por fila: desplazamiento de 8 dígitos hex **en minúsculas** en `text_3`, 3 espacios, 16 bytes en **mayúsculas** separados por un espacio y dos tras el octavo, 2 espacios, ASCII en `text_2`. Bytes `00` en `text_3`; bytes modificados en `accent` y `mono_13_bold` (añade `RawDoc::is_modified(i) -> bool` comparando con el original del mmap; con test); byte seleccionado con fondo `accent` redondeado 2 y texto `on_accent`; su carácter ASCII con fondo `accent_soft` y texto `text`. Mientras se escribe un nibble, la celda muestra ese nibble. Usa un `IDWriteTextLayout` por fila y `SetDrawingEffect` (con la brocha) para colorear rangos, en vez de un layout por byte. Revisa que `hex_row` produzca el formato de la maqueta (desplazamiento en minúsculas, bytes en mayúsculas); si no, ajústalo con sus tests.
- [ ] **Step 2: Menús desplegables** (`MENUS`, línea 496, y CSS `.dropdown`/`.menu-item`/`.menu-sep`): al hacer clic en `Hit::Menu(i)`, se abre debajo del botón (x del botón, y = `menubar.bottom - 2`), ancho mínimo 250, fondo `chrome_hi`, radio 8, relleno 4, sombra como la lista de sugerencias; elementos de 6×10 de relleno con el nombre en `ui_12_5` `text` a la izquierda y el atajo en `mono_11` `text_3` a la derecha; separadores de 1 px `line` con 4×6 de margen; hover con fondo `hover` redondeado 4. Clic en un elemento ejecuta su comando y cierra; clic fuera o Esc cierra. Mapea los comandos a lo que ya existe (Nuevo = `Ctrl+N`, Abrir = prompt de ruta, Guardar, Guardar como = prompt de ruta para guardar, Ajustes, Cerrar pestaña, Deshacer, Rehacer, Buscar, Reemplazar, Siguiente, Anterior, Modo vim, Ver como raw, Números de línea y Barra de atajos alternan la config y la guardan). Los que aún no existen (Nuevo temporal, Atajos de teclado, Acerca de notty) se dibujan igual pero no hacen nada.
- [ ] **Step 3:** Capturas y comparación (raw, raw sin permiso, clásica con un menú abierto).
- [ ] **Step 4: Commit** `feat(ui): vista raw y menús desplegables como la maqueta`

---

### Task 10: Tema claro y cambio de tema en caliente

**Files:**
- Modify: `crates/notty-ui/src/window.rs`

- [ ] **Step 1:** `ViewState.dark = theme::is_dark(cfg.ui.theme, system_uses_dark_mode())`. Maneja `WM_SETTINGCHANGE` con `lparam` = «ImmersiveColorSet»: vuelve a leer el registro, actualiza `DWMWA_USE_IMMERSIVE_DARK_MODE` y repinta. Al cambiar el tema desde Ajustes, lo mismo.
- [ ] **Step 2:** Captura con tema claro (pon `theme = "light"` en `config.toml` de prueba, o cambia Windows a claro) y compárala con `ref/moderna-claro.png`.
- [ ] **Step 3: Commit** `feat(ui): tema claro de la maqueta y cambio de tema en caliente`

---

### Task 11: Ajustes, dibujado como la maqueta

**Files:**
- Create: `crates/notty-ui/src/settings_model.rs` (puro, con tests)
- Modify (reescritura): `crates/notty-ui/src/settings_window.rs`

Compara con `ref/ajustes.png` (y abre la maqueta en el navegador → «Ajustes» para ver las otras secciones). Solo se incluyen las secciones cuyos ajustes ya existen en `Config`: **Apariencia, Ventana, Teclado**. Archivos y Atajo global llegan con el Plan 6, que añadirá sus filas a este mismo modelo.

- [ ] **Step 1: Modelo puro** (`SECTIONS`, líneas 993-1027): `pub struct Section { id, name, rows: Vec<Row> }`, `pub enum Row { Seg{title, desc, key, options: &[(&str, &str)]}, Select{..}, Toggle{..}, Kbd{title, keys}, Link{title, desc, label}, Group(&str) }` y `pub fn sections(cfg: &Config) -> Vec<Section>` con, textos literales de la maqueta:
  - Apariencia: Preset (seg Moderna/Clásica/Zen, desc «Punto de partida. Cambiar cualquier pieza lo convierte en Personalizado.»), Tema (seg Sistema/Claro/Oscuro, «Por defecto sigue al de Windows.»), Números de línea (toggle).
  - Ventana: Varios archivos (seg Pestañas/Buffers, «Pestañas visibles o buffers estilo vim (Ctrl+Tab, :b).»), Posición de las pestañas (select: En la barra de título / Bajo el menú / Solo si hay más de 1 / Ocultas), Barra de menús (select: Oculta / Visible / Aparece con Alt), Barra de atajos (toggle, «Estilo nano. Cambia según lo que estés haciendo.»), Barra de estado (toggle, «Si la ocultas, reaparece para rutas, búsquedas y avisos.»), Línea de comandos fusionada (toggle, «Una sola línea abajo para estado y comandos, como en Zen.»).
  - Teclado: Modo vim siempre (toggle, «Cada ventana arranca en modo vim.»), Alternar vim en esta ventana (kbd Ctrl+Alt+V), Ver como raw (kbd Ctrl+Shift+H), Todos los atajos (link «Abrir [keys]», «Cada acción es un comando con nombre. Reasigna cualquiera en config.toml, sección [keys].»).
  Y `pub fn apply(cfg: &mut Config, key: SettingKey, value: SettingValue)` que, como `setSetting` de la maqueta (línea 1054), aplica el preset si se elige uno y marca el preset como personalizado si se toca una pieza suelta (si `Preset` no tiene variante `Custom`, añádela a `notty-config` con `#[serde(rename = "custom")]` y test). Tests: número de secciones y filas, `apply` de preset, `apply` de pieza → personalizado.
  Las filas «Campos de la barra de estado» de la maqueta no se incluyen (no existen en `Config` todavía).
- [ ] **Step 2: Ventana** (CSS `.settings` líneas 214-239): ventana propia de 820×620 DIPs (misma técnica de barra de título que la Task 6, con altura 36 y solo el botón cerrar), centrada sobre la principal. Barra de título `chrome` con icono 14 px y «Ajustes · notty» en `ui_12` `text_2`. Columna de navegación de 200 DIPs, fondo `chrome`, relleno 8×6: botones de 7×12 de relleno en `ui_13` `text_2`; el actual con fondo `hover`, texto `text` y barra de 3 px `accent` redondeada a la izquierda (de y+9 a y-9); pie «Todo se guarda en **config.toml**.» + enlace «Editar el archivo» en `accent` (`ui_11_5`, `text_3`), abajo del todo con relleno 10×12. Panel con relleno 22×28: título de sección en `ui_20_semibold` `text` (en Apariencia, si el preset es personalizado, « · Personalizado» en `ui_12` `text_3`); filas `.srow` con fondo `surface_2`, radio 6, relleno 12×14, separación 3; título `ui_13` `text`, descripción `ui_11_5` `text_3` con ajuste de línea a 52 caracteres de ancho aprox.; grupos `.sgroup` `ui_12` semibold `text_2`.
  Controles: **seg** (`.seg`): fondo `hover` radio 6 relleno 2; opciones `ui_12` `text_2` relleno 3×9 radio 4; la marcada con fondo `surface`, texto `text` y borde 1 px `line`. **toggle** (40×20, radio 10): apagado = borde 1 px `text_2` y bolita 10 px `text_2` a 5 px; encendido = fondo `accent` y bolita `on_accent` desplazada 20. **select**: caja `ui_12_5` `text`, fondo `surface`, borde 1 px `line`, radio 5, relleno 4×8, con una flecha ˅; al hacer clic abre una lista como los menús desplegables de la Task 9. **kbd**: `mono_11`, relleno 1×5, radio 4, fondo `hover`, texto `text_2`. **link**: `ui_13` `accent`.
  Cada cambio se guarda en `config.toml` al instante (como ahora) y llama a `on_change`. «Editar el archivo» y «Abrir [keys]» abren `config.toml` como documento en la ventana principal (usa `on_change` con un aviso, o un callback nuevo `on_open_path`).
- [ ] **Step 3:** Captura de Ajustes (`-Keys '^,'`) y comparación con `ref/ajustes.png` (compara solo la ventana de Ajustes).
- [ ] **Step 4: Commit** `feat(ui): ventana de Ajustes dibujada como la maqueta`

---

### Task 12: Repaso final contra todas las referencias

- [ ] **Step 1:** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --release --workspace`: todo limpio.
- [ ] **Step 2:** Captura la app en **cada** escenario de `docs/mockups/ref/` que la app ya soporta (todos menos los que dependen de dos archivos abiertos a la vez; para esos, abre un segundo archivo con `^o`) y compáralas una a una con la referencia. Arregla lo que no coincida. Guarda las capturas finales en `docs/mockups/app/<escenario>.png` para que el usuario pueda revisarlas.
- [ ] **Step 3:** Prueba también con el escalado de Windows al 125 % o 150 % si puedes (o simula `dpi = 144` forzándolo temporalmente): nada borroso, nada descolocado.
- [ ] **Step 4: Commit** `chore(ui): repaso visual contra la maqueta` con, en el cuerpo del mensaje, la lista de escenarios comparados y cualquier diferencia que quede y por qué.

---

## Lo que el usuario tiene que revisar a mano al terminar

- Arrastrar la ventana por la barra de título, doble clic para maximizar, Snap Layouts al pasar por maximizar, redimensionar por los cuatro bordes, Win+flechas.
- Hover de pestañas, ✕, +, botones de la barra y lista de sugerencias.
- Cambiar el tema de Windows con la app abierta.
- Ajustes: cada control, y que los cambios se vean al instante en la ventana principal.
