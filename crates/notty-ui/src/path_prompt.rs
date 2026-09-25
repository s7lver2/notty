#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Open,
    Save,
}

pub struct PathPromptState {
    pub value: String,
    pub purpose: Purpose,
    pub selected: usize,
    /// Último segmento tecleado por el usuario antes de que `accept()` lo completara
    /// con una sugerencia. Se conserva para que pulsar `Tab` otra vez (sin volver a
    /// teclear) siga comparando candidatos contra lo que el usuario escribió de verdad,
    /// en vez de contra el segmento vacío que queda tras añadir la `\` final.
    pending_prefix: Option<String>,
}

impl PathPromptState {
    pub fn new(purpose: Purpose, initial: String) -> Self {
        Self { value: initial, purpose, selected: 0, pending_prefix: None }
    }

    pub fn type_text(&mut self, raw: &str, ctx: &notty_io::PathContext) {
        self.value = notty_io::normalize(raw, ctx);
        self.selected = 0;
        self.pending_prefix = None;
    }

    pub fn suggestions(&self) -> Vec<notty_io::Entry> {
        notty_io::suggestions(&self.value, 5)
    }

    pub fn hint(&self) -> notty_io::Hint {
        notty_io::hint_for(&self.value)
    }

    /// La palabra de estado que enseña la maqueta a la derecha del campo de ruta.
    pub fn hint_word(&self) -> &'static str {
        if self.is_invalid() {
            return "no válido";
        }
        match self.hint() {
            notty_io::Hint::Empty => "",
            notty_io::Hint::New => "nuevo",
            notty_io::Hint::Exists => "existe",
            notty_io::Hint::Dir => "carpeta",
            notty_io::Hint::DirNew => "carpeta nueva",
        }
    }

    fn last_segment(&self) -> &str {
        self.value.rsplit('\\').next().unwrap_or(&self.value)
    }

    pub fn is_invalid(&self) -> bool {
        notty_io::has_invalid_chars(self.last_segment())
    }

    pub fn ghost(&self) -> String {
        let sugs = self.suggestions();
        if !sugs.is_empty() {
            let idx = self.selected.min(sugs.len() - 1);
            let chosen = &sugs[idx];
            let rest = chosen.name.strip_prefix(self.last_segment()).unwrap_or("");
            return format!("{rest}{}", if chosen.is_dir { "\\" } else { "" });
        }
        let has_ext = self.last_segment().rsplit_once('.').is_some();
        match self.hint() {
            notty_io::Hint::New | notty_io::Hint::DirNew if !has_ext && !self.last_segment().is_empty() => ".txt".to_string(),
            _ => String::new(),
        }
    }

    pub fn move_selection(&mut self, delta: i32) {
        let n = self.suggestions().len();
        if n == 0 {
            return;
        }
        self.selected = ((self.selected as i64 + delta as i64).rem_euclid(n as i64)) as usize;
    }

    pub fn accept(&mut self) {
        // Si el último `accept()` ya completó un segmento (el valor termina en `\`
        // tras añadir una sugerencia), recuperamos el prefijo original que el usuario
        // tecleó para poder seguir comparando candidatos contra él.
        let advancing = self.pending_prefix.is_some();
        let (base, last) = match &self.pending_prefix {
            Some(prefix) => {
                let base = match self.value.strip_suffix('\\').and_then(|v| v.rfind('\\')) {
                    Some(i) => self.value[..=i].to_string(),
                    None => String::new(),
                };
                (base, prefix.clone())
            }
            None => {
                let last = self.last_segment().to_string();
                let base = self.value[..self.value.len() - last.len()].to_string();
                (base, last)
            }
        };
        let typed = format!("{base}{last}");
        let sugs = notty_io::suggestions(&typed, 5);
        if sugs.is_empty() {
            return;
        }
        let exact_match = last == sugs[self.selected.min(sugs.len() - 1)].name;
        if (advancing || exact_match) && sugs.len() > 1 {
            self.selected = (self.selected + 1) % sugs.len();
        }
        let chosen = &sugs[self.selected.min(sugs.len() - 1)];
        self.pending_prefix = Some(last);
        self.value = format!("{base}{}{}", chosen.name, if chosen.is_dir { "\\" } else { "" });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx(dir: &std::path::Path) -> notty_io::PathContext {
        notty_io::PathContext { home: dir.to_path_buf(), current_dir: None }
    }

    fn setup() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("proyectos")).unwrap();
        std::fs::create_dir(dir.path().join("proyectos-viejos")).unwrap();
        std::fs::write(dir.path().join("presupuesto.txt"), "x").unwrap();
        dir
    }

    #[test]
    fn typing_normalizes_and_resets_selection() {
        let mut p = PathPromptState::new(Purpose::Open, String::new());
        p.selected = 2;
        p.type_text("a/b", &ctx(&PathBuf::from(".")));
        assert_eq!(p.value, r"a\b");
        assert_eq!(p.selected, 0);
    }

    #[test]
    fn ghost_shows_rest_of_top_suggestion() {
        let dir = setup();
        let mut p = PathPromptState::new(Purpose::Open, String::new());
        p.type_text(&format!("{}\\pro", dir.path().display()), &ctx(dir.path()));
        assert_eq!(p.ghost(), r"yectos\");
    }

    #[test]
    fn arrow_moves_selection_and_changes_ghost() {
        let dir = setup();
        let mut p = PathPromptState::new(Purpose::Open, String::new());
        p.type_text(&format!("{}\\pro", dir.path().display()), &ctx(dir.path()));
        p.move_selection(1);
        assert_eq!(p.ghost(), r"yectos-viejos\");
    }

    #[test]
    fn accept_appends_the_selected_suggestion() {
        let dir = setup();
        let mut p = PathPromptState::new(Purpose::Open, String::new());
        p.type_text(&format!("{}\\pro", dir.path().display()), &ctx(dir.path()));
        p.accept();
        assert_eq!(p.value, format!("{}\\proyectos\\", dir.path().display()));
    }

    #[test]
    fn accepting_twice_cycles_to_next_sibling() {
        let dir = setup();
        let mut p = PathPromptState::new(Purpose::Open, String::new());
        p.type_text(&format!("{}\\pro", dir.path().display()), &ctx(dir.path()));
        p.accept();
        p.accept();
        assert_eq!(p.value, format!("{}\\proyectos-viejos\\", dir.path().display()));
    }

    #[test]
    fn hint_new_gets_default_extension_ghost_when_no_candidates() {
        let dir = setup();
        let mut p = PathPromptState::new(Purpose::Save, String::new());
        p.type_text(&format!("{}\\idea", dir.path().display()), &ctx(dir.path()));
        assert_eq!(p.hint(), notty_io::Hint::New);
        assert_eq!(p.ghost(), ".txt");
    }

    #[test]
    fn invalid_last_segment_is_detected() {
        let mut p = PathPromptState::new(Purpose::Save, String::new());
        p.type_text("a\\nota?.txt", &ctx(&PathBuf::from(".")));
        assert!(p.is_invalid());
    }

    #[test]
    fn hint_word_matches_hint_and_invalid_wins() {
        let dir = setup();
        let mut p = PathPromptState::new(Purpose::Save, String::new());
        p.type_text(&format!("{}\\presupuesto.txt", dir.path().display()), &ctx(dir.path()));
        assert_eq!(p.hint_word(), "existe");

        p.type_text(&format!("{}\\idea.txt", dir.path().display()), &ctx(dir.path()));
        assert_eq!(p.hint_word(), "nuevo");

        p.type_text(&format!("{}\\proyectos", dir.path().display()), &ctx(dir.path()));
        assert_eq!(p.hint_word(), "carpeta");

        p.type_text("a\\nota?.txt", &ctx(&PathBuf::from(".")));
        assert_eq!(p.hint_word(), "no válido");
    }
}
