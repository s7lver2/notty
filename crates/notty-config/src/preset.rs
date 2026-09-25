use crate::{MenuBar, Preset, TabsPosition, UiConfig};

/// Aplica las piezas fijas de un preset. No toca `theme` ni `files`: son
/// independientes del preset, tal como describe la spec. La maqueta define
/// Moderna y Clásica con números de línea y sin línea de comandos fusionada;
/// Zen sin números de línea y con la línea de comandos fusionada (estado y
/// prompts en una sola franja).
/// `Custom` no es un preset "aplicable": no lo pasa nadie a propósito (lo pone
/// `settings_model::apply` cuando se toca una pieza suelta que ya no coincide con
/// ninguno de los tres de abajo), así que aquí no hace nada más que marcarlo.
pub fn apply_preset(ui: &mut UiConfig, preset: Preset) {
    ui.preset = preset;
    match preset {
        Preset::Custom => {}
        Preset::Moderna => {
            ui.tabs_position = TabsPosition::Title;
            ui.menubar = MenuBar::Hidden;
            ui.hints_bar = true;
            ui.status_bar = true;
            ui.line_numbers = true;
            ui.merged_command_line = false;
        }
        Preset::Clasica => {
            ui.tabs_position = TabsPosition::Below;
            ui.menubar = MenuBar::Visible;
            ui.hints_bar = false;
            ui.status_bar = true;
            ui.line_numbers = true;
            ui.merged_command_line = false;
        }
        Preset::Zen => {
            ui.tabs_position = TabsPosition::Auto;
            ui.menubar = MenuBar::Hidden;
            ui.hints_bar = false;
            ui.status_bar = true;
            ui.line_numbers = false;
            ui.merged_command_line = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MenuBar, TabsPosition, UiConfig};

    #[test]
    fn moderna_has_tabs_in_title_and_no_menubar() {
        let mut ui = UiConfig::default();
        apply_preset(&mut ui, Preset::Moderna);
        assert_eq!(ui.tabs_position, TabsPosition::Title);
        assert_eq!(ui.menubar, MenuBar::Hidden);
        assert!(ui.hints_bar);
        assert!(ui.line_numbers);
        assert!(!ui.merged_command_line);
    }

    #[test]
    fn clasica_shows_menubar_and_tabs_below() {
        let mut ui = UiConfig::default();
        apply_preset(&mut ui, Preset::Clasica);
        assert_eq!(ui.tabs_position, TabsPosition::Below);
        assert_eq!(ui.menubar, MenuBar::Visible);
        assert!(!ui.hints_bar);
        assert!(ui.line_numbers);
        assert!(!ui.merged_command_line);
    }

    #[test]
    fn zen_hides_hints_and_shows_tabs_only_if_several() {
        let mut ui = UiConfig::default();
        apply_preset(&mut ui, Preset::Zen);
        assert_eq!(ui.tabs_position, TabsPosition::Auto);
        assert_eq!(ui.menubar, MenuBar::Hidden);
        assert!(!ui.hints_bar);
        assert!(!ui.line_numbers);
        assert!(ui.merged_command_line);
    }

    #[test]
    fn applying_a_preset_does_not_touch_theme() {
        let mut ui = UiConfig { theme: crate::Theme::Dark, ..UiConfig::default() };
        apply_preset(&mut ui, Preset::Clasica);
        assert_eq!(ui.theme, crate::Theme::Dark);
    }

    #[test]
    fn preset_field_itself_is_updated() {
        let mut ui = UiConfig::default();
        apply_preset(&mut ui, Preset::Zen);
        assert_eq!(ui.preset, Preset::Zen);
    }
}
