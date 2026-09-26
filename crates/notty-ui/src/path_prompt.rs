#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Open,
    Save,
}

/// Filas visibles a la vez en la caja de sugerencias; con más candidatos que esto
/// (típico al filtrar con comodines, `*.py`) se navega con el scroll o las flechas.
pub const VISIBLE_SUGGESTIONS: usize = 5;
/// Cuántos candidatos se piden como máximo a `notty_io::suggestions`. Antes eran 5
/// (igual que lo visible): un comodín como `*.py` con más de 5 aciertos escondía el
/// resto sin ninguna forma de llegar a ellos.
const FETCH_MAX: usize = 50;

pub struct PathPromptState {
    pub value: String,
    pub purpose: Purpose,
    pub selected: usize,
    /// Primer candidato visible en la caja (el resto de `VISIBLE_SUGGESTIONS - 1`
    /// siguen). El scroll del ratón lo mueve directamente; moverse con flechas o Tab
    /// lo arrastra lo mínimo para que `selected` se mantenga visible.
    pub scroll: usize,
    /// Último segmento tecleado por el usuario antes de que `accept()` lo completara
    /// con una sugerencia. Se conserva para que pulsar `Tab` otra vez (sin volver a
    /// teclear) siga comparando candidatos contra lo que el usuario escribió de verdad,
    /// en vez de contra el segmento vacío que queda tras añadir la `\` final.
    pending_prefix: Option<String>,
    /// Por qué falló el último intento de aceptar esta ruta (permisos, disco lleno,
    /// carpeta que no se pudo crear...). Se enseña en rojo en vez de la palabra de
    /// estado normal; nunca se cierra el prompt sin avisar de por qué no pasó nada.
    pub last_error: Option<String>,
    /// Ctrl+A: todo el valor está seleccionado; lo siguiente que se teclee, pegue o
    /// borre lo sustituye entero.
    pub all_selected: bool,
    /// Guardar un documento nuevo sobre una ruta que ya existe: en vez de escribir
    /// encima, se pregunta (sobrescribir / abrir el existente / cancelar).
    pub ask_overwrite: bool,
    undo_stack: Vec<String>,
    /// Historial de rutas usadas: ordena las sugerencias por uso frecuente y reciente.
    usage: notty_io::Usage,
}

/// Qué hacer cuando un documento nuevo se guardaría encima de un archivo existente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwriteChoice {
    Overwrite,
    OpenExisting,
    Cancel,
}

impl OverwriteChoice {
    /// Tecla de la pregunta de sobrescritura: S/Enter sobrescribe, A abre el
    /// existente, C/Esc cancela. Cualquier otra tecla no responde.
    pub fn from_vk(vk: u32) -> Option<Self> {
        match vk {
            0x53 | 0x0D => Some(Self::Overwrite),
            0x41 => Some(Self::OpenExisting),
            0x43 | 0x1B => Some(Self::Cancel),
            _ => None,
        }
    }
}

pub const OVERWRITE_QUESTION: &str = "Ya existe · S sobrescribir · A abrirlo · C cancelar";

const UNDO_MAX: usize = 100;

/// Limpia lo que llega del portapapeles para usarlo como ruta: primera línea no
/// vacía, sin espacios alrededor ni las comillas de "Copiar como ruta" del Explorador.
pub fn clean_pasted_path(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    line.trim_matches('"').trim().to_string()
}

fn is_absolute_path(s: &str) -> bool {
    let b = s.as_bytes();
    s.starts_with(r"\\") || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
}

