//! Estado y dibujo de `notty-setup`, siguiendo el patrón de `notty_ui::settings_window`
//! (misma técnica de ventana propia + Direct2D) pero con la paleta y el guion de
//! `docs/mockups/setup/instalador-flujo.html` e `instalador-opciones.html`.
use std::f32::consts::PI;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use notty_ui::layout::{Rect, TITLEBAR_H};
use notty_ui::welcome_window::adaptive::text_fit;
use notty_ui::{Anim, Renderer, Rgba};
use windows_numerics::{Matrix3x2, Vector2};

use crate::features::FeatureToggles;
use crate::msi_driver::{self, InstallEvent};
use crate::palette::{self, DARK, SetupPalette};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Texto para cada código de error de Windows Installer que tiene sentido explicar.
pub fn human_error(code: i32) -> &'static str {
    match code {
        1603 => "Windows Installer encontró un error grave durante la instalación.",
        1619 => "No se pudo abrir el paquete de instalación.",
        1620 => "El paquete de instalación no es válido o está dañado.",
        1625 => "Una directiva del sistema impide esta instalación.",
        1633 => "Este paquete no es compatible con este equipo.",
        1638 => "Ya hay otra versión de notty instalada. Desinstálala desde Aplicaciones instaladas.",
        _ => "La instalación no se ha completado.",
    }
}

pub const WIN_W: f32 = 760.0;
pub const WIN_H: f32 = 480.0;
/// Tamaño mínimo al redimensionar (sin pasar nunca del área de trabajo del monitor).
pub const MIN_W: f32 = 520.0;
pub const MIN_H: f32 = 400.0;
pub const RAIL_W: f32 = 168.0;
pub const PREVIEW_W: f32 = 236.0;
const CLOSE_W: f32 = 46.0;
pub const FOOT_H: f32 = 56.0;
/// Por debajo de este ancho de ventana el carril solo enseña los puntos.
const RAIL_COMPACT_BELOW: f32 = 600.0;
const RAIL_W_COMPACT: f32 = 44.0;
/// La vista previa de Opciones se oculta si no deja al menos esto a la columna.
const OPTIONS_MIN_W: f32 = 300.0;
const PREVIEW_MIN_W: f32 = 190.0;
const PREVIEW_MAX_W: f32 = 320.0;

/// Ancho del carril para una ventana de ancho `w`: estrecho proporcional hasta el
/// de la maqueta, y solo puntos (`true`) cuando no caben las etiquetas.
fn rail_width(w: f32) -> (f32, bool) {
    if w < RAIL_COMPACT_BELOW { (RAIL_W_COMPACT, true) } else { ((w * RAIL_W / WIN_W).clamp(128.0, RAIL_W), false) }
}

/// Ancho de la vista previa de Opciones dentro de un área de ancho `area_w` (236 en
/// la maqueta); `0` si no cabe junto a una columna de opciones legible.
fn preview_width(area_w: f32) -> f32 {
    if area_w < OPTIONS_MIN_W + PREVIEW_MIN_W {
        return 0.0;
    }
    let w = if area_w < 700.0 { area_w * 0.42 } else { PREVIEW_W + (area_w - 700.0) * 0.1 };
    w.clamp(PREVIEW_MIN_W, PREVIEW_MAX_W).min(area_w - OPTIONS_MIN_W)
}

const STEP_OUT_MS: u64 = 140;
const STEP_IN_MS: u64 = 180;
const EXPANDER_MS: u64 = 260;
const EXPANDER: Duration = Duration::from_millis(EXPANDER_MS);
const CHEVRON: Duration = Duration::from_millis(220);
const ITEM: Duration = Duration::from_millis(200);
const ITEM_STAGGER: Duration = Duration::from_millis(30);
const TOGGLE: Duration = Duration::from_millis(200);
const COUNT_OUT: Duration = Duration::from_millis(120);
const SCENE_FADE: Duration = Duration::from_millis(250);
const SCENE_SCALE: Duration = Duration::from_millis(300);

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
}

struct Group {
    title: &'static str,
    desc: &'static str,
    rows: &'static [usize],
    /// "Avanzado": en vez de interruptores, una fila con la carpeta de instalación.
    folder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    Notepad,
    Ctx,
    Path,
    Assoc,
    Start,
    Dir,
}

/// `.pcap` de cada escena (el `caps` del script de la maqueta).
fn scene_caption(scene: Scene) -> &'static str {
    match scene {
        Scene::Notepad => "Escribir \"notepad\" en cualquier sitio abrirá notty. El Bloc de notas original sigue disponible.",
        Scene::Ctx => "Clic derecho sobre cualquier archivo → Abrir con notty. En Windows 11, dentro de \"Mostrar más opciones\".",
        Scene::Path => "Escribe notty seguido de un archivo desde cmd o PowerShell.",
        Scene::Assoc => "notty aparecerá en \"Abrir con\" para estos tipos. Al terminar podrás elegirlo como predeterminado.",
        Scene::Start => "notty aparece en la lista de aplicaciones del menú Inicio.",
        Scene::Dir => "Dónde se copian los archivos. Hace falta una carpeta con permisos de administrador.",
    }
}

fn option_rows() -> [OptionRow; 9] {
    [
        OptionRow {
            title: "Sustituir el Bloc de notas",
            desc: "\"notepad\" abre notty · el original queda como notepad_legacy",
            get: |t| t.notepad_replace,
            set: |t, v| t.notepad_replace = v,
            scene: Scene::Notepad,
        },
        OptionRow {
            title: "\"Abrir con notty\" en el menú contextual",
            desc: "",
            get: |t| t.context_menu,
            set: |t, v| t.context_menu = v,
            scene: Scene::Ctx,
        },
        OptionRow {
            title: "Usar notty desde la terminal",
            desc: "Añade notty al PATH",
            get: |t| t.path_env,
            set: |t, v| t.path_env = v,
            scene: Scene::Path,
        },
        OptionRow {
            title: "Texto plano",
            desc: ".txt · .log · .text",
            get: |t| t.assoc_text,
            set: |t, v| t.assoc_text = v,
            scene: Scene::Assoc,
        },
        OptionRow {
            title: "Configuración",
            desc: ".ini · .cfg · .conf · .toml",
            get: |t| t.assoc_config,
            set: |t, v| t.assoc_config = v,
            scene: Scene::Assoc,
        },
        OptionRow {
            title: "Markdown",
            desc: ".md · .markdown",
            get: |t| t.assoc_markdown,
            set: |t, v| t.assoc_markdown = v,
            scene: Scene::Assoc,
        },
        OptionRow {
            title: "Código y datos",
            desc: ".json · .xml · .csv · .yaml",
            get: |t| t.assoc_code,
            set: |t, v| t.assoc_code = v,
            scene: Scene::Assoc,
        },
        OptionRow {
            title: "Menú Inicio",
            desc: "",
            get: |t| t.start_menu,
            set: |t, v| t.start_menu = v,
            scene: Scene::Start,
        },
        OptionRow {
            title: "Escritorio",
            desc: "",
            get: |t| t.desktop,
            set: |t, v| t.desktop = v,
            scene: Scene::Start,
        },
    ]
}

