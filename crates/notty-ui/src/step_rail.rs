//! Carril de pasos reutilizable (ventana de bienvenida, instalador): una columna con
//! una etiqueta por paso, un punto que se rellena en el paso activo/completado y una
//! línea vertical de progreso detrás de los puntos (`docs/mockups/setup/tutorial.html`,
//! `.rail`/`.st`/`.track`).

use crate::layout::Rect;
use crate::theme::Palette;
use crate::Renderer;

/// Alto de cada fila de paso (`.st{height:30px}`).
pub const STEP_ROW_H: f32 = 30.0;
/// Diámetro del punto (`.st i{width:8px;height:8px}`).
const DOT_D: f32 = 8.0;
/// Separación entre el punto y la etiqueta (`.st{gap:9px}`).
const DOT_LABEL_GAP: f32 = 9.0;

pub struct StepRail {
    pub steps: Vec<&'static str>,
    pub current: usize,
}

impl StepRail {
    pub fn new(steps: Vec<&'static str>) -> Self {
        Self { steps, current: 0 }
    }

    /// `0.0..=1.0`: qué parte del carril debe quedar ya "rellena" con el color de
    /// acento (`.track b{height}` en la maqueta), según el paso activo.
    pub fn progress_fraction(&self) -> f32 {
        if self.steps.len() <= 1 { 1.0 } else { self.current as f32 / (self.steps.len() - 1) as f32 }
    }
}

/// Dibuja el carril dentro de `target` (normalmente la columna izquierda de la
/// ventana): una fila por paso con su punto y etiqueta, más la línea de progreso
/// detrás de los puntos. `progress` es la fracción ya animada (`0.0..=1.0`), para que
/// el llamador pueda interpolarla con `Anim` en vez de saltar de golpe.
pub fn draw(r: &Renderer, pal: &Palette, target: Rect, rail: &StepRail, progress: f32) {
    let dot_cx = target.left + DOT_D / 2.0;
    let first_cy = target.top + STEP_ROW_H / 2.0;
    let last_cy = target.top + STEP_ROW_H * (rail.steps.len().max(1) as f32 - 1.0) + STEP_ROW_H / 2.0;

    // Línea de fondo (recorrido completo) + relleno de acento hasta `progress`.
    if rail.steps.len() > 1 {
        r.stroke_line(dot_cx, first_cy, dot_cx, last_cy, 1.5, pal.line);
        let filled_cy = first_cy + (last_cy - first_cy) * progress.clamp(0.0, 1.0);
        r.stroke_line(dot_cx, first_cy, dot_cx, filled_cy, 1.5, pal.accent);
    }

    for (i, label) in rail.steps.iter().enumerate() {
        let row_top = target.top + STEP_ROW_H * i as f32;
        let cy = row_top + STEP_ROW_H / 2.0;
        let done = i < rail.current;
        let on = i == rail.current;

        if on {
            // Anillo tenue alrededor del punto activo (`.st.on i{box-shadow:...}`).
            r.stroke_circle(dot_cx, cy, DOT_D / 2.0 + 3.0, 3.0, pal.accent_soft);
        }
        if done || on {
            r.fill_circle(dot_cx, cy, DOT_D / 2.0, pal.accent);
        } else {
            r.fill_circle(dot_cx, cy, DOT_D / 2.0, pal.surface);
            r.stroke_circle(dot_cx, cy, DOT_D / 2.0, 1.5, pal.text_3);
        }

        let label_left = target.left + DOT_D + DOT_LABEL_GAP;
        let label_r = Rect::new(label_left, row_top, target.right, row_top + STEP_ROW_H);
        let (fmt, color) = if on {
            (&r.fonts().ui_12_5, pal.text)
        } else if done {
            (&r.fonts().ui_12, pal.text_2)
        } else {
            (&r.fonts().ui_12, pal.text_3)
        };
        r.text(label, fmt, label_r, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_fraction_is_zero_at_first_step() {
        let rail = StepRail::new(vec!["Hola", "Estilo", "Tema", "Teclado", "Privacidad", "Listo"]);
        assert_eq!(rail.progress_fraction(), 0.0);
    }

    #[test]
    fn progress_fraction_is_one_at_last_step() {
        let mut rail = StepRail::new(vec!["Hola", "Estilo", "Tema", "Teclado", "Privacidad", "Listo"]);
        rail.current = 5;
        assert_eq!(rail.progress_fraction(), 1.0);
    }

    #[test]
    fn progress_fraction_of_single_step_is_one() {
        let rail = StepRail::new(vec!["Solo"]);
        assert_eq!(rail.progress_fraction(), 1.0);
    }

    #[test]
    fn progress_fraction_is_linear_between_steps() {
        let mut rail = StepRail::new(vec!["a", "b", "c"]);
        rail.current = 1;
        assert_eq!(rail.progress_fraction(), 0.5);
    }
}
