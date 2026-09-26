//! Paleta de `docs/mockups/setup/instalador-flujo.html` e `instalador-opciones.html`
//! (mismos hex en ambos archivos): a diferencia de `notty_ui::theme`, que está
//! calibrada sobre la maqueta de la ventana principal, notty-setup necesita sus
//! propios valores porque la maqueta del instalador usa un fondo bastante más oscuro
//! (`#202124` frente al `#2b2c2e` aprox. de `notty-ui`).
//!
//! Solo existe la variante oscura: las dos maquetas del instalador solo muestran
//! ese tema. Para el modo claro (el spec dice "sigue el tema claro u oscuro del
//! sistema") se usa `notty_ui::theme::palette(false)` como aproximación razonable
//! hasta que exista una maqueta clara del instalador.
use notty_ui::Rgba;

const fn hex(rgb: u32) -> Rgba {
    let r = ((rgb >> 16) & 0xFF) as f32 / 255.0;
    let g = ((rgb >> 8) & 0xFF) as f32 / 255.0;
    let b = (rgb & 0xFF) as f32 / 255.0;
    Rgba(r, g, b, 1.0)
}

const fn hexa(rgb: u32, a: f32) -> Rgba {
    let Rgba(r, g, b, _) = hex(rgb);
    Rgba(r, g, b, a)
}

#[derive(Debug, Clone, Copy)]
pub struct SetupPalette {
    /// `.win` -- fondo general de la ventana.
    pub win: Rgba,
    /// `.win` -- borde de 1px.
    pub win_border: Rgba,
    /// `.tb` -- texto de la barra de título.
    pub titlebar_text: Rgba,
    /// `.foot` -- fondo del pie.
    pub foot: Rgba,
    /// `.foot` -- borde superior del pie / `.rail`/`.prev` bordes verticales.
    pub foot_border: Rgba,
    /// Texto principal (`color:#e5e6e8`).
    pub text: Rgba,
    /// Texto secundario (`.sub`, `#a9aaad`).
    pub text_2: Rgba,
    /// Texto terciario / hint (`#8a8b8e`).
    pub text_3: Rgba,
    /// Acento (`#73b6fa`).
    pub accent: Rgba,
    /// Borde del botón primario (`#8cc4fb`).
    pub accent_border: Rgba,
    /// Texto sobre el acento (botón primario, bolita del toggle encendido: `#0e1216`).
    pub on_accent: Rgba,
    /// `.btn2` fondo.
    pub btn2: Rgba,
    /// `.btn2` borde.
    pub btn2_border: Rgba,
    /// Línea/borde general (`#2a2b2f`, `.row`).
    pub line: Rgba,
    /// `.box.off` / toggle apagado (`#6b6c70`).
    pub off_border: Rgba,
    /// Bolita de un toggle apagado (`#c9cacd`).
    pub off_ball: Rgba,
    /// `.bar` fondo de la pista de progreso (`#34353a`).
    pub track: Rgba,
    /// `.grp` fondo (`#28292c`).
    pub group: Rgba,
    /// `.grp:hover` fondo (`#2c2d31`).
    pub group_hover: Rgba,
    /// `.grp` borde (`#303136`).
    pub group_border: Rgba,
    /// `.it:hover` fondo (`#2f3034`).
    pub item_hover: Rgba,
    /// `.prev`/`.canvas` fondo (`#1b1c1e` / `#141517`).
    pub preview_bg: Rgba,
    pub canvas_bg: Rgba,
    /// `.canvas` borde (`#2a2b2f`).
    pub canvas_border: Rgba,
    /// Punto del carril sin visitar (`#55565b`).
    pub rail_dot: Rgba,
    /// Halo del punto activo (`rgba(115,182,250,.18)`).
    pub rail_glow: Rgba,
}

pub const DARK: SetupPalette = SetupPalette {
    win: hex(0x202124),
    win_border: hex(0x34353a),
    titlebar_text: hex(0xa9aaad),
    foot: hex(0x1b1c1e),
    foot_border: hex(0x2c2d31),
    text: hex(0xe5e6e8),
    text_2: hex(0xa9aaad),
    text_3: hex(0x8a8b8e),
    accent: hex(0x73b6fa),
    accent_border: hex(0x8cc4fb),
    on_accent: hex(0x0e1216),
    btn2: hex(0x2d2e32),
    btn2_border: hex(0x3a3b40),
    line: hex(0x2a2b2f),
    off_border: hex(0x6b6c70),
    off_ball: hex(0xc9cacd),
    track: hex(0x34353a),
    group: hex(0x28292c),
    group_hover: hex(0x2c2d31),
    group_border: hex(0x303136),
    item_hover: hex(0x2f3034),
    preview_bg: hex(0x1b1c1e),
    canvas_bg: hex(0x141517),
    canvas_border: hex(0x2a2b2f),
    rail_dot: hex(0x55565b),
    rail_glow: hexa(0x73b6fa, 0.18),
};
