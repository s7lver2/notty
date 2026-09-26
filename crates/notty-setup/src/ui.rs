//! Estado y dibujo de `notty-setup`, siguiendo el patrón de `notty_ui::settings_window`
//! (misma técnica de ventana propia + Direct2D) pero con la paleta y el guion de
//! `docs/mockups/setup/instalador-flujo.html` e `instalador-opciones.html`.
use std::path::PathBuf;
use std::time::{Duration, Instant};

use notty_ui::layout::{Rect, TITLEBAR_H};
use notty_ui::{Anim, Renderer, Rgba};

use crate::features::FeatureToggles;
use crate::msi_driver::InstallEvent;
use crate::palette::{self, SetupPalette};

pub const WIN_W: f32 = 760.0;
pub const WIN_H: f32 = 480.0;
pub const RAIL_W: f32 = 168.0;
pub const PREVIEW_W: f32 = 236.0;
const CLOSE_W: f32 = 46.0;
const FOOT_H: f32 = 56.0;

const STEP_OUT_MS: u64 = 140;
const STEP_IN_MS: u64 = 180;
const EXPANDER_MS: u64 = 260;

/// Las cuatro pantallas del asistente normal (Task 6). El modo `--update` (Task 7)
/// no pasa por aquí: usa su propio bucle de pintado reducido en `main.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Options,
    Installing,
    Done,
}

impl Step {
    pub fn index(self) -> usize {
        match self {
            Step::Welcome => 0,
            Step::Options => 1,
            Step::Installing => 2,
            Step::Done => 3,
        }
    }
}

const STEP_LABELS: [&str; 4] = ["Bienvenida", "Opciones", "Instalar", "Listo"];

/// Una fila de opción dentro de un grupo (`.it` en la maqueta): título, descripción
/// opcional, el campo de `FeatureToggles` que gobierna, y la escena de vista previa
/// que ilumina al pasar el ratón.
struct OptionRow {
    title: &'static str,
    desc: &'static str,
    get: fn(&FeatureToggles) -> bool,
    set: fn(&mut FeatureToggles, bool),
    scene: Scene,
    caption: &'static str,
}

struct Group {
    title: &'static str,
    desc: &'static str,
    rows: &'static [usize],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    Notepad,
    Ctx,
    Path,
    Assoc,
    Start,
}

fn option_rows() -> [OptionRow; 9] {
    [
        OptionRow {
            title: "Sustituir el Bloc de notas",
            desc: "\"notepad\" abre notty · el original queda como notepad_legacy",
            get: |t| t.notepad_replace,
            set: |t, v| t.notepad_replace = v,
            scene: Scene::Notepad,
            caption: "Escribir \"notepad\" en cualquier sitio abrirá notty. El Bloc de notas original sigue disponible.",
        },
        OptionRow {
            title: "\"Abrir con notty\" en el menú contextual",
            desc: "",
            get: |t| t.context_menu,
            set: |t, v| t.context_menu = v,
            scene: Scene::Ctx,
            caption: "Clic derecho sobre cualquier archivo → Abrir con notty. En Windows 11, dentro de \"Mostrar más opciones\".",
        },
        OptionRow {
            title: "Usar notty desde la terminal",
            desc: "Añade notty al PATH",
            get: |t| t.path_env,
            set: |t, v| t.path_env = v,
            scene: Scene::Path,
            caption: "Escribe notty seguido de un archivo desde cmd o PowerShell.",
        },
        OptionRow {
            title: "Texto plano",
            desc: ".txt · .log · .text",
            get: |t| t.assoc_text,
            set: |t, v| t.assoc_text = v,
            scene: Scene::Assoc,
            caption: "notty aparecerá en \"Abrir con\" para estos tipos. Al terminar podrás elegirlo como predeterminado.",
        },
        OptionRow {
            title: "Configuración",
            desc: ".ini · .cfg · .conf · .toml",
            get: |t| t.assoc_config,
            set: |t, v| t.assoc_config = v,
            scene: Scene::Assoc,
            caption: "notty aparecerá en \"Abrir con\" para estos tipos. Al terminar podrás elegirlo como predeterminado.",
        },
        OptionRow {
            title: "Markdown",
            desc: ".md · .markdown",
            get: |t| t.assoc_markdown,
            set: |t, v| t.assoc_markdown = v,
            scene: Scene::Assoc,
            caption: "notty aparecerá en \"Abrir con\" para estos tipos. Al terminar podrás elegirlo como predeterminado.",
        },
        OptionRow {
            title: "Código y datos",
            desc: ".json · .xml · .csv · .yaml",
            get: |t| t.assoc_code,
            set: |t, v| t.assoc_code = v,
            scene: Scene::Assoc,
            caption: "notty aparecerá en \"Abrir con\" para estos tipos. Al terminar podrás elegirlo como predeterminado.",
        },
        OptionRow {
            title: "Menú Inicio",
            desc: "",
            get: |t| t.start_menu,
            set: |t, v| t.start_menu = v,
            scene: Scene::Start,
            caption: "notty aparece en la lista de aplicaciones del menú Inicio.",
        },
        OptionRow {
            title: "Escritorio",
            desc: "",
            get: |t| t.desktop,
            set: |t, v| t.desktop = v,
            scene: Scene::Start,
            caption: "notty aparece en la lista de aplicaciones del menú Inicio.",
        },
    ]
}

