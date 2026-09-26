//! Paleta de la maqueta (`docs/mockups/notty-ui.html`, bloques `:root[data-theme=...]`),
//! convertida de OKLCH a sRGB. Cada campo se llama como su variable CSS.

use notty_config::Theme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba(pub f32, pub f32, pub f32, pub f32);

impl Rgba {
    /// Mismo color con la opacidad multiplicada por `factor` (`0.0..=1.0`), para
    /// fundidos: `Rgba(r,g,b,a).faded(0.4)` da `Rgba(r,g,b,a*0.4)`.
    pub fn faded(self, factor: f32) -> Self {
        Self(self.0, self.1, self.2, self.3 * factor.clamp(0.0, 1.0))
    }

    pub fn mix(self, to: Rgba, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self(
            self.0 + (to.0 - self.0) * t,
            self.1 + (to.1 - self.1) * t,
            self.2 + (to.2 - self.2) * t,
            self.3 + (to.3 - self.3) * t,
        )
    }
}

impl Palette {
    /// Paleta intermedia entre `self` y `to`: el fundido al cambiar de tema.
    pub fn mix(&self, to: &Palette, t: f32) -> Palette {
        macro_rules! m {
            ($($f:ident),*) => { Palette { $($f: self.$f.mix(to.$f, t)),* } };
        }
        m!(
            chrome, chrome_hi, surface, surface_2, cmd, hover, press, line, text, text_2, text_3, text_hint, accent,
            accent_soft, on_accent, danger, warn, ok, mark, mark_cur, close_hover, close_hover_fg, shadow, shadow_ring
        )
    }
}

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
    fn faded_scales_alpha_only() {
        let c = Rgba(1.0, 1.0, 1.0, 1.0);
        assert_eq!(c.faded(0.5), Rgba(1.0, 1.0, 1.0, 0.5));
        assert_eq!(c.faded(1.0), c);
    }

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
    #[allow(clippy::assertions_on_constants)]
    fn dark_surface_is_darker_than_chrome() {
        // En la maqueta el editor (--surface) es más oscuro que la barra de título (--chrome).
        assert!(DARK.surface.0 < DARK.chrome.0);
        assert!(LIGHT.surface.0 > LIGHT.chrome.0);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn text_hierarchy_is_ordered() {
        assert!(DARK.text.0 > DARK.text_2.0 && DARK.text_2.0 > DARK.text_3.0);
        assert!(LIGHT.text.0 < LIGHT.text_2.0 && LIGHT.text_2.0 < LIGHT.text_3.0);
    }
}