const GROUPS: [Group; 4] = [
    Group { title: "Integración con Windows", desc: "Bloc de notas, menú contextual, terminal", rows: &[0, 1, 2], folder: false },
    Group { title: "Tipos de archivo", desc: "Qué archivos ofrece abrir notty", rows: &[3, 4, 5, 6], folder: false },
    Group { title: "Accesos directos", desc: "", rows: &[7, 8], folder: false },
    Group { title: "Avanzado", desc: "Carpeta de instalación", rows: &[], folder: true },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hit {
    #[default]
    None,
    Caption,
    Minimize,
    Close,
    Back,
    Next,
    GroupHeader(usize),
    Toggle(usize),
    /// Fila de la carpeta de instalación dentro de "Avanzado".
    FolderRow,
    /// "Cambiar…" (Bienvenida y Avanzado): abre el selector de carpetas.
    ChangeFolder,
    ScrollThumb,
    OpenDefaultApps,
    OpenNotty,
    Retry,
    OpenLog,
    CopyDetails,
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
    /// Cuándo se abrió/cerró por última vez (escalonado de 30ms de las filas).
    pub since: Option<Instant>,
}

pub struct State {
    pub renderer: Renderer,
    pub animations_enabled: bool,
    pub anim_timer_running: bool,
    pub step: Step,
    pub toggles: FeatureToggles,
    pub install_folder: PathBuf,
    pub hover: Hit,
    /// Zona sobre la que se pulsó el botón del ratón (el clic se confirma al soltar).
    pub pressed: Hit,
    pub hits: Vec<(Rect, Hit)>,
    pub groups: [GroupState; 4],
    pub transition: Option<Transition>,
    pub install: InstallPhase,
    pub check_anim: Option<Anim>,
    pub relaunch_after_done: bool,
    /// Último texto de error que mandó MSI, para enseñarlo junto al código final.
    pub last_error: String,
    pub copied_at: Option<Instant>,
    /// Cuándo se pulsó cada interruptor (`.tg` anima bola y color en 200ms).
    pub toggle_at: [Option<Instant>; 9],
    /// Resumen "N de M" de cada grupo: cuándo cambió y cuántos había encendidos antes.
    pub count_at: [Option<(Instant, usize)>; 4],
    /// Desplazamiento vertical de la columna de opciones (`.opts{overflow:auto}`).
    pub scroll: f32,
    pub scroll_anim: Option<(Instant, f32)>,
    pub content_h: f32,
    pub viewport_h: f32,
    /// Arrastre de la barra de desplazamiento: (y del ratón al empezar, scroll al empezar).
    pub drag: Option<(f32, f32)>,
    /// Alto de la pista y del pulgar de la barra de desplazamiento del último pintado.
    pub scroll_geom: Option<(f32, f32)>,
    pub scene: Scene,
    pub prev_scene: Option<Scene>,
    pub scene_at: Instant,
    /// Cuándo entra el contenido al abrir la ventana (tras la animación nativa).
    pub entrance: Instant,
    rail_from: f32,
    rail_at: Option<Instant>,
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
            pressed: Hit::None,
            hits: Vec::new(),
            groups: [
                GroupState { open: true, anim: None, since: None },
                GroupState { open: false, anim: None, since: None },
                GroupState { open: false, anim: None, since: None },
                GroupState { open: false, anim: None, since: None },
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
            last_error: String::new(),
            copied_at: None,
            toggle_at: [None; 9],
            count_at: [None; 4],
            scroll: 0.0,
            scroll_anim: None,
            content_h: 0.0,
            viewport_h: 0.0,
            drag: None,
            scroll_geom: None,
            scene: Scene::Notepad,
            prev_scene: None,
            scene_at: Instant::now(),
            entrance: Instant::now() + Duration::from_millis(90),
            rail_from: 0.0,
            rail_at: None,
        }
    }

    pub fn max_scroll(&self) -> f32 {
        (self.content_h - self.viewport_h).max(0.0)
    }

    /// Posición de scroll a dibujar ahora (interpola 150ms hacia `scroll`).
    pub fn scroll_now(&self, now: Instant) -> f32 {
        match self.scroll_anim {
            Some((t0, from)) => {
                let p = if self.animations_enabled { (now.saturating_duration_since(t0).as_secs_f32() / 0.15).clamp(0.0, 1.0) } else { 1.0 };
                from + (self.scroll - from) * notty_ui::ease_out_cubic(p)
            }
            None => self.scroll,
        }
    }

    pub fn scroll_by(&mut self, dy: f32) {
        let now = Instant::now();
        let current = self.scroll_now(now);
        self.scroll = (self.scroll + dy).clamp(0.0, self.max_scroll());
        self.scroll_anim = Some((now, current));
    }

    pub fn begin_drag(&mut self, y: f32) {
        self.drag = Some((y, self.scroll));
        self.scroll_anim = None;
    }

    pub fn drag_to(&mut self, y: f32) {
        let (Some((y0, s0)), Some((track_h, thumb_h))) = (self.drag, self.scroll_geom) else { return };
        let travel = (track_h - thumb_h).max(1.0);
        self.scroll = (s0 + (y - y0) * self.max_scroll() / travel).clamp(0.0, self.max_scroll());
    }

    /// Cambia la escena de la vista previa (fundido + escala de 250/300ms).
    pub fn set_scene(&mut self, scene: Scene) {
        if scene != self.scene {
            self.prev_scene = Some(self.scene);
            self.scene = scene;
            self.scene_at = Instant::now();
        }
    }

    /// Actualiza la zona bajo el ratón; en Opciones, pasar por una fila cambia la vista previa.
    pub fn set_hover(&mut self, hit: Hit) -> bool {
        if hit == self.hover {
            return false;
        }
        self.hover = hit;
        if self.step == Step::Options {
            match hit {
                Hit::Toggle(ri) => self.set_scene(option_rows()[ri].scene),
                Hit::FolderRow | Hit::ChangeFolder => self.set_scene(Scene::Dir),
                _ => {}
            }
        }
        true
    }

    pub fn error_details(&self) -> String {
        let code = match self.install {
            InstallPhase::Error(code, _) => code,
            _ => 0,
        };
        let mut s = format!("notty-setup {VERSION}\nCódigo de MSI: {code} · {}\n", human_error(code));
        if !self.last_error.is_empty() {
            s.push_str(&self.last_error);
            s.push('\n');
        }
        s.push_str(&format!("Registro: {}", msi_driver::log_path().display()));
        s
    }

    pub fn go_to(&mut self, step: Step) {
        if step == self.step {
            return;
        }
        let now = Instant::now();
        self.rail_from = self.rail_now(now);
        self.rail_at = Some(now);
        self.transition = Some(Transition { from: self.step, start: now });
        self.step = step;
        if step == Step::Done {
            // El ✓ se dibuja cuando ya ha entrado la pantalla (140ms) + `.2s` de retraso.
            let start = now + Duration::from_millis(STEP_OUT_MS + 200);
            self.check_anim = Some(Anim::new_maybe(start, Duration::from_millis(600), self.animations_enabled));
        }
    }

    /// Hasta dónde debería llegar la línea del carril (0..1).
    fn rail_target(&self, now: Instant) -> f32 {
        let extra = match (&self.step, &self.install) {
            (Step::Installing, InstallPhase::Running { progress, progress_anim, .. }) => {
                let pct = progress_anim.map(|(a, from, to)| a.value(now, from as f32, to as f32)).unwrap_or(*progress as f32);
                pct / 100.0
            }
            _ => 0.0,
        };
        ((self.step.index() as f32 + extra) / 3.0).min(1.0)
    }

    pub fn rail_now(&self, now: Instant) -> f32 {
        let target = self.rail_target(now);
        match self.rail_at {
            Some(t0) => self.rail_from + (target - self.rail_from) * eased(now, t0, Duration::from_millis(400), self.animations_enabled),
            None => target,
        }
    }

    pub fn toggle_group(&mut self, idx: usize) {
        if let Some(g) = self.groups.get_mut(idx) {
            if !GROUPS[idx].rows.is_empty() || GROUPS[idx].folder {
                g.open = !g.open;
                g.anim = Some(Anim::new_maybe(Instant::now(), Duration::from_millis(EXPANDER_MS), self.animations_enabled));
                g.since = Some(Instant::now());
            }
        }
    }

    pub fn toggle_feature(&mut self, row: usize) {
        let rows = option_rows();
        let Some(row_def) = rows.get(row) else { return };
        if let Some(gi) = GROUPS.iter().position(|g| g.rows.contains(&row)) {
            let before = GROUPS[gi].rows.iter().filter(|&&ri| (rows[ri].get)(&self.toggles)).count();
            self.count_at[gi] = Some((Instant::now(), before));
        }
        let cur = (row_def.get)(&self.toggles);
        (row_def.set)(&mut self.toggles, !cur);
        self.toggle_at[row] = Some(Instant::now());
    }

    /// Vuelve a dejar la instalación lista para otro intento (Reintentar / volver a Opciones).
    pub fn reset_install(&mut self) {
        self.install = InstallPhase::Running { progress: 0, progress_anim: None, action_text: String::new(), action_anim: None };
        self.last_error.clear();
        self.copied_at = None;
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
                self.last_error = msg;
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
        // La vista previa de Opciones tiene animaciones en bucle (terminal, cursor).
        if self.step == Step::Options && self.animations_enabled {
            return true;
        }
        let recent = |t: Option<Instant>, ms: u64| t.is_some_and(|t0| now.saturating_duration_since(t0) < Duration::from_millis(ms));
        if now < self.entrance + Duration::from_millis(STEP_IN_MS)
            || recent(self.rail_at, 400)
            || recent(self.copied_at, 1700)
            || self.toggle_at.iter().any(|t| recent(*t, 200))
            || self.count_at.iter().any(|c| recent(c.map(|c| c.0), 300))
            || self.groups.iter().any(|g| recent(g.since, 500))
            || self.scroll_anim.is_some_and(|(t0, _)| now.saturating_duration_since(t0) < Duration::from_millis(150))
        {
            return true;
        }
        if self.transition.as_ref().is_some_and(|t| now.duration_since(t.start) < Duration::from_millis(STEP_OUT_MS + STEP_IN_MS)) {
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
        if self.transition.as_ref().is_some_and(|t| now.duration_since(t.start) >= Duration::from_millis(STEP_OUT_MS + STEP_IN_MS)) {
            self.transition = None;
        }
        if self.scroll_anim.is_some_and(|(t0, _)| now.saturating_duration_since(t0) >= Duration::from_millis(150)) {
            self.scroll_anim = None;
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

    let mut opts_out = OptsLayout::default();
    {
        let (w, h) = st.renderer.size_dips();
        let r = &st.renderer;
        r.begin_paint(pal.win);

        draw_titlebar(r, "Instalar notty", w, st.hover, &mut hits);

        let (rail_w, compact_rail) = rail_width(w);
        let rail = Rect::new(0.0, TITLEBAR_H, rail_w, h - FOOT_H);
        draw_rail(r, rail, st, now, &pal, compact_rail);

        let content = Rect::new(rail_w, TITLEBAR_H, w, h - FOOT_H);
        // Nada del paso se sale por encima del pie aunque la ventana sea baja (margen a
        // la izquierda para el desplazamiento de salida de 12px).
        r.push_clip(Rect::new(content.left - 12.0, content.top, content.right, content.bottom));
        let anim = st.animations_enabled;
        let out_d = Duration::from_millis(STEP_OUT_MS);
        let in_d = Duration::from_millis(STEP_IN_MS);
        // Salida (140ms, 12px a la izquierda) y después entrada (180ms desde la
        // derecha, ease-out). Al abrir la ventana solo hay entrada.
        let in_start = match st.transition {
            Some(t) if anim && now < t.start + out_d => {
                let lin = now.saturating_duration_since(t.start).as_secs_f32() / out_d.as_secs_f32();
                let mut throwaway = Vec::new();
                let mut throwaway_out = OptsLayout::default();
                draw_step_content(r, &pal, content, t.from, st, now, -12.0 * lin, 1.0 - lin, &mut throwaway, &mut throwaway_out);
                None
            }
            Some(t) => Some(t.start + out_d),
            None => Some(st.entrance),
        };
        if let Some(start) = in_start {
            let v = eased(now, start, in_d, anim);
            draw_step_content(r, &pal, content, st.step, st, now, 12.0 * (1.0 - v), v, &mut hits, &mut opts_out);
        }
        r.pop_clip();

        let foot = Rect::new(0.0, h - FOOT_H, w, h);
        r.fill(foot, pal.foot);
        r.fill(Rect::new(0.0, foot.top, w, foot.top + 1.0), pal.foot_border);
        draw_footer(r, &pal, foot, st, &mut hits);

        r.end_paint();
    }

    st.hits = hits;
    if st.step == Step::Options && opts_out.viewport_h > 0.0 {
        st.content_h = opts_out.content_h;
        st.viewport_h = opts_out.viewport_h;
        st.scroll_geom = opts_out.scroll_geom;
        let max = st.max_scroll();
        if st.scroll > max {
            st.scroll = max;
            st.scroll_anim = None;
        }
    }
    st.gc_animations(now);
    st.any_animating(now)
}

/// Barra de título de 32px (`.tb`): título y botones "—"/"✕" de 46px.
pub fn draw_titlebar(r: &Renderer, title: &str, w: f32, hover: Hit, hits: &mut Vec<(Rect, Hit)>) {
    let pal = DARK;
    r.text(title, &r.fonts().ui_12, Rect::new(12.0, 0.0, w - 2.0 * CLOSE_W, TITLEBAR_H), pal.titlebar_text);
    let min_r = Rect::new(w - 2.0 * CLOSE_W, 0.0, w - CLOSE_W, TITLEBAR_H);
    let close_r = Rect::new(w - CLOSE_W, 0.0, w, TITLEBAR_H);
    if hover == Hit::Minimize {
        r.fill(min_r, pal.caption_hover);
    }
    if hover == Hit::Close {
        r.fill(close_r, pal.close_hover);
    }
    r.text_center("—", &r.fonts().ui_11, min_r, pal.text_3);
    r.text_center("✕", &r.fonts().ui_11, close_r, if hover == Hit::Close { pal.text } else { pal.text_3 });
    hits.push((Rect::new(0.0, 0.0, w, TITLEBAR_H), Hit::Caption));
    hits.push((min_r, Hit::Minimize));
    hits.push((close_r, Hit::Close));
}

/// Carril de pasos (`.rail`, `.st`, `.track`): la línea se rellena en 400ms al cambiar
/// de paso y, en "Instalar", avanza con el progreso real de MSI.
fn draw_rail(r: &Renderer, rail: Rect, st: &State, now: Instant, pal: &SetupPalette, compact: bool) {
    let x = rail.left + 22.25;
    let top = rail.top + 18.0 + 17.0;
    let len = 34.0 * 3.0;
    r.fill(Rect::new(x - 0.75, top, x + 0.75, top + len), pal.track);
    let fill = st.rail_now(now);
    if fill > 0.0 {
        r.fill(Rect::new(x - 0.75, top, x + 0.75, top + len * fill), pal.accent);
    }
    let idx = st.step.index();
    for (i, label) in STEP_LABELS.iter().enumerate() {
        let cy = top + 34.0 * i as f32;
        if i == idx {
            r.fill_circle(x, cy, 4.75 + 3.0, pal.rail_glow);
            r.fill_circle(x, cy, 4.0, pal.win);
            r.stroke_circle(x, cy, 4.0, 1.5, pal.accent);
        } else if i < idx {
            r.fill_circle(x, cy, 4.75, pal.accent);
        } else {
            r.fill_circle(x, cy, 4.0, pal.win);
            r.stroke_circle(x, cy, 4.0, 1.5, pal.rail_dot);
        }
        if compact {
            continue;
        }
        let (font, c) = if i == idx {
            (&r.fonts().ui_13_semibold, pal.text)
        } else if i < idx {
            (&r.fonts().ui_13, pal.text_2)
        } else {
            (&r.fonts().ui_13, pal.text_3)
        };
        text_fit(r, label, font, Rect::new(rail.left + 37.0, cy - 17.0, rail.right - 6.0, cy + 17.0), c);
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
    out: &mut OptsLayout,
) {
    let area = Rect::new(area.left + dx, area.top, area.right + dx, area.bottom);
    match step {
        Step::Welcome => draw_welcome(r, pal, area, fade, st, hits),
        Step::Options => draw_options(r, pal, area, fade, st, now, hits, out),
        Step::Installing => draw_installing(r, pal, area, fade, st, now, hits),
        Step::Done => draw_done(r, pal, area, fade, st, now),
    }
}

/// Bienvenida (`instalador-flujo.html` 1): h4, `.sub`, "CARPETA" y el campo `.path`
/// (`background:#18191b;border-bottom-color:#73b6fa`) con "Cambiar…".
fn draw_welcome(r: &Renderer, pal: &SetupPalette, area: Rect, fade: f32, st: &State, hits: &mut Vec<(Rect, Hit)>) {
    r.set_fade(fade);
    let x = area.left + 4.0;
    let right = content_right(area, x);
    let mut y = area.top + 16.0;
    text_fit(r, &format!("notty {VERSION}"), &r.fonts().ui_20_semibold, Rect::new(x, y, right, y + 27.0), pal.text);
    y += 27.0 + 6.0;
    let sub = "Un bloc de notas ligero para Windows. Se instalará para todos los usuarios de este equipo.";
    let sub_h = r.measure_wrapped(sub, &r.fonts().ui_13, right - x, Some(19.5));
    r.text_wrapped(sub, &r.fonts().ui_13, Rect::new(x, y, right, y + sub_h), pal.text_2, Some(19.5));
    y += sub_h + 18.0 + 4.0;
    spaced_text(r, "CARPETA", Rect::new(x + 2.0, y, right, y + 15.0), pal.text_3);
    y += 15.0 + 8.0;

    let field = Rect::new(x, y, right, y + 32.0);
    r.fill_round(field, 4.0, pal.field_bg);
    r.stroke_round_rect(Rect::new(field.left + 0.5, field.top + 0.5, field.right - 0.5, field.bottom - 0.5), 4.0, 1.0, pal.win_border);
    r.fill(Rect::new(field.left + 3.0, field.bottom - 1.0, field.right - 3.0, field.bottom), pal.accent);
    let change_w = r.measure("Cambiar…", &r.fonts().ui_12);
    let change = Rect::new(field.right - 10.0 - change_w, field.top, field.right - 10.0, field.bottom);
    r.text(&st.install_folder.display().to_string(), &r.fonts().mono_12, Rect::new(field.left + 10.0, field.top, change.left - 8.0, field.bottom), pal.scene_text);
    r.text("Cambiar…", &r.fonts().ui_12, change, pal.accent);
    if st.hover == Hit::ChangeFolder {
        r.stroke_line(change.left, field.bottom - 8.0, change.right, field.bottom - 8.0, 1.0, pal.accent);
    }
    hits.push((Rect::new(change.left - 6.0, field.top, field.right, field.bottom), Hit::ChangeFolder));
    y += 32.0 + 10.0;
    text_fit(r, "2,3 MB · Windows 10 1809 o posterior", &r.fonts().ui_11_5, Rect::new(x, y, right, y + 16.0), pal.text_3);
    r.set_fade(1.0);
}

/// Borde derecho del contenido de un paso que empieza en `x`: margen de 32px de la
/// maqueta (18 en ventanas estrechas) y un ancho máximo para que, maximizada, la
/// barra de progreso o el campo de carpeta no crucen toda la pantalla.
fn content_right(area: Rect, x: f32) -> f32 {
    let pad = if area.width() < 460.0 { 18.0 } else { 32.0 };
    (area.right - pad).min(x + 600.0)
}

/// Medidas de la columna de opciones que `paint` devuelve al estado (scroll).
#[derive(Default)]
pub struct OptsLayout {
    pub content_h: f32,
    pub viewport_h: f32,
    pub scroll_geom: Option<(f32, f32)>,
}

/// Registra `rect` como zona de clic solo en la parte que queda dentro de `clip`.
fn push_hit(hits: &mut Vec<(Rect, Hit)>, rect: Rect, hit: Hit, clip: Rect) {
    let r = Rect::new(rect.left.max(clip.left), rect.top.max(clip.top), rect.right.min(clip.right), rect.bottom.min(clip.bottom));
    if r.width() > 0.0 && r.height() > 0.0 {
        hits.push((r, hit));
    }
}

fn eased(now: Instant, start: Instant, dur: Duration, enabled: bool) -> f32 {
    if !enabled {
        return 1.0;
    }
    let p = (now.saturating_duration_since(start).as_secs_f32() / dur.as_secs_f32()).clamp(0.0, 1.0);
    notty_ui::ease_out_cubic(p)
}

/// Icono de 18×18 de cada grupo (`.gic svg`, trazo de 1,3px).
fn draw_group_icon(r: &Renderer, gi: usize, x: f32, y: f32, c: Rgba) {
    let p = |px: f32, py: f32| (x + px, y + py);
    let w = 1.3;
    match gi {
        0 => {
            r.stroke_round_rect(Rect::new(x + 2.0, y + 3.0, x + 16.0, y + 14.0), 1.5, w, c);
            r.stroke_line(x + 2.0, y + 6.0, x + 16.0, y + 6.0, w, c);
        }
        1 => {
            r.stroke_polyline(&[p(4.0, 2.0), p(11.0, 2.0), p(14.0, 5.0), p(14.0, 16.0), p(4.0, 16.0), p(4.0, 2.0)], w, c);
            r.stroke_polyline(&[p(11.0, 2.0), p(11.0, 5.0), p(14.0, 5.0)], w, c);
        }
        2 => {
            r.stroke_line(x + 9.0, y + 3.0, x + 9.0, y + 11.0, w, c);
            r.stroke_polyline(&[p(5.5, 7.5), p(9.0, 11.0), p(12.5, 7.5)], w, c);
            r.stroke_line(x + 3.0, y + 14.0, x + 15.0, y + 14.0, w, c);
        }
        _ => {
            r.stroke_circle(x + 9.0, y + 9.0, 2.5, w, c);
            for (x0, y0, x1, y1) in [
                (9.0, 1.8, 9.0, 4.2),
                (9.0, 13.8, 9.0, 16.2),
                (1.8, 9.0, 4.2, 9.0),
                (13.8, 9.0, 16.2, 9.0),
                (3.9, 3.9, 5.6, 5.6),
                (12.4, 12.4, 14.1, 14.1),
                (3.9, 14.1, 5.6, 12.4),
                (12.4, 5.6, 14.1, 3.9),
            ] {
                r.stroke_line(x + x0, y + y0, x + x1, y + y1, w, c);
            }
        }
    }
}

/// Alto de una fila `.it` (`padding:9px 14px 9px 54px`, `border-top:1px`) con el título
/// y la descripción ya partidos en líneas al ancho `text_w`.
fn item_height(r: &Renderer, title: &str, desc: &str, text_w: f32) -> (f32, f32) {
    let title_h = r.measure_wrapped(title, &r.fonts().ui_13, text_w, None);
    let desc_h = if desc.is_empty() { 0.0 } else { 1.0 + r.measure_wrapped(desc, &r.fonts().ui_11_5, text_w, None) };
    (1.0 + 9.0 + title_h + desc_h + 9.0, title_h)
}

#[allow(clippy::too_many_arguments)]
fn draw_options(
    r: &Renderer,
    pal: &SetupPalette,
    area: Rect,
    fade: f32,
    st: &State,
    now: Instant,
    hits: &mut Vec<(Rect, Hit)>,
    out: &mut OptsLayout,
) {
    // `.opts{padding:16px 16px 16px 4px;overflow:auto}` junto a `.prev{width:236px}`;
    // en ventanas estrechas la vista previa encoge y, si no cabe, desaparece.
    let preview_w = preview_width(area.width());
    let col = Rect::new(area.left, area.top, area.right - preview_w, area.bottom);
    let preview = Rect::new(area.right - preview_w, area.top, area.right, area.bottom);
    let rows = option_rows();
    let left = col.left + 4.0;
    let right = col.right - 16.0;
    let scroll = st.scroll_now(now);
    let anim = st.animations_enabled;

    r.set_fade(fade);
    r.push_clip(col);
    let mut y = col.top + 16.0 - scroll;
    text_fit(r, "Opciones", &r.fonts().ui_20_semibold, Rect::new(left, y, right, y + 27.0), pal.text);
    y += 27.0 + 4.0;
    let intro = "Todo se puede cambiar después desde Ajustes.";
    let intro_h = r.measure_wrapped(intro, &r.fonts().ui_13, right - left, None).max(18.0);
    r.text_wrapped(intro, &r.fonts().ui_13, Rect::new(left, y, right, y + intro_h), pal.text_2, None);
    y += intro_h + 14.0;

    let hover_group = match st.hover {
        Hit::GroupHeader(gi) => Some(gi),
        Hit::Toggle(ri) => GROUPS.iter().position(|g| g.rows.contains(&ri)),
        Hit::FolderRow | Hit::ChangeFolder => Some(3),
        _ => None,
    };

    for (gi, group) in GROUPS.iter().enumerate() {
        let gs = st.groups[gi];
        let header_h = if group.desc.is_empty() { 50.0 } else { 56.0 };
        let text_x = left + 1.0 + 14.0 + 28.0 + 12.0;
        let toggle_left = right - 1.0 - 14.0 - 38.0;
        let text_w = toggle_left - 12.0 - text_x;

        // Alto completo del cuerpo y cuánto se ve ahora (`grid-template-rows` 260ms).
        let item_hs: Vec<(f32, f32)> = group.rows.iter().map(|&ri| item_height(r, rows[ri].title, rows[ri].desc, text_w)).collect();
        let full_h: f32 = item_hs.iter().map(|h| h.0).sum::<f32>() + if group.folder { 1.0 + 9.0 + 16.0 + 9.0 } else { 0.0 };
        let open_t = match gs.since {
            Some(t0) => {
                let v = eased(now, t0, EXPANDER, anim);
                if gs.open { v } else { 1.0 - v }
            }
            None => if gs.open { 1.0 } else { 0.0 },
        };
        let body_h = full_h * open_t;

        let card = Rect::new(left, y, right, y + header_h + body_h + 2.0);
        r.fill_round(card, 6.0, if hover_group == Some(gi) { pal.group_hover } else { pal.group });

        let header = Rect::new(left + 1.0, y + 1.0, right - 1.0, y + 1.0 + header_h);
        draw_group_icon(r, gi, left + 1.0 + 14.0 + 5.0, header.top + (header_h - 18.0) / 2.0, pal.text_2);
        let count_right = right - 1.0 - 14.0 - 22.0 - 12.0;
        let text_right = count_right - 40.0;
        if group.desc.is_empty() {
            text_fit(r, group.title, &r.fonts().ui_13, Rect::new(text_x, header.top, text_right, header.bottom), pal.text);
        } else {
            text_fit(r, group.title, &r.fonts().ui_13, Rect::new(text_x, header.top + 11.0, text_right, header.top + 28.5), pal.text);
            text_fit(r, group.desc, &r.fonts().ui_11_5, Rect::new(text_x, header.top + 29.5, text_right, header.top + 45.0), pal.text_3);
        }
        if !group.rows.is_empty() {
            let on = group.rows.iter().filter(|&&ri| (rows[ri].get)(&st.toggles)).count();
            // `.cnt`: se apaga 120ms con el valor viejo y vuelve con el nuevo.
            let (label, alpha) = match st.count_at[gi] {
                Some((t0, before)) if anim => {
                    let e = now.saturating_duration_since(t0);
                    if e < COUNT_OUT {
                        (before, 1.0 - e.as_secs_f32() / COUNT_OUT.as_secs_f32())
                    } else {
                        (on, ((e - COUNT_OUT).as_secs_f32() / 0.15).clamp(0.0, 1.0))
                    }
                }
                _ => (on, 1.0),
            };
            let summary = format!("{label} de {}", group.rows.len());
            r.text_right(&summary, &r.fonts().ui_11_5, Rect::new(count_right - 60.0, header.top, count_right, header.bottom), pal.text_2.faded(alpha));
        }
        // `.chev`: "v" cerrado, "^" abierto, gira en 220ms.
        let chev_t = match gs.since {
            Some(t0) => {
                let v = eased(now, t0, CHEVRON, anim);
                if gs.open { v } else { 1.0 - v }
            }
            None => if gs.open { 1.0 } else { 0.0 },
        };
        let (ccx, ccy) = (right - 1.0 - 14.0 - 4.0 - 5.0, header.top + header_h / 2.0);
        let (sin, cos) = (chev_t * PI).sin_cos();
        let rot = |px: f32, py: f32| (ccx + px * cos - py * sin, ccy + px * sin + py * cos);
        r.stroke_polyline(&[rot(-4.5, -2.25), rot(0.0, 2.25), rot(4.5, -2.25)], 1.5, pal.text_2);
        push_hit(hits, header, Hit::GroupHeader(gi), col);

        if body_h > 0.5 {
            let body = Rect::new(left + 1.0, header.bottom, right - 1.0, header.bottom + body_h);
            r.push_clip(body);
            let mut iy = header.bottom;
            let item_alpha = |ci: usize| -> f32 {
                match gs.since {
                    Some(t0) if gs.open => eased(now, t0 + ITEM_STAGGER * ci as u32, ITEM, anim),
                    Some(t0) => 1.0 - eased(now, t0, ITEM, anim),
                    None => 1.0,
                }
            };
            for (ci, &ri) in group.rows.iter().enumerate() {
                let (item_h, title_h) = item_hs[ci];
                let a = item_alpha(ci);
                let dy = -4.0 * (1.0 - a);
                let item = Rect::new(left + 1.0, iy, right - 1.0, iy + item_h);
                if st.hover == Hit::Toggle(ri) {
                    r.fill(Rect::new(item.left, item.top + 1.0, item.right, item.bottom), pal.item_hover);
                }
                r.fill(Rect::new(item.left, item.top, item.right, item.top + 1.0), pal.group_border);
                r.set_fade(fade * a);
                let ty = item.top + 1.0 + 9.0 + dy;
                r.text_wrapped(rows[ri].title, &r.fonts().ui_13, Rect::new(text_x, ty, text_x + text_w, ty + title_h), pal.text, None);
                if !rows[ri].desc.is_empty() {
                    let dy0 = ty + title_h + 1.0;
                    r.text_wrapped(rows[ri].desc, &r.fonts().ui_11_5, Rect::new(text_x, dy0, text_x + text_w, item.bottom), pal.text_3, None);
                }
                let tg_top = item.top + 1.0 + (item_h - 1.0 - 20.0) / 2.0 + dy;
                let k = toggle_k(st, ri, now);
                draw_toggle(r, pal, Rect::new(toggle_left, tg_top, toggle_left + 38.0, tg_top + 20.0), k, st.pressed == Hit::Toggle(ri));
                r.set_fade(fade);
                push_hit(hits, item, Hit::Toggle(ri), intersect_or(body, col));
                iy += item_h;
            }
            if group.folder {
                let item_h = 1.0 + 9.0 + 16.0 + 9.0;
                let a = item_alpha(0);
                let item = Rect::new(left + 1.0, iy, right - 1.0, iy + item_h);
                if matches!(st.hover, Hit::FolderRow | Hit::ChangeFolder) {
                    r.fill(Rect::new(item.left, item.top + 1.0, item.right, item.bottom), pal.item_hover);
                }
                r.fill(Rect::new(item.left, item.top, item.right, item.top + 1.0), pal.group_border);
                r.set_fade(fade * a);
                let change_w = r.measure("Cambiar…", &r.fonts().ui_12);
                let change = Rect::new(item.right - 14.0 - change_w, item.top + 1.0, item.right - 14.0, item.bottom);
                let path = st.install_folder.display().to_string();
                r.text(&path, &r.fonts().mono_12, Rect::new(text_x, item.top + 1.0, change.left - 12.0, item.bottom), pal.text);
                r.text("Cambiar…", &r.fonts().ui_12, change, pal.accent);
                if st.hover == Hit::ChangeFolder {
                    let uy = item.top + 1.0 + (item_h - 1.0) / 2.0 + 8.0;
                    r.stroke_line(change.left, uy, change.right, uy, 1.0, pal.accent);
                }
                r.set_fade(fade);
                let clip = intersect_or(body, col);
                push_hit(hits, item, Hit::FolderRow, clip);
                push_hit(hits, Rect::new(change.left - 6.0, item.top, change.right + 6.0, item.bottom), Hit::ChangeFolder, clip);
            }
            r.pop_clip();
        }
        r.stroke_round_rect(Rect::new(card.left + 0.5, card.top + 0.5, card.right - 0.5, card.bottom - 0.5), 6.0, 1.0, pal.group_border);
        y = card.bottom + 6.0;
    }
    r.pop_clip();

    // Barra de desplazamiento fina estilo Windows 11 (se ensancha al pasar el ratón).
    out.viewport_h = col.height();
    out.content_h = (y + scroll) - col.top - 6.0 + 16.0;
    if out.content_h > out.viewport_h + 0.5 {
        let track = Rect::new(col.right - 12.0, col.top + 4.0, col.right - 2.0, col.bottom - 4.0);
        let thumb_h = (track.height() * out.viewport_h / out.content_h).max(28.0);
        let max = out.content_h - out.viewport_h;
        let thumb_top = track.top + (track.height() - thumb_h) * (scroll / max).clamp(0.0, 1.0);
        let active = st.hover == Hit::ScrollThumb || st.drag.is_some();
        let bar_w = if active { 6.0 } else { 2.0 };
        let bar = Rect::new(track.right - 2.0 - bar_w, thumb_top, track.right - 2.0, thumb_top + thumb_h);
        r.fill_round(bar, bar_w / 2.0, pal.text_3.faded(if active { 0.85 } else { 0.55 }));
        hits.push((Rect::new(track.left, thumb_top, track.right, thumb_top + thumb_h), Hit::ScrollThumb));
        out.scroll_geom = Some((track.height(), thumb_h));
    }

    if preview_w <= 0.0 {
        r.set_fade(1.0);
        return;
    }
    // `.prev{padding:16px;gap:10px}`: etiqueta, lienzo y `.pcap{min-height:54px;line-height:1.5}`.
    r.fill(preview, pal.preview_bg);
    r.fill(Rect::new(preview.left, preview.top, preview.left + 1.0, preview.bottom), pal.foot_border);
    spaced_text(r, "VISTA PREVIA", Rect::new(preview.left + 16.0, preview.top + 16.0, preview.right - 16.0, preview.top + 31.0), pal.text_3);
    let caption = scene_caption(st.scene);
    let cap_w = preview.width() - 32.0;
    let cap_h = r.measure_wrapped(caption, &r.fonts().ui_12, cap_w, Some(18.0)).max(54.0);
    let cap = Rect::new(preview.left + 16.0, preview.bottom - 16.0 - cap_h, preview.right - 16.0, preview.bottom - 16.0);
    let canvas = Rect::new(preview.left + 16.0, preview.top + 16.0 + 15.0 + 10.0, preview.right - 16.0, cap.top - 10.0);
    // Ventana muy baja: sin sitio para el lienzo, solo queda la explicación.
    if canvas.height() >= 40.0 {
        draw_preview_canvas(r, pal, canvas, st, now, fade);
    }
    r.text_wrapped(caption, &r.fonts().ui_12, cap, pal.text_2, Some(18.0));
    r.set_fade(1.0);
}

/// Lienzo de la vista previa con la escena actual (y la anterior mientras se va).
fn draw_preview_canvas(r: &Renderer, pal: &SetupPalette, canvas: Rect, st: &State, now: Instant, fade: f32) {
    let anim = st.animations_enabled;
    r.fill_round(canvas, 6.0, pal.canvas_bg);
    r.stroke_round_rect(Rect::new(canvas.left + 0.5, canvas.top + 0.5, canvas.right - 0.5, canvas.bottom - 0.5), 6.0, 1.0, pal.canvas_border);

    // `.scene{transition:opacity .25s,transform .3s}`: la vieja se va, la nueva llega.
    r.push_clip(Rect::new(canvas.left + 1.0, canvas.top + 1.0, canvas.right - 1.0, canvas.bottom - 1.0));
    let op = eased(now, st.scene_at, SCENE_FADE, anim);
    let sc = eased(now, st.scene_at, SCENE_SCALE, anim);
    let center = Vector2 { X: (canvas.left + canvas.right) / 2.0, Y: (canvas.top + canvas.bottom) / 2.0 };
    let elapsed = now.saturating_duration_since(st.scene_at).as_secs_f32();
    if let Some(prev) = st.prev_scene.filter(|_| op < 1.0) {
        let s = 1.0 - 0.03 * sc;
        r.set_transform(Matrix3x2::scale_around(s, s, center));
        r.set_fade(fade * (1.0 - op));
        draw_scene(r, pal, canvas, prev, 10.0, &st.install_folder);
    }
    let s = 0.97 + 0.03 * sc;
    r.set_transform(Matrix3x2::scale_around(s, s, center));
    r.set_fade(fade * op);
    draw_scene(r, pal, canvas, st.scene, if anim { elapsed } else { 10.0 }, &st.install_folder);
    r.reset_transform();
    r.set_fade(fade);
    r.pop_clip();
}

/// Texto en mayúsculas con `letter-spacing:.06em` (`.pl`, `.cap-t`).
fn spaced_text(r: &Renderer, s: &str, rect: Rect, c: Rgba) {
    let font = &r.fonts().ui_11;
    let mut x = rect.left;
    let mut buf = [0u8; 4];
    for ch in s.chars() {
        let g = ch.encode_utf8(&mut buf);
        r.text(g, font, Rect::new(x, rect.top, rect.right, rect.bottom), c);
        x += r.measure(g, font) + 11.0 * 0.06;
    }
}

fn intersect_or(a: Rect, b: Rect) -> Rect {
    Rect::new(a.left.max(b.left), a.top.max(b.top), a.right.min(b.right), a.bottom.min(b.bottom))
}

/// Cuánto de "encendido" tiene el interruptor de la fila `row` ahora mismo (0..1),
/// animado 200ms tras cada pulsación.
fn toggle_k(st: &State, row: usize, now: Instant) -> f32 {
    let on = (option_rows()[row].get)(&st.toggles);
    let t = st.toggle_at[row].map(|t0| eased(now, t0, TOGGLE, st.animations_enabled)).unwrap_or(1.0);
    if on { t } else { 1.0 - t }
}

/// Interruptor de Windows 11 (`.tg`, 38×20): bola de 12px que se desliza y se
/// ensancha a 15px mientras se mantiene pulsado.
fn draw_toggle(r: &Renderer, pal: &SetupPalette, track: Rect, k: f32, pressed: bool) {
    let off_border = pal.text_3;
    r.fill_round(track, 10.0, pal.accent.faded(k));
    let border = mix(off_border, pal.accent, k);
    r.stroke_round_rect(Rect::new(track.left + 0.5, track.top + 0.5, track.right - 0.5, track.bottom - 0.5), 9.5, 1.0, border);
    let knob_w = if pressed { 15.0 } else { 12.0 };
    let x_off = track.left + 4.0;
    let x_on = track.right - 4.0 - knob_w;
    let kx = x_off + (x_on - x_off) * k;
    let ky = track.top + 4.0;
    r.fill_round(Rect::new(kx, ky, kx + knob_w, ky + 12.0), 6.0, mix(pal.off_ball, pal.on_accent, k));
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    Rgba(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t, a.3 + (b.3 - a.3) * t)
}

/// Un tramo de una línea de terminal: texto, color y si va en negrita.
type Seg<'a> = (&'a str, Rgba, bool);

/// Una línea de `.term` (monoespaciada 11px, `line-height:1.7`), por tramos. Devuelve
/// dónde acaba, para colocar el cursor.
fn term_line(r: &Renderer, x: f32, y: f32, right: f32, segs: &[Seg]) -> f32 {
    let mut cx = x;
    for &(s, c, bold) in segs {
        let font = if bold { &r.fonts().mono_11_bold } else { &r.fonts().mono_11 };
        r.text(s, font, Rect::new(cx, y, right, y + 18.7), c);
        cx += r.measure(s, font);
    }
    cx
}

/// `.typed{animation:ty 1.4s steps(14) infinite alternate}`: cuántos caracteres se ven.
fn typed(s: &str, elapsed: f32) -> &str {
    let cycle = elapsed % 2.8;
    let p = if cycle < 1.4 { cycle / 1.4 } else { (2.8 - cycle) / 1.4 };
    let n = ((p * 14.0).floor() as usize).min(14);
    let end = s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len());
    &s[..end]
}

/// `.c` (6×12, parpadeo `steps(1)` de 1s) al final de una línea de terminal.
fn term_cursor(r: &Renderer, pal: &SetupPalette, x: f32, y: f32, elapsed: f32) {
    if elapsed % 1.0 < 0.5 {
        r.fill(Rect::new(x, y + 5.0, x + 6.0, y + 17.0), pal.scene_text);
    }
}

/// `.file`: icono + nombre (`gap:6px;padding:4px 6px`), opcionalmente con el texto de
/// la derecha (`margin-left:auto`) y el fondo de seleccionado.
fn file_row(r: &Renderer, pal: &SetupPalette, row: Rect, icon: &str, name: &str, right: Option<(&str, Rgba)>, sel: bool) {
    if sel {
        r.fill_round(row, 3.0, pal.file_sel);
    }
    let font = &r.fonts().ui_11_5;
    let x = row.left + 6.0;
    r.text_color_font(icon, font, Rect::new(x, row.top, row.right, row.bottom), pal.scene_text);
    let name_x = x + r.measure(icon, font) + 6.0;
    r.text(name, font, Rect::new(name_x, row.top, row.right - 6.0, row.bottom), pal.scene_text);
    if let Some((s, c)) = right {
        r.text_right(s, font, Rect::new(name_x, row.top, row.right - 6.0, row.bottom), c);
    }
}

/// Aparición de `@keyframes pop{from{opacity:0;transform:translateY(-6px) scale(.97)}}`.
fn pop(elapsed: f32, secs: f32) -> (f32, f32) {
    let v = notty_ui::ease_out_cubic((elapsed / secs).clamp(0.0, 1.0));
    (v, -6.0 * (1.0 - v))
}

/// Cada escena de `.canvas` (`padding:14px;font-size:11.5px`). `elapsed` son los
/// segundos desde que se mostró, para las animaciones en bucle.
fn draw_scene(r: &Renderer, pal: &SetupPalette, canvas: Rect, scene: Scene, elapsed: f32, folder: &Path) {
    let x = canvas.left + 14.0;
    let y = canvas.top + 14.0;
    let right = canvas.right - 14.0;
    let lh = 18.7;
    let row_h = 23.3;
    let t = pal.scene_text;
    match scene {
        Scene::Notepad => {
            term_line(r, x, y, right, &[("C:\\> ", t, false), (typed("notepad notas", elapsed), t, false)]);
            term_line(r, x, y + lh, right, &[("→", pal.accent, false), (" se abre ", t, false), ("notty", pal.text, true)]);
            term_line(r, x, y + lh * 3.0, right, &[("C:\\> notepad_legacy", t, false)]);
            let end = term_line(r, x, y + lh * 4.0, right, &[("→", pal.accent, false), (" Bloc de notas original", t, false)]);
            term_cursor(r, pal, end, y + lh * 4.0, elapsed);
        }
        Scene::Path => {
            term_line(r, x, y, right, &[("PS> ", t, false), (typed("notty .\\app.log", elapsed), t, false)]);
            let end = term_line(r, x, y + lh, right, &[("→", pal.accent, false), (" abierto en una pestaña nueva", t, false)]);
            term_cursor(r, pal, end, y + lh, elapsed);
        }
        Scene::Ctx => {
            for (i, (name, sel)) in [("lista.txt", false), ("notas.txt", true), ("ideas.md", false)].into_iter().enumerate() {
                let row = Rect::new(x, y + row_h * i as f32, right, y + row_h * (i + 1) as f32);
                file_row(r, pal, row, "📄", name, None, sel);
            }
            // `.ctx{left:58px;top:52px;width:140px;padding:4px}` con su `pop` de .3s.
            // En lienzos estrechos se corre a la izquierda para no quedar cortado.
            let (a, dy) = pop(elapsed, 0.3);
            let mx = canvas.left + 58.0_f32.min(canvas.width() - 140.0 - 8.0).max(8.0);
            let menu = Rect::new(mx, canvas.top + 52.0 + dy, mx + 140.0, canvas.top + 52.0 + dy + 2.0 + 8.0 + row_h * 4.0);
            let base = r.fade();
            r.set_fade(base * a);
            r.draw_popup_shadow(menu, 6.0, 1.0, pal.shadow);
            r.fill_round(menu, 6.0, pal.ctx_bg);
            r.stroke_round_rect(Rect::new(menu.left + 0.5, menu.top + 0.5, menu.right - 0.5, menu.bottom - 0.5), 6.0, 1.0, pal.btn2_border);
            for (i, label) in ["Abrir", "Abrir con notty", "Copiar ruta", "Propiedades"].into_iter().enumerate() {
                let item = Rect::new(menu.left + 5.0, menu.top + 5.0 + row_h * i as f32, menu.right - 5.0, menu.top + 5.0 + row_h * (i + 1) as f32);
                let hl = i == 1;
                if hl {
                    r.fill_round(item, 3.0, pal.ctx_hl);
                }
                r.text(label, &r.fonts().ui_11_5, Rect::new(item.left + 8.0, item.top, item.right - 8.0, item.bottom), if hl { pal.text } else { t });
            }
            r.set_fade(base);
        }
        Scene::Assoc => {
            let files: [(&str, &str, bool); 4] = [("📄", "readme.md", true), ("⚙", "app.ini", true), ("📄", "build.log", true), ("{ }", "data.json", false)];
            let base = r.fade();
            for (i, (icon, name, on)) in files.into_iter().enumerate() {
                let row = Rect::new(x, y + row_h * i as f32, right, y + row_h * (i + 1) as f32);
                r.set_fade(if on { base } else { base * 0.45 });
                let tag = if on { ("notty", pal.accent) } else { ("—", t) };
                file_row(r, pal, row, icon, name, Some(tag), false);
            }
            r.set_fade(base);
        }
        Scene::Start => {
            // Caja "Anclado" (`margin-top:30px;padding:10px;border-radius:6px`).
            let top = y + 30.0;
            let panel = Rect::new(x, top, right, top + 10.0 + 14.0 + 6.0 + row_h * 2.0 + 10.0);
            r.fill_round(panel, 6.0, pal.start_box);
            r.text("Anclado", &r.fonts().ui_10_5, Rect::new(panel.left + 10.0, top + 10.0, panel.right - 10.0, top + 24.0), pal.text_3);
            let ry = top + 10.0 + 14.0 + 6.0;
            let (a, dy) = pop(elapsed, 0.4);
            let base = r.fade();
            r.set_fade(base * a);
            file_row(r, pal, Rect::new(panel.left + 10.0, ry + dy, panel.right - 10.0, ry + dy + row_h), "▤", "notty", None, true);
            r.set_fade(base);
            file_row(r, pal, Rect::new(panel.left + 10.0, ry + row_h, panel.right - 10.0, ry + row_h * 2.0), "▤", "Terminal", None, false);
        }
        Scene::Dir => {
            let dim = pal.text_3;
            let mut dir = folder.display().to_string();
            if !dir.ends_with('\\') {
                dir.push('\\');
            }
            term_line(r, x, y, right, &[(&dir, dim, false)]);
            term_line(r, x, y + lh, right, &[("  notty.exe", dim, false)]);
            term_line(r, x, y + lh * 2.0, right, &[("  notepad_legacy.exe", dim, false)]);
        }
    }
}

fn draw_installing(r: &Renderer, pal: &SetupPalette, area: Rect, fade: f32, st: &State, now: Instant, hits: &mut Vec<(Rect, Hit)>) {
    let x = area.left + 4.0;
    let right = content_right(area, x);
    let mut y = area.top + 34.0;
    match &st.install {
        InstallPhase::Running { progress, progress_anim, action_text, action_anim } => {
            text_fit(r, "Instalando…", &r.fonts().ui_20_semibold, Rect::new(x, y, right, y + 28.0), pal.text.faded(fade));
            y += 30.0;
            text_fit(r, "No tardará más de unos segundos.", &r.fonts().ui_13, Rect::new(x, y, right, y + 18.0), pal.text_2.faded(fade));
            y += 40.0;
            let bar = Rect::new(x, y, right, y + 3.0);
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
            r.text(action_text, &r.fonts().ui_12, Rect::new(x, y, right - 88.0, y + 18.0), pal.text_3.faded(text_fade));
            r.text_right(&format!("{} %", pct.round() as i32), &r.fonts().ui_12, Rect::new(right - 80.0, y, right, y + 18.0), pal.text_3.faded(fade));
        }
        InstallPhase::UacCancelled | InstallPhase::Done => {}
        InstallPhase::Busy => {
            let _ = hits;
            r.set_fade(fade);
            text_fit(r, "Otra instalación en curso", &r.fonts().ui_20_semibold, Rect::new(x, y, right, y + 27.0), pal.text);
            y += 27.0 + 6.0;
            r.text_wrapped(
                "Espera a que termine la otra instalación y pulsa Reintentar.",
                &r.fonts().ui_13,
                Rect::new(x, y, right, y + 60.0),
                pal.text_2,
                Some(19.5),
            );
            r.set_fade(1.0);
        }
        InstallPhase::Error(code, _) => {
            r.set_fade(fade);
            text_fit(r, "No se pudo instalar notty", &r.fonts().ui_20_semibold, Rect::new(x, y, right, y + 27.0), pal.text);
            y += 27.0 + 6.0;
            let human = human_error(*code);
            let human_h = r.measure_wrapped(human, &r.fonts().ui_13, right - x, Some(19.5));
            r.text_wrapped(human, &r.fonts().ui_13, Rect::new(x, y, right, y + human_h), pal.text_2, Some(19.5));
            y += human_h + 14.0;
            // Caja de detalles con el estilo de `.notes` (lo mismo que copia "Copiar detalles").
            let mut details = format!("Código {code}");
            if !st.last_error.is_empty() {
                details.push_str(" · ");
                details.push_str(st.last_error.trim());
            }
            details.push_str(&format!("\nRegistro: {}", msi_driver::log_path().display()));
            let line_h = 12.0 * 1.65;
            let inner_w = right - x - 24.0;
            // Con la ventana baja la caja encoge (su texto se recorta dentro), pero
            // nunca por debajo de una línea: lo que no quepa lo corta el panel.
            let box_h = (r.measure_wrapped(&details, &r.fonts().mono_12, inner_w, Some(line_h)) + 20.0)
                .min(area.bottom - y - 12.0)
                .max(20.0 + line_h);
            let notes = Rect::new(x, y, right, y + box_h);
            r.fill_round(notes, 6.0, pal.field_bg);
            r.stroke_round_rect(Rect::new(notes.left + 0.5, notes.top + 0.5, notes.right - 0.5, notes.bottom - 0.5), 6.0, 1.0, pal.foot_border);
            r.push_clip(Rect::new(notes.left + 12.0, notes.top + 10.0, notes.right - 12.0, notes.bottom - 10.0));
            r.text_wrapped(&details, &r.fonts().mono_12, Rect::new(notes.left + 12.0, notes.top + 10.0, notes.right - 12.0, notes.bottom), pal.scene_text, Some(line_h));
            r.pop_clip();
            r.set_fade(1.0);
        }
    }
}

fn draw_done(r: &Renderer, pal: &SetupPalette, area: Rect, fade: f32, st: &State, now: Instant) {
    let x = area.left + 4.0;
    let mut y = area.top + 24.0;

    let circle_r = 22.0;
    let cx = x + circle_r;
    let cy = y + circle_r;
    r.stroke_circle(cx, cy, circle_r, 2.0, pal.accent.faded(fade));
    let t = st.check_anim.map(|a| a.value(now, 0.0, 1.0)).unwrap_or(1.0);
    draw_check_stroke(r, cx, cy, pal.accent.faded(fade), t);
    y += 44.0 + 18.0;

    let right = content_right(area, x);
    text_fit(r, "notty está listo", &r.fonts().ui_20_semibold, Rect::new(x, y, right, y + 28.0), pal.text.faded(fade));
    y += 32.0;
    let hint = "Falta un paso: elegir notty como app predeterminada para .txt en Ajustes de Windows.";
    let hint_h = r.measure_wrapped(hint, &r.fonts().ui_13, right - x, Some(19.5));
    r.text_wrapped(hint, &r.fonts().ui_13, Rect::new(x, y, right, y + hint_h), pal.text_2.faded(fade), Some(19.5));
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

/// Enlace del pie (`color:#73b6fa;font-size:12.5px`), subrayado al pasar el ratón.
/// Se trunca con «…» para no pasar de `max_right` (los botones de la derecha).
fn footer_link(r: &Renderer, pal: &SetupPalette, foot: Rect, label: &str, hover: bool, max_right: f32) -> Rect {
    let font = &r.fonts().ui_12_5;
    let left = foot.left + 18.0;
    let shown = notty_ui::welcome_window::adaptive::ellipsize(r, label, font, (max_right - left).max(0.0));
    let w = r.measure(&shown, font);
    let link = Rect::new(left, foot.top + 14.0, left + w, foot.bottom - 14.0);
    r.text(&shown, font, link, pal.accent);
    if hover {
        let uy = foot.top + foot.height() / 2.0 + 8.0;
        r.stroke_line(link.left, uy, link.right, uy, 1.0, pal.accent);
    }
    link
}

fn draw_footer(r: &Renderer, pal: &SetupPalette, foot: Rect, st: &State, hits: &mut Vec<(Rect, Hit)>) {
    let cy = foot.top + foot.height() / 2.0;
    let right = foot.right - 18.0;
    let hover = st.hover;
    match st.step {
        Step::Welcome => {
            draw_dots(r, foot, pal, 0);
            let next = primary_button(r, right, cy, "Siguiente", false, hover == Hit::Next);
            hits.push((next, Hit::Next));
        }
        Step::Options => {
            r.text("2,3 MB", &r.fonts().ui_12, Rect::new(foot.left + 18.0, foot.top, foot.left + 200.0, foot.bottom), pal.text_3);
            let next = primary_button(r, right, cy, "Instalar", true, hover == Hit::Next);
            let back = secondary_button(r, next.left - 6.0, cy, "Atrás", hover == Hit::Back);
            hits.push((back, Hit::Back));
            hits.push((next, Hit::Next));
        }
        Step::Installing => match &st.install {
            InstallPhase::Error(_, _) => {
                let back = primary_button(r, right, cy, "Volver a Opciones", false, hover == Hit::Back);
                let copied = st.copied_at.is_some_and(|t| t.elapsed() < Duration::from_millis(1600));
                let label = if copied { "Copiado ✓" } else { "Copiar detalles" };
                let copy = secondary_button(r, back.left - 6.0, cy, label, hover == Hit::CopyDetails);
                let log = footer_link(r, pal, foot, "Abrir el registro", hover == Hit::OpenLog, copy.left - 12.0);
                if log.width() > 0.0 {
                    hits.push((log, Hit::OpenLog));
                }
                hits.push((copy, Hit::CopyDetails));
                hits.push((back, Hit::Back));
            }
            InstallPhase::Busy => {
                let retry = primary_button(r, right, cy, "Reintentar", true, hover == Hit::Retry);
                let back = secondary_button(r, retry.left - 6.0, cy, "Atrás", hover == Hit::Back);
                hits.push((back, Hit::Back));
                hits.push((retry, Hit::Retry));
            }
            _ => {
                draw_dots(r, foot, pal, 2);
                r.set_fade(0.45);
                secondary_button(r, right, cy, "Cancelar", false);
                r.set_fade(1.0);
            }
        },
        Step::Done => {
            let btn = primary_button(r, right, cy, "Abrir notty", false, hover == Hit::OpenNotty);
            let link = footer_link(r, pal, foot, "Abrir Aplicaciones predeterminadas", hover == Hit::OpenDefaultApps, btn.left - 12.0);
            if link.width() > 0.0 {
                hits.push((link, Hit::OpenDefaultApps));
            }
            hits.push((btn, Hit::OpenNotty));
        }
    }
}

/// `.dots i{width:6px;height:6px;background:#46474c}`, `.dots i.on{width:18px;background:#73b6fa}`.
fn draw_dots(r: &Renderer, foot: Rect, pal: &SetupPalette, active: usize) {
    let mut x = foot.left + 20.0;
    let y = foot.top + foot.height() / 2.0;
    for i in 0..3 {
        let w = if i == active { 18.0 } else { 6.0 };
        let c = if i == active { pal.accent } else { pal.dot_off };
        r.fill_round(Rect::new(x, y - 3.0, x + w, y + 3.0), 3.0, c);
        x += w + 6.0;
    }
}

/// Botón primario del pie (`.btn{padding:6px 18px;font-weight:600;font-size:12.5px}`),
/// con el escudo UAC (`.shield`, 10×12) si `shield`. Borde derecho en `right`, centrado en `cy`.
pub fn primary_button(r: &Renderer, right: f32, cy: f32, label: &str, shield: bool, hover: bool) -> Rect {
    let pal = DARK;
    let font = &r.fonts().ui_12_5_semibold;
    let shield_w = if shield { 10.0 + 7.0 } else { 0.0 };
    let w = 1.0 + 18.0 + shield_w + r.measure(label, font) + 18.0 + 1.0;
    let btn = Rect::new(right - w, cy - 15.5, right, cy + 15.5);
    r.fill_round(btn, 4.0, if hover { pal.accent_border } else { pal.accent });
    r.stroke_round_rect(btn, 4.0, 1.0, pal.accent_border);
    let mut x = btn.left + 19.0;
    if shield {
        // clip-path:polygon(50% 0,100% 18%,100% 55%,50% 100%,0 55%,0 18%)
        let (sx, sy) = (x, cy - 6.0);
        let pts = [(5.0, 0.0), (10.0, 2.16), (10.0, 6.6), (5.0, 12.0), (0.0, 6.6), (0.0, 2.16)];
        let pts: Vec<(f32, f32)> = pts.iter().map(|&(px, py)| (sx + px, sy + py)).collect();
        r.fill_polygon(&pts, pal.on_accent.faded(0.85));
        x += shield_w;
    }
    r.text(label, font, Rect::new(x, btn.top, btn.right - 18.0, btn.bottom), pal.on_accent);
    btn
}

/// Botón secundario del pie (`.btn2{padding:6px 16px;font-size:12.5px}`).
pub fn secondary_button(r: &Renderer, right: f32, cy: f32, label: &str, hover: bool) -> Rect {
    let pal = DARK;
    let font = &r.fonts().ui_12_5;
    let w = 1.0 + 16.0 + r.measure(label, font) + 16.0 + 1.0;
    let btn = Rect::new(right - w, cy - 15.5, right, cy + 15.5);
    r.fill_round(btn, 4.0, if hover { pal.ctx_hl } else { pal.btn2 });
    r.stroke_round_rect(btn, 4.0, 1.0, pal.btn2_border);
    r.text_center(label, font, btn, pal.text);
    btn
}