const GROUPS: [Group; 4] = [
    Group { title: "Integración con Windows", desc: "Bloc de notas, menú contextual, terminal", rows: &[0, 1, 2] },
    Group { title: "Tipos de archivo", desc: "Qué archivos ofrece abrir notty", rows: &[3, 4, 5, 6] },
    Group { title: "Accesos directos", desc: "", rows: &[7, 8] },
    Group { title: "Avanzado", desc: "Carpeta de instalación", rows: &[] },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hit {
    #[default]
    None,
    Caption,
    Close,
    Back,
    Next,
    GroupHeader(usize),
    Toggle(usize),
    OpenDefaultApps,
    OpenNotty,
    Retry,
}

pub enum InstallPhase {
    Running { progress: u8, progress_anim: Option<(Anim, u8, u8)>, action_text: String, action_anim: Option<Anim> },
    UacCancelled,
    Busy,
    Error(i32, String),
    Done,
}

#[derive(Clone, Copy)]
pub struct GroupState {
    pub open: bool,
    pub anim: Option<Anim>,
}

pub struct State {
    pub renderer: Renderer,
    pub animations_enabled: bool,
    pub anim_timer_running: bool,
    pub step: Step,
    pub toggles: FeatureToggles,
    pub install_folder: PathBuf,
    pub hover: Hit,
    pub hits: Vec<(Rect, Hit)>,
    pub groups: [GroupState; 4],
    pub transition: Option<Transition>,
    pub install: InstallPhase,
    pub check_anim: Option<Anim>,
    pub relaunch_after_done: bool,
}

#[derive(Clone, Copy)]
pub struct Transition {
    pub from: Step,
    pub start: Instant,
}

impl State {
    pub fn new(renderer: Renderer, animations_enabled: bool) -> Self {
        Self {
            renderer,
            animations_enabled,
            anim_timer_running: false,
            step: Step::Welcome,
            toggles: FeatureToggles::default(),
            install_folder: PathBuf::from(r"C:\Program Files\notty"),
            hover: Hit::None,
            hits: Vec::new(),
            groups: [
                GroupState { open: true, anim: None },
                GroupState { open: false, anim: None },
                GroupState { open: false, anim: None },
                GroupState { open: false, anim: None },
            ],
            transition: None,
            install: InstallPhase::Running {
                progress: 0,
                progress_anim: None,
                action_text: String::new(),
                action_anim: None,
            },
            check_anim: None,
            relaunch_after_done: false,
        }
    }

    pub fn go_to(&mut self, step: Step) {
        if step == self.step {
            return;
        }
        self.transition = Some(Transition { from: self.step, start: Instant::now() });
        self.step = step;
        if step == Step::Done {
            self.check_anim = Some(Anim::new_maybe(Instant::now(), Duration::from_millis(600), self.animations_enabled));
        }
    }

    pub fn toggle_group(&mut self, idx: usize) {
        if let Some(g) = self.groups.get_mut(idx) {
            if !GROUPS[idx].rows.is_empty() {
                g.open = !g.open;
                g.anim = Some(Anim::new_maybe(Instant::now(), Duration::from_millis(EXPANDER_MS), self.animations_enabled));
            }
        }
    }

    pub fn toggle_feature(&mut self, row: usize) {
        let rows = option_rows();
        if let Some(row_def) = rows.get(row) {
            let cur = (row_def.get)(&self.toggles);
            (row_def.set)(&mut self.toggles, !cur);
        }
    }

    pub fn on_install_event(&mut self, event: InstallEvent) {
        match event {
            InstallEvent::Progress(p) => {
                if let InstallPhase::Running { progress, progress_anim, .. } = &mut self.install {
                    let from = *progress;
                    *progress = p;
                    *progress_anim =
                        Some((Anim::new_maybe(Instant::now(), Duration::from_millis(200), self.animations_enabled), from, p));
                }
            }
            InstallEvent::ActionText(text) => {
                if let InstallPhase::Running { action_text, action_anim, .. } = &mut self.install {
                    *action_text = text;
                    *action_anim = Some(Anim::new_maybe(Instant::now(), Duration::from_millis(180), self.animations_enabled));
                }
            }
            InstallEvent::Error(msg) => {
                self.install = InstallPhase::Error(-1, msg);
            }
            InstallEvent::Done => {
                self.install = InstallPhase::Done;
                self.go_to(Step::Done);
            }
        }
    }

    /// Errores especiales de MSI que Task 6 Paso 4 pide tratar de forma distinta:
    /// 1602 (UAC cancelado, vuelta silenciosa a Opciones) y 1618 (otra instalación en
    /// curso).
    pub fn on_install_failed(&mut self, code: i32) {
        match code {
            1602 => {
                self.install = InstallPhase::UacCancelled;
                self.go_to(Step::Options);
            }
            1618 => self.install = InstallPhase::Busy,
            other => self.install = InstallPhase::Error(other, String::new()),
        }
    }

    pub fn any_animating(&self, now: Instant) -> bool {
        if self.transition.as_ref().is_some_and(|t| now.duration_since(t.start) < Duration::from_millis(STEP_IN_MS)) {
            return true;
        }
        if self.groups.iter().any(|g| g.anim.is_some_and(|a| !a.is_done(now))) {
            return true;
        }
        if let InstallPhase::Running { progress_anim, action_anim, .. } = &self.install {
            if progress_anim.is_some_and(|(a, _, _)| !a.is_done(now)) {
                return true;
            }
            if action_anim.is_some_and(|a| !a.is_done(now)) {
                return true;
            }
        }
        if self.check_anim.is_some_and(|a| !a.is_done(now)) {
            return true;
        }
        false
    }

    pub fn gc_animations(&mut self, now: Instant) {
        if self.transition.as_ref().is_some_and(|t| now.duration_since(t.start) >= Duration::from_millis(STEP_IN_MS)) {
            self.transition = None;
        }
        for g in &mut self.groups {
            if g.anim.is_some_and(|a| a.is_done(now)) {
                g.anim = None;
            }
        }
        if let InstallPhase::Running { progress_anim, action_anim, .. } = &mut self.install {
            if progress_anim.is_some_and(|(a, _, _)| a.is_done(now)) {
                *progress_anim = None;
            }
            if action_anim.is_some_and(|a| a.is_done(now)) {
                *action_anim = None;
            }
        }
        if self.check_anim.is_some_and(|a| a.is_done(now)) {
            self.check_anim = None;
        }
    }
}

fn pal() -> SetupPalette {
    palette::DARK
}

pub fn hit_test(st: &State, x: f32, y: f32) -> Hit {
    for &(r, h) in st.hits.iter().rev() {
        if r.contains(x, y) {
            return h;
        }
    }
    if y < TITLEBAR_H { Hit::Caption } else { Hit::None }
}

/// Dibuja la ventana completa del asistente. Devuelve `true` si hay que seguir
/// animando (el llamador decide si mantiene el `SetTimer` de 16ms activo).
///
/// Nota de implementación: `hits` se recoge en un `Vec` local en vez de directamente
/// en `st.hits` porque las funciones de dibujo necesitan `&Renderer` (que pide
/// prestado `st.renderer`) *a la vez* que necesitan anotar zonas de clic -- si esas
/// funciones tomaran `&mut State` para eso, el compilador no permitiría mantener
/// también `r: &Renderer` (préstamo de `st.renderer`) vivo mientras tanto. Pasando
/// `st: &State` (compartida, solo lectura) y `hits: &mut Vec<_>` por separado, ambos
/// préstamos son disjuntos y conviven sin problema.
pub fn paint(st: &mut State) -> bool {
    let now = Instant::now();
    let pal = pal();
    let mut hits: Vec<(Rect, Hit)> = Vec::new();

    {
        let (w, h) = st.renderer.size_dips();
        let r = &st.renderer;
        r.begin_paint(pal.win);

        let titlebar = Rect::new(0.0, 0.0, w, TITLEBAR_H);
        r.text("Instalar notty", &r.fonts().ui_12, Rect::new(12.0, 0.0, w - CLOSE_W, TITLEBAR_H), pal.titlebar_text);
        let close_r = Rect::new(w - CLOSE_W, 0.0, w, TITLEBAR_H);
        if st.hover == Hit::Close {
            r.fill(close_r, Rgba(1.0, 0.2, 0.2, 0.15));
        }
        let ccx = close_r.left + close_r.width() / 2.0;
        let ccy = close_r.top + close_r.height() / 2.0;
        r.stroke_line(ccx - 5.0, ccy - 5.0, ccx + 5.0, ccy + 5.0, 1.0, pal.text_3);
        r.stroke_line(ccx + 5.0, ccy - 5.0, ccx - 5.0, ccy + 5.0, 1.0, pal.text_3);
        hits.push((titlebar, Hit::Caption));
        hits.push((close_r, Hit::Close));

        let rail = Rect::new(0.0, TITLEBAR_H, RAIL_W, h - FOOT_H);
        draw_rail(r, rail, st.step, &pal);

        let content = Rect::new(RAIL_W, TITLEBAR_H, w, h - FOOT_H);

        if let Some(t) = st.transition {
            let elapsed = now.duration_since(t.start);
            if elapsed < Duration::from_millis(STEP_OUT_MS) {
                let lin = if st.animations_enabled { elapsed.as_secs_f32() / (STEP_OUT_MS as f32 / 1000.0) } else { 1.0 };
                let fade = (1.0 - lin).clamp(0.0, 1.0);
                let dx = -12.0 * lin;
                draw_step_content(r, &pal, content, t.from, st, now, dx, fade, &mut hits);
            }
        }
        {
            let (dx, fade) = match st.transition {
                Some(t) => {
                    let a = Anim::new_maybe(t.start, Duration::from_millis(STEP_IN_MS), st.animations_enabled);
                    let v = a.value(now, 0.0, 1.0);
                    (12.0 * (1.0 - v), v)
                }
                None => (0.0, 1.0),
            };
            draw_step_content(r, &pal, content, st.step, st, now, dx, fade, &mut hits);
        }

        let foot = Rect::new(0.0, h - FOOT_H, w, h);
        r.fill(foot, pal.foot);
        r.stroke_line(0.0, foot.top, w, foot.top, 1.0, pal.foot_border);
        draw_footer(r, &pal, foot, st, &mut hits);

        r.end_paint();
    }

    st.hits = hits;
    st.gc_animations(now);
    st.any_animating(now)
}

fn draw_rail(r: &Renderer, rail: Rect, step: Step, pal: &SetupPalette) {
    let track_x = rail.left + 21.5;
    let track_top = rail.top + 18.0 + 17.0;
    let track_h = 34.0 * 3.0;
    r.stroke_line(track_x, track_top, track_x, track_top + track_h, 1.5, pal.track);
    let done_h = track_h * (step.index() as f32 / 3.0);
    if done_h > 0.0 {
        r.stroke_line(track_x, track_top, track_x, track_top + done_h, 1.5, pal.accent);
    }

    let mut y = rail.top + 18.0;
    for (i, label) in STEP_LABELS.iter().enumerate() {
        let cy = y + 17.0;
        if i == step.index() {
            r.fill_circle(track_x, cy, 6.5, pal.rail_glow);
        }
        if i <= step.index() {
            r.fill_circle(track_x, cy, 4.0, pal.accent);
        } else {
            r.stroke_circle(track_x, cy, 4.0, 1.5, pal.rail_dot);
        }
        let text_c = if i == step.index() { pal.text } else if i < step.index() { pal.text_2 } else { pal.text_3 };
        r.text(label, &r.fonts().ui_13, Rect::new(rail.left + 40.0, y, rail.right, y + 34.0), text_c);
        y += 34.0;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_step_content(
    r: &Renderer,
    pal: &SetupPalette,
    area: Rect,
    step: Step,
    st: &State,
    now: Instant,
    dx: f32,
    fade: f32,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let area = Rect::new(area.left + dx, area.top, area.right + dx, area.bottom);
    match step {
        Step::Welcome => draw_welcome(r, pal, area, fade),
        Step::Options => draw_options(r, pal, area, fade, st, now, hits),
        Step::Installing => draw_installing(r, pal, area, fade, st, now, hits),
        Step::Done => draw_done(r, pal, area, fade, st, now),
    }
}

fn draw_welcome(r: &Renderer, pal: &SetupPalette, area: Rect, fade: f32) {
    let x = area.left + 32.0;
    let mut y = area.top + 24.0;
    r.text("notty 1.3.0", &r.fonts().ui_20_semibold, Rect::new(x, y, area.right - 32.0, y + 28.0), pal.text.faded(fade));
    y += 34.0;
    r.text(
        "Un bloc de notas ligero para Windows. Se instalará para todos los usuarios de este equipo.",
        &r.fonts().ui_13,
        Rect::new(x, y, area.right - 32.0, y + 40.0),
        pal.text_2.faded(fade),
    );
    y += 54.0;
    r.text("CARPETA", &r.fonts().ui_11, Rect::new(x, y, area.right - 32.0, y + 16.0), pal.text_3.faded(fade));
    y += 20.0;
    let path_r = Rect::new(x, y, area.right - 32.0, y + 32.0);
    r.fill_round(path_r, 4.0, pal.group.faded(fade));
    r.stroke_round_rect(path_r, 4.0, 1.0, pal.win_border.faded(fade));
    r.text(
        r"C:\Program Files\notty",
        &r.fonts().mono_12,
        Rect::new(path_r.left + 10.0, path_r.top, path_r.right - 80.0, path_r.bottom),
        pal.text_2.faded(fade),
    );
    r.text("Cambiar…", &r.fonts().ui_12, Rect::new(path_r.right - 70.0, path_r.top, path_r.right - 10.0, path_r.bottom), pal.accent.faded(fade));
    y += 44.0;
    r.text("2,3 MB · Windows 10 1809 o posterior", &r.fonts().ui_11, Rect::new(x, y, area.right - 32.0, y + 16.0), pal.text_3.faded(fade));
}

fn draw_options(r: &Renderer, pal: &SetupPalette, area: Rect, fade: f32, st: &State, now: Instant, hits: &mut Vec<(Rect, Hit)>) {
    let opts = Rect::new(area.left + 20.0, area.top + 16.0, area.right - PREVIEW_W - 16.0, area.bottom - 16.0);
    let preview = Rect::new(area.right - PREVIEW_W, area.top, area.right, area.bottom);
    let rows = option_rows();

    r.text("Opciones", &r.fonts().ui_20_semibold, Rect::new(opts.left, opts.top, opts.right, opts.top + 28.0), pal.text.faded(fade));
    r.text(
        "Todo se puede cambiar después desde Ajustes.",
        &r.fonts().ui_13,
        Rect::new(opts.left, opts.top + 30.0, opts.right, opts.top + 48.0),
        pal.text_2.faded(fade),
    );

    let mut y = opts.top + 58.0;
    for (gi, group) in GROUPS.iter().enumerate() {
        let gstate = st.groups[gi];
        let header_h = 40.0;
        let header_r = Rect::new(opts.left, y, opts.right, y + header_h);
        let hovered = st.hover == Hit::GroupHeader(gi);
        r.fill_round(header_r, 6.0, (if hovered { pal.group_hover } else { pal.group }).faded(fade));
        r.text(
            group.title,
            &r.fonts().ui_13,
            Rect::new(header_r.left + 14.0, header_r.top + 5.0, header_r.right - 90.0, header_r.top + 22.0),
            pal.text.faded(fade),
        );
        if !group.desc.is_empty() {
            r.text(
                group.desc,
                &r.fonts().ui_11_5,
                Rect::new(header_r.left + 14.0, header_r.top + 22.0, header_r.right - 90.0, header_r.top + 36.0),
                pal.text_3.faded(fade),
            );
        }
        if !group.rows.is_empty() {
            let on_count = group.rows.iter().filter(|&&ri| (rows[ri].get)(&st.toggles)).count();
            let summary = format!("{on_count} de {}", group.rows.len());
            r.text(&summary, &r.fonts().ui_11_5, Rect::new(header_r.right - 80.0, header_r.top, header_r.right - 28.0, header_r.bottom), pal.text_2.faded(fade));
        }
        let cx = header_r.right - 16.0;
        let cy = header_r.top + header_r.height() / 2.0;
        if gstate.open {
            r.stroke_line(cx - 4.0, cy - 2.0, cx, cy + 2.0, 1.5, pal.text_2.faded(fade));
            r.stroke_line(cx, cy + 2.0, cx + 4.0, cy - 2.0, 1.5, pal.text_2.faded(fade));
        } else {
            r.stroke_line(cx - 3.0, cy - 3.0, cx + 1.0, cy, 1.5, pal.text_2.faded(fade));
            r.stroke_line(cx + 1.0, cy, cx - 3.0, cy + 3.0, 1.5, pal.text_2.faded(fade));
        }
        hits.push((header_r, Hit::GroupHeader(gi)));
        y += header_h + 6.0;

        let open_t = match gstate.anim {
            Some(a) => a.value(now, if gstate.open { 0.0 } else { 1.0 }, if gstate.open { 1.0 } else { 0.0 }),
            None => if gstate.open { 1.0 } else { 0.0 },
        };
        if open_t > 0.001 && !group.rows.is_empty() {
            let row_h = 40.0;
            let full_h = row_h * group.rows.len() as f32;
            let clip_h = full_h * open_t;
            // Las filas se desvanecen con `open_t` (aproximación al fundido +
            // desplazamiento escalonado de 30ms/fila de la maqueta; sin acceso al
            // instante de inicio de `Anim` no se puede reproducir el stagger exacto
            // fila a fila, así que todas comparten el mismo progreso del grupo).
            let child_fade = fade * open_t;
            for (ci, &ri) in group.rows.iter().enumerate() {
                let row_top = y + ci as f32 * row_h;
                if row_top - y >= clip_h {
                    break;
                }
                let row_r = Rect::new(opts.left, row_top, opts.right, row_top + row_h - 2.0);
                if st.hover == Hit::Toggle(ri) {
                    r.fill_round(row_r, 4.0, pal.item_hover.faded(child_fade));
                }
                let title_r = Rect::new(row_r.left + 40.0, row_r.top + 4.0, row_r.right - 60.0, row_r.top + 20.0);
                r.text(rows[ri].title, &r.fonts().ui_13, title_r, pal.text.faded(child_fade));
                if !rows[ri].desc.is_empty() {
                    r.text(rows[ri].desc, &r.fonts().ui_11_5, Rect::new(title_r.left, title_r.bottom, title_r.right, title_r.bottom + 16.0), pal.text_3.faded(child_fade));
                }
                let on = (rows[ri].get)(&st.toggles);
                let track = Rect::new(row_r.right - 12.0 - 38.0, row_r.top + (row_h - 20.0) / 2.0, row_r.right - 12.0, row_r.top + (row_h - 20.0) / 2.0 + 20.0);
                draw_win11_toggle(r, track, on, pal, child_fade);
                hits.push((row_r, Hit::Toggle(ri)));
            }
            y += clip_h;
        }
        y += 6.0;
    }

    r.fill(preview, pal.preview_bg.faded(fade));
    r.stroke_line(preview.left, preview.top, preview.left, preview.bottom, 1.0, pal.foot_border.faded(fade));
    r.text("VISTA PREVIA", &r.fonts().ui_11, Rect::new(preview.left + 16.0, preview.top + 16.0, preview.right - 16.0, preview.top + 30.0), pal.text_3.faded(fade));
    let canvas = Rect::new(preview.left + 16.0, preview.top + 40.0, preview.right - 16.0, preview.bottom - 70.0);
    r.fill_round(canvas, 6.0, pal.canvas_bg.faded(fade));
    r.stroke_round_rect(canvas, 6.0, 1.0, pal.canvas_border.faded(fade));
    let active_row = match st.hover {
        Hit::Toggle(ri) => ri,
        _ => 0,
    };
    let row = &rows[active_row];
    draw_scene(r, pal, canvas, row.scene, fade);
    r.text(row.caption, &r.fonts().ui_12, Rect::new(preview.left + 16.0, canvas.bottom + 10.0, preview.right - 16.0, preview.bottom), pal.text_2.faded(fade));
}

fn draw_scene(r: &Renderer, pal: &SetupPalette, canvas: Rect, scene: Scene, fade: f32) {
    let x = canvas.left + 14.0;
    let y = canvas.top + 14.0;
    match scene {
        Scene::Notepad | Scene::Path => {
            r.text("C:\\> notepad notas", &r.fonts().mono_11, Rect::new(x, y, canvas.right - 14.0, y + 16.0), pal.text_2.faded(fade));
            r.text("→ se abre notty", &r.fonts().mono_11, Rect::new(x, y + 18.0, canvas.right - 14.0, y + 34.0), pal.accent.faded(fade));
        }
        Scene::Ctx => {
            r.text("📄 notas.txt", &r.fonts().ui_12, Rect::new(x, y, canvas.right - 14.0, y + 18.0), pal.text.faded(fade));
            let ctx = Rect::new(x + 20.0, y + 24.0, x + 140.0, y + 84.0);
            r.fill_round(ctx, 4.0, pal.group.faded(fade));
            r.stroke_round_rect(ctx, 4.0, 1.0, pal.btn2_border.faded(fade));
            r.text("Abrir con notty", &r.fonts().ui_11_5, Rect::new(ctx.left + 6.0, ctx.top + 20.0, ctx.right - 6.0, ctx.top + 36.0), pal.text.faded(fade));
        }
        Scene::Assoc => {
            r.text("readme.md            notty", &r.fonts().mono_11, Rect::new(x, y, canvas.right - 14.0, y + 16.0), pal.text_2.faded(fade));
            r.text("app.ini               notty", &r.fonts().mono_11, Rect::new(x, y + 18.0, canvas.right - 14.0, y + 34.0), pal.text_2.faded(fade));
        }
        Scene::Start => {
            r.fill_round(Rect::new(x, y + 20.0, canvas.right - 14.0, y + 44.0), 4.0, pal.group.faded(fade));
            r.text("▤ notty", &r.fonts().ui_12, Rect::new(x + 8.0, y + 24.0, canvas.right - 20.0, y + 40.0), pal.text.faded(fade));
        }
    }
}

fn draw_win11_toggle(r: &Renderer, track: Rect, on: bool, pal: &SetupPalette, fade: f32) {
    let (bg, border) = if on { (pal.accent, pal.accent) } else { (Rgba(0.0, 0.0, 0.0, 0.0), pal.off_border) };
    r.fill_round(track, 10.0, bg.faded(fade));
    r.stroke_round_rect(track, 10.0, 1.0, border.faded(fade));
    let ball_d = 12.0;
    let bx = if on { track.right - 3.0 - ball_d } else { track.left + 3.0 };
    let by = track.top + (track.height() - ball_d) / 2.0;
    let ball_c = if on { pal.on_accent } else { pal.off_ball };
    r.fill_round(Rect::new(bx, by, bx + ball_d, by + ball_d), ball_d / 2.0, ball_c.faded(fade));
}

fn draw_installing(r: &Renderer, pal: &SetupPalette, area: Rect, fade: f32, st: &State, now: Instant, hits: &mut Vec<(Rect, Hit)>) {
    let x = area.left + 32.0;
    let mut y = area.top + 34.0;
    match &st.install {
        InstallPhase::Running { progress, progress_anim, action_text, action_anim } => {
            r.text("Instalando…", &r.fonts().ui_20_semibold, Rect::new(x, y, area.right - 32.0, y + 28.0), pal.text.faded(fade));
            y += 30.0;
            r.text("No tardará más de unos segundos.", &r.fonts().ui_13, Rect::new(x, y, area.right - 32.0, y + 18.0), pal.text_2.faded(fade));
            y += 40.0;
            let bar = Rect::new(x, y, area.right - 32.0, y + 3.0);
            r.fill_round(bar, 1.5, pal.track.faded(fade));
            let pct = match progress_anim {
                Some((a, from, to)) => a.value(now, *from as f32, *to as f32),
                None => *progress as f32,
            };
            let fill_w = bar.width() * (pct / 100.0).clamp(0.0, 1.0);
            if fill_w > 0.0 {
                r.fill_round(Rect::new(bar.left, bar.top, bar.left + fill_w, bar.bottom), 1.5, pal.accent.faded(fade));
            }
            y += 20.0;
            let text_fade = fade * action_anim.map(|a| a.value(now, 0.3, 1.0)).unwrap_or(1.0);
            r.text(action_text, &r.fonts().ui_12, Rect::new(x, y, area.right - 120.0, y + 18.0), pal.text_3.faded(text_fade));
            r.text(&format!("{} %", pct.round() as i32), &r.fonts().ui_12, Rect::new(area.right - 90.0, y, area.right - 32.0, y + 18.0), pal.text_3.faded(fade));
        }
        InstallPhase::UacCancelled | InstallPhase::Done => {}
        InstallPhase::Busy => {
            r.text("Espera a que termine la otra instalación", &r.fonts().ui_13, Rect::new(x, y, area.right - 32.0, y + 40.0), pal.text.faded(fade));
            y += 50.0;
            let retry = Rect::new(x, y, x + 100.0, y + 30.0);
            r.fill_round(retry, 4.0, pal.btn2.faded(fade));
            r.stroke_round_rect(retry, 4.0, 1.0, pal.btn2_border.faded(fade));
            r.text("Reintentar", &r.fonts().ui_12_5, retry, pal.text.faded(fade));
            hits.push((retry, Hit::Retry));
        }
        InstallPhase::Error(code, msg) => {
            r.text(&format!("Error de instalación (código {code})"), &r.fonts().ui_13, Rect::new(x, y, area.right - 32.0, y + 20.0), pal.text.faded(fade));
            y += 24.0;
            if !msg.is_empty() {
                r.text(msg, &r.fonts().ui_11_5, Rect::new(x, y, area.right - 32.0, y + 40.0), pal.text_2.faded(fade));
                y += 40.0;
            }
            let log_path = std::env::temp_dir().join("notty-install.log");
            r.text(&format!("Registro: {}", log_path.display()), &r.fonts().ui_11_5, Rect::new(x, y, area.right - 32.0, y + 18.0), pal.accent.faded(fade));
        }
    }
}

fn draw_done(r: &Renderer, pal: &SetupPalette, area: Rect, fade: f32, st: &State, now: Instant) {
    let x = area.left + 32.0;
    let mut y = area.top + 24.0;

    let circle_r = 22.0;
    let cx = x + circle_r;
    let cy = y + circle_r;
    r.stroke_circle(cx, cy, circle_r, 2.0, pal.accent.faded(fade));
    let t = st.check_anim.map(|a| a.value(now, 0.0, 1.0)).unwrap_or(1.0);
    draw_check_stroke(r, cx, cy, pal.accent.faded(fade), t);
    y += 44.0 + 18.0;

    r.text("notty está listo", &r.fonts().ui_20_semibold, Rect::new(x, y, area.right - 32.0, y + 28.0), pal.text.faded(fade));
    y += 32.0;
    r.text(
        "Falta un paso: elegir notty como app predeterminada para .txt en Ajustes de Windows.",
        &r.fonts().ui_13,
        Rect::new(x, y, area.right - 32.0, y + 40.0),
        pal.text_2.faded(fade),
    );
}

/// Dibuja el "✓" de la maqueta (`viewBox 0 0 20 16`, dos segmentos: (2,8.5)-(7.5,13.5)
/// y (7.5,13.5)-(18,2.5)) revelándolo progresivamente según la longitud acumulada de
/// cada segmento, para imitar el `stroke-dashoffset` animado de la maqueta sin
/// necesitar geometría D2D dedicada (`ID2D1PathGeometry`).
fn draw_check_stroke(r: &Renderer, cx: f32, cy: f32, color: Rgba, t: f32) {
    let p0 = (cx - 8.0, cy + 0.5);
    let p1 = (cx - 2.5, cy + 5.5);
    let p2 = (cx + 8.0, cy - 5.5);
    let len1 = dist(p0, p1);
    let len2 = dist(p1, p2);
    let total = len1 + len2;
    let target = total * t.clamp(0.0, 1.0);
    if target <= 0.0 {
        return;
    }
    if target <= len1 {
        let f = target / len1;
        let p = lerp(p0, p1, f);
        r.stroke_line(p0.0, p0.1, p.0, p.1, 2.2, color);
    } else {
        r.stroke_line(p0.0, p0.1, p1.0, p1.1, 2.2, color);
        let f = (target - len1) / len2;
        let p = lerp(p1, p2, f);
        r.stroke_line(p1.0, p1.1, p.0, p.1, 2.2, color);
    }
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}
fn lerp(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

fn draw_footer(r: &Renderer, pal: &SetupPalette, foot: Rect, st: &State, hits: &mut Vec<(Rect, Hit)>) {
    match st.step {
        Step::Welcome => {
            draw_dots(r, foot, pal, 0);
            let next = primary_button(r, foot, "Siguiente", false, pal);
            hits.push((next, Hit::Next));
        }
        Step::Options => {
            draw_dots(r, foot, pal, 1);
            let next = primary_button(r, foot, "Instalar", true, pal);
            let back = secondary_button(r, foot, "Atrás", foot.right - next.left + 8.0, pal);
            hits.push((back, Hit::Back));
            hits.push((next, Hit::Next));
        }
        Step::Installing => {
            draw_dots(r, foot, pal, 2);
            let cancel = secondary_button(r, foot, "Cancelar", 0.0, pal);
            let _ = cancel; // Deshabilitado (opacidad reducida): sin cancelación real en Task 6.
        }
        Step::Done => {
            let link_r = Rect::new(foot.left + 18.0, foot.top, foot.left + 260.0, foot.bottom);
            r.text("Abrir Aplicaciones predeterminadas", &r.fonts().ui_12_5, link_r, pal.accent);
            hits.push((link_r, Hit::OpenDefaultApps));
            let btn = primary_button(r, foot, "Abrir notty", false, pal);
            hits.push((btn, Hit::OpenNotty));
        }
    }
}

fn draw_dots(r: &Renderer, foot: Rect, pal: &SetupPalette, active: usize) {
    let mut x = foot.left + 18.0;
    let y = foot.top + foot.height() / 2.0;
    for i in 0..3 {
        let w = if i == active { 18.0 } else { 6.0 };
        let c = if i == active { pal.accent } else { pal.track };
        r.fill_round(Rect::new(x, y - 3.0, x + w, y + 3.0), 3.0, c);
        x += w + 6.0;
    }
}

fn primary_button(r: &Renderer, foot: Rect, label: &str, shield: bool, pal: &SetupPalette) -> Rect {
    let text_w = r.measure(label, &r.fonts().ui_12_5);
    let shield_w = if shield { 16.0 } else { 0.0 };
    let w = text_w + shield_w + 36.0;
    let btn = Rect::new(foot.right - 18.0 - w, foot.top + 10.0, foot.right - 18.0, foot.bottom - 10.0);
    r.fill_round(btn, 4.0, pal.accent);
    r.stroke_round_rect(btn, 4.0, 1.0, pal.accent_border);
    let label_x = btn.left + 18.0 + shield_w;
    r.text(label, &r.fonts().ui_12_5, Rect::new(label_x, btn.top, btn.right - 18.0, btn.bottom), pal.on_accent);
    if shield {
        // Escudo UAC simplificado: un pentágono aproximado bastaría, pero un
        // rectángulo redondeado pequeño ya transmite "hay una elevación" sin
        // necesitar una geometría D2D dedicada.
        r.fill_round(Rect::new(btn.left + 16.0, btn.top + 8.0, btn.left + 24.0, btn.top + 20.0), 2.0, pal.on_accent);
    }
    btn
}

fn secondary_button(r: &Renderer, foot: Rect, label: &str, right_offset: f32, pal: &SetupPalette) -> Rect {
    let text_w = r.measure(label, &r.fonts().ui_12_5);
    let w = text_w + 32.0;
    let btn = Rect::new(foot.right - 18.0 - right_offset - w, foot.top + 10.0, foot.right - 18.0 - right_offset, foot.bottom - 10.0);
    r.fill_round(btn, 4.0, pal.btn2);
    r.stroke_round_rect(btn, 4.0, 1.0, pal.btn2_border);
    r.text(label, &r.fonts().ui_12_5, btn, pal.text);
    btn
}
