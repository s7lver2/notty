#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureToggles {
    pub notepad_replace: bool,
    pub context_menu: bool,
    pub path_env: bool,
    pub assoc_text: bool,
    pub assoc_config: bool,
    pub assoc_markdown: bool,
    pub assoc_code: bool,
    pub start_menu: bool,
    pub desktop: bool,
}

impl Default for FeatureToggles {
    fn default() -> Self {
        Self {
            notepad_replace: true,
            context_menu: true,
            path_env: true,
            assoc_text: true,
            assoc_config: true,
            assoc_markdown: true,
            assoc_code: false,
            start_menu: true,
            desktop: false,
        }
    }
}

pub fn to_addlocal(t: &FeatureToggles) -> String {
    let mut features = vec!["Core"];
    if t.notepad_replace { features.push("NotepadReplace"); }
    if t.context_menu { features.push("ContextMenu"); }
    if t.path_env { features.push("PathEnv"); }
    if t.assoc_text { features.push("AssocText"); }
    if t.assoc_config { features.push("AssocConfig"); }
    if t.assoc_markdown { features.push("AssocMarkdown"); }
    if t.assoc_code { features.push("AssocCode"); }
    if t.start_menu { features.push("StartMenu"); }
    if t.desktop { features.push("Desktop"); }
    features.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec_table() {
        let t = FeatureToggles::default();
        assert!(t.notepad_replace && t.context_menu && t.path_env);
        assert!(t.assoc_text && t.assoc_config && t.assoc_markdown);
        assert!(!t.assoc_code && !t.desktop);
        assert!(t.start_menu);
    }

    #[test]
    fn addlocal_all_default() {
        let t = FeatureToggles::default();
        assert_eq!(
            to_addlocal(&t),
            "Core,NotepadReplace,ContextMenu,PathEnv,AssocText,AssocConfig,AssocMarkdown,StartMenu"
        );
    }

    #[test]
    fn addlocal_minimal() {
        let t = FeatureToggles {
            notepad_replace: false, context_menu: false, path_env: false,
            assoc_text: false, assoc_config: false, assoc_markdown: false,
            assoc_code: false, start_menu: false, desktop: false,
        };
        assert_eq!(to_addlocal(&t), "Core");
    }

    #[test]
    fn addlocal_everything() {
        let t = FeatureToggles {
            notepad_replace: true, context_menu: true, path_env: true,
            assoc_text: true, assoc_config: true, assoc_markdown: true,
            assoc_code: true, start_menu: true, desktop: true,
        };
        assert_eq!(
            to_addlocal(&t),
            "Core,NotepadReplace,ContextMenu,PathEnv,AssocText,AssocConfig,AssocMarkdown,AssocCode,StartMenu,Desktop"
        );
    }
}