impl PathPromptState {
    /// Si no hay una ruta inicial que reutilizar (guardar desde CLICKME, o abrir sin
    /// un documento ya abierto), la línea de ruta arranca en `C:\` en vez de vacía,
    /// para que las sugerencias aparezcan al instante sin tener que teclear nada.
    pub fn new(purpose: Purpose, initial: String) -> Self {
        let value = if initial.is_empty() { r"C:\".to_string() } else { initial };
        Self {
            value,
            purpose,
            selected: 0,
            scroll: 0,
            pending_prefix: None,
            last_error: None,
            all_selected: false,
            ask_overwrite: false,
            undo_stack: Vec::new(),
            usage: notty_io::Usage::load(),
        }
    }

    pub fn type_text(&mut self, raw: &str, ctx: &notty_io::PathContext) {
        self.value = notty_io::normalize(raw, ctx);
        self.selected = 0;
        self.scroll = 0;
        self.pending_prefix = None;
        self.last_error = None;
        self.all_selected = false;
        self.ask_overwrite = false;
    }

    /// Como `type_text`, pero guardando el valor anterior para Ctrl+Z.
    fn edit(&mut self, raw: &str, ctx: &notty_io::PathContext) {
        if self.undo_stack.last() != Some(&self.value) {
            self.undo_stack.push(self.value.clone());
            if self.undo_stack.len() > UNDO_MAX {
                self.undo_stack.remove(0);
            }
        }
        self.type_text(raw, ctx);
    }

    pub fn type_char(&mut self, ch: char, ctx: &notty_io::PathContext) {
        let raw = if self.all_selected { ch.to_string() } else { format!("{}{ch}", self.value) };
        self.edit(&raw, ctx);
    }

    pub fn backspace(&mut self, ctx: &notty_io::PathContext) {
        let raw = if self.all_selected {
            String::new()
        } else {
            let mut v = self.value.clone();
            v.pop();
            v
        };
        self.edit(&raw, ctx);
    }

    pub fn select_all(&mut self) {
        self.all_selected = !self.value.is_empty();
    }

    /// Ctrl+C: la ruta entera (no hay selección parcial en esta línea).
    pub fn copy_text(&self) -> String {
        self.value.clone()
    }

    /// Ctrl+X: devuelve la ruta y vacía el campo.
    pub fn cut(&mut self, ctx: &notty_io::PathContext) -> String {
        let out = self.value.clone();
        self.edit("", ctx);
        out
    }

    /// Ctrl+V: una ruta absoluta (o con todo seleccionado) sustituye el valor; un
    /// trozo relativo se añade al final, como si se hubiera tecleado.
    pub fn paste(&mut self, text: &str, ctx: &notty_io::PathContext) {
        let clean = clean_pasted_path(text);
        if clean.is_empty() {
            return;
        }
        let raw = if self.all_selected || is_absolute_path(&clean) { clean } else { format!("{}{clean}", self.value) };
        self.edit(&raw, ctx);
    }

    pub fn undo(&mut self, ctx: &notty_io::PathContext) {
        if let Some(prev) = self.undo_stack.pop() {
            self.type_text(&prev, ctx);
        }
    }

    pub fn suggestions(&self) -> Vec<notty_io::Entry> {
        notty_io::ranked_suggestions(&self.value, FETCH_MAX, &self.usage)
    }

    /// El tramo de `VISIBLE_SUGGESTIONS` candidatos que toca dibujar, ya recortado a
    /// partir de `scroll`, junto con el total real (para el indicador "N más").
    pub fn visible_suggestions(&self) -> (Vec<notty_io::Entry>, usize) {
        let all = self.suggestions();
        let total = all.len();
        let start = self.scroll.min(total.saturating_sub(VISIBLE_SUGGESTIONS.min(total)));
        let end = (start + VISIBLE_SUGGESTIONS).min(total);
        (all.into_iter().skip(start).take(end - start).collect(), total)
    }

    /// Scroll del ratón sobre la caja de sugerencias: mueve la ventana visible sin
    /// tocar `selected` (como una lista normal de Explorador).
    pub fn scroll_by(&mut self, delta: i32) {
        let total = self.suggestions().len();
        if total <= VISIBLE_SUGGESTIONS {
            self.scroll = 0;
            return;
        }
        let max_scroll = total - VISIBLE_SUGGESTIONS;
        self.scroll = (self.scroll as i64 + delta as i64).clamp(0, max_scroll as i64) as usize;
    }

    /// Arrastra `scroll` lo mínimo para que `selected` quede dentro de la ventana
    /// visible, tras moverla con flechas o Tab.
    fn ensure_selected_visible(&mut self) {
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + VISIBLE_SUGGESTIONS {
            self.scroll = self.selected + 1 - VISIBLE_SUGGESTIONS;
        }
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
        self.ensure_selected_visible();
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
        let sugs = notty_io::ranked_suggestions(&typed, FETCH_MAX, &self.usage);
        if sugs.is_empty() {
            return;
        }
        let exact_match = last == sugs[self.selected.min(sugs.len() - 1)].name;
        if (advancing || exact_match) && sugs.len() > 1 {
            self.selected = (self.selected + 1) % sugs.len();
        }
        self.ensure_selected_visible();
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
    fn select_all_then_typing_replaces_everything() {
        let c = ctx(&PathBuf::from("."));
        let mut p = PathPromptState::new(Purpose::Save, r"C:\algo\viejo.txt".to_string());
        p.select_all();
        p.type_char('D', &c);
        assert_eq!(p.value, "D");
        assert!(!p.all_selected);
    }

    #[test]
    fn select_all_then_backspace_clears() {
        let c = ctx(&PathBuf::from("."));
        let mut p = PathPromptState::new(Purpose::Save, r"C:\algo".to_string());
        p.select_all();
        p.backspace(&c);
        assert_eq!(p.value, "");
    }

    #[test]
    fn pasting_an_absolute_path_replaces_and_strips_quotes() {
        let c = ctx(&PathBuf::from("."));
        let mut p = PathPromptState::new(Purpose::Open, String::new());
        p.paste("\"D:\\docs\\notas.txt\"\r\n", &c);
        assert_eq!(p.value, r"D:\docs\notas.txt");
    }

    #[test]
    fn pasting_a_relative_piece_appends() {
        let c = ctx(&PathBuf::from("."));
        let mut p = PathPromptState::new(Purpose::Open, r"C:\docs\".to_string());
        p.paste("notas.txt", &c);
        assert_eq!(p.value, r"C:\docs\notas.txt");
    }

    #[test]
    fn cut_empties_and_undo_restores() {
        let c = ctx(&PathBuf::from("."));
        let mut p = PathPromptState::new(Purpose::Open, r"C:\docs\a.txt".to_string());
        assert_eq!(p.cut(&c), r"C:\docs\a.txt");
        assert_eq!(p.value, "");
        p.undo(&c);
        assert_eq!(p.value, r"C:\docs\a.txt");
    }

    #[test]
    fn overwrite_keys_map_to_choices() {
        assert_eq!(OverwriteChoice::from_vk(0x53), Some(OverwriteChoice::Overwrite));
        assert_eq!(OverwriteChoice::from_vk(0x41), Some(OverwriteChoice::OpenExisting));
        assert_eq!(OverwriteChoice::from_vk(0x1B), Some(OverwriteChoice::Cancel));
        assert_eq!(OverwriteChoice::from_vk(0x51), None);
    }

    #[test]
    fn typing_again_clears_the_last_error() {
        let mut p = PathPromptState::new(Purpose::Save, String::new());
        p.last_error = Some("sin permisos".to_string());
        p.type_text("a", &ctx(&std::path::PathBuf::from(".")));
        assert!(p.last_error.is_none());
    }

    #[test]
    fn empty_initial_value_defaults_to_c_drive() {
        let p = PathPromptState::new(Purpose::Open, String::new());
        assert_eq!(p.value, r"C:\");
    }

    #[test]
    fn non_empty_initial_value_is_kept_as_is() {
        let p = PathPromptState::new(Purpose::Save, r"D:\notas.txt".to_string());
        assert_eq!(p.value, r"D:\notas.txt");
    }

    #[test]
    fn wheel_scroll_moves_the_window_without_touching_selection() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..8 {
            std::fs::write(dir.path().join(format!("f{i}.py")), "x").unwrap();
        }
        let mut p = PathPromptState::new(Purpose::Open, String::new());
        p.type_text(&format!("{}\\*.py", dir.path().display()), &ctx(dir.path()));
        let (_, total) = p.visible_suggestions();
        assert_eq!(total, 8);
        assert_eq!(p.scroll, 0);
        p.scroll_by(1);
        assert_eq!(p.scroll, 1);
        assert_eq!(p.selected, 0); // el scroll no mueve la selección
        p.scroll_by(100);
        assert_eq!(p.scroll, 8 - VISIBLE_SUGGESTIONS); // se clampa al final de la lista
        p.scroll_by(-100);
        assert_eq!(p.scroll, 0);
    }

    #[test]
    fn moving_selection_past_the_window_scrolls_to_follow_it() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..8 {
            std::fs::write(dir.path().join(format!("f{i}.py")), "x").unwrap();
        }
        let mut p = PathPromptState::new(Purpose::Open, String::new());
        p.type_text(&format!("{}\\*.py", dir.path().display()), &ctx(dir.path()));
        for _ in 0..VISIBLE_SUGGESTIONS {
            p.move_selection(1);
        }
        assert_eq!(p.selected, VISIBLE_SUGGESTIONS);
        assert!(p.scroll > 0, "scroll debería haber seguido a la selección");
        let (visible, _) = p.visible_suggestions();
        assert!(p.selected < p.scroll + visible.len());
        assert!(p.selected >= p.scroll);
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
