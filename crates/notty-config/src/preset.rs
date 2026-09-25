use crate::{MenuBar, Preset, TabsPosition, UiConfig};

/// Aplica las piezas fijas de un preset. No toca `theme`, `line_numbers` ni `files`:
/// son independientes del preset, tal como describe la spec.
pub fn apply_preset(ui: &mut UiConfig, preset: Preset) {
    ui.preset = preset;
    match preset {
        Preset::Moderna => {
            ui.tabs_position = TabsPosition::Title;
            ui.menubar = MenuBar::Hidden;
            ui.hints_bar = true;
            ui.status_bar = true;
        }
        Preset::Clasica => {
            ui.tabs_position = TabsPosition::Below;
            ui.menubar = MenuBar::Visible;
            ui.hints_bar = false;
            ui.status_bar = true;
        }
        Preset::Zen => {
            ui.tabs_position = TabsPosition::Auto;
            ui.menubar = MenuBar::Hidden;
            ui.hints_bar = false;
            ui.status_bar = true;
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
    }

    #[test]
    fn clasica_shows_menubar_and_tabs_below() {
        let mut ui = UiConfig::default();
        apply_preset(&mut ui, Preset::Clasica);
        assert_eq!(ui.tabs_position, TabsPosition::Below);
        assert_eq!(ui.menubar, MenuBar::Visible);
        assert!(!ui.hints_bar);
    }

    #[test]
    fn zen_hides_hints_and_shows_tabs_only_if_several() {
        let mut ui = UiConfig::default();
        apply_preset(&mut ui, Preset::Zen);
        assert_eq!(ui.tabs_position, TabsPosition::Auto);
        assert_eq!(ui.menubar, MenuBar::Hidden);
        assert!(!ui.hints_bar);
    }

    #[test]
    fn applying_a_preset_does_not_touch_theme_or_line_numbers() {
        let mut ui = UiConfig { theme: crate::Theme::Dark, line_numbers: false, ..UiConfig::default() };
        apply_preset(&mut ui, Preset::Clasica);
        assert_eq!(ui.theme, crate::Theme::Dark);
        assert!(!ui.line_numbers);
    }

    #[test]
    fn preset_field_itself_is_updated() {
        let mut ui = UiConfig::default();
        apply_preset(&mut ui, Preset::Zen);
        assert_eq!(ui.preset, Preset::Zen);
    }
}
