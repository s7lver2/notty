//! Animaciones de entrada/salida de pestañas: la nueva crece desde ancho 0 fundiéndose
//! y la que se cierra encoge hasta desaparecer, empujando a sus vecinas en vez de que
//! salten. `Workspace` no sabe nada de esto: aquí se lleva la cuenta por índice, y
//! cada cierre corrige los índices de lo que ya estaba en marcha.

use std::time::{Duration, Instant};

use crate::Anim;

pub const TAB_ANIM_MS: u64 = 180;

/// Pestaña ya cerrada que se sigue dibujando mientras encoge. `at` es la posición de
/// la fila en la que se dibuja: justo antes del documento que ahora tiene índice `at`
/// (o al final si `at == len`).
#[derive(Debug, Clone)]
pub struct Ghost {
    pub at: usize,
    pub name: String,
    pub dirty: bool,
    from_scale: f32,
    anim: Anim,
}

/// Lo que `render.rs` necesita en un instante concreto (ya evaluado, sin relojes).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TabAnimFrame {
    /// (índice de documento, escala `0..=1`) de las pestañas que están entrando.
    pub opening: Vec<(usize, f32)>,
    pub ghosts: Vec<GhostFrame>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GhostFrame {
    pub at: usize,
    pub name: String,
    pub dirty: bool,
    pub scale: f32,
}

impl TabAnimFrame {
    pub fn open_scale(&self, doc: usize) -> f32 {
        self.opening.iter().find(|(i, _)| *i == doc).map(|(_, s)| *s).unwrap_or(1.0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct TabAnims {
    opening: Vec<(usize, Anim)>,
    ghosts: Vec<Ghost>,
}

impl TabAnims {
    /// Se acaba de abrir el documento `idx`.
    pub fn on_open(&mut self, idx: usize, now: Instant, enabled: bool) {
        self.opening.retain(|(i, _)| *i != idx);
        self.opening.push((idx, Anim::new_maybe(now, Duration::from_millis(TAB_ANIM_MS), enabled)));
    }

    /// Se acaba de quitar el documento `idx` de la lista (antes de quitarlo tenía ese
    /// índice). `name`/`dirty` son los de la pestaña cerrada, para seguir dibujándola.
    pub fn on_close(&mut self, idx: usize, name: String, dirty: bool, now: Instant, enabled: bool) {
        let mut from_scale = 1.0;
        self.opening.retain_mut(|(i, a)| {
            if *i == idx {
                from_scale = a.value(now, 0.0, 1.0);
                return false;
            }
            if *i > idx {
                *i -= 1;
            }
            true
        });
        let insert_pos = self.ghosts.iter().take_while(|g| g.at <= idx).count();
        for g in &mut self.ghosts {
            if g.at > idx {
                g.at -= 1;
            }
        }
        let anim = Anim::new_maybe(now, Duration::from_millis(TAB_ANIM_MS), enabled);
        self.ghosts.insert(insert_pos, Ghost { at: idx, name, dirty, from_scale, anim });
    }

    /// Olvida lo que ya terminó.
    pub fn prune(&mut self, now: Instant) {
        self.opening.retain(|(_, a)| !a.is_done(now));
        self.ghosts.retain(|g| !g.anim.is_done(now));
    }

    pub fn is_animating(&self) -> bool {
        !self.opening.is_empty() || !self.ghosts.is_empty()
    }

    pub fn frame(&self, now: Instant) -> TabAnimFrame {
        TabAnimFrame {
            opening: self.opening.iter().map(|(i, a)| (*i, a.value(now, 0.0, 1.0))).collect(),
            ghosts: self
                .ghosts
                .iter()
                .map(|g| GhostFrame { at: g.at, name: g.name.clone(), dirty: g.dirty, scale: g.anim.value(now, g.from_scale, 0.0) })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_tab_grows_from_zero() {
        let now = Instant::now();
        let mut a = TabAnims::default();
        a.on_open(2, now, true);
        assert_eq!(a.frame(now).open_scale(2), 0.0);
        assert_eq!(a.frame(now).open_scale(1), 1.0);
        let later = now + Duration::from_millis(TAB_ANIM_MS);
        assert_eq!(a.frame(later).open_scale(2), 1.0);
        a.prune(later);
        assert!(!a.is_animating());
    }

    #[test]
    fn closing_leaves_a_shrinking_ghost_where_the_tab_was() {
        let now = Instant::now();
        let mut a = TabAnims::default();
        a.on_close(1, "b.txt".into(), false, now, true);
        let f = a.frame(now);
        assert_eq!(f.ghosts.len(), 1);
        assert_eq!(f.ghosts[0].at, 1);
        assert_eq!(f.ghosts[0].scale, 1.0);
        assert_eq!(a.frame(now + Duration::from_millis(TAB_ANIM_MS)).ghosts[0].scale, 0.0);
    }

    #[test]
    fn closing_shifts_later_indices_and_keeps_visual_order() {
        let now = Instant::now();
        let mut a = TabAnims::default();
        a.on_open(3, now, true);
        // Cerrar 3, 2, 1 seguidos (p.ej. "Cerrar las de la derecha" desde la 0).
        a.on_close(3, "d".into(), false, now, true);
        a.on_close(2, "c".into(), false, now, true);
        a.on_close(1, "b".into(), false, now, true);
        let f = a.frame(now);
        assert!(f.opening.is_empty());
        let names: Vec<&str> = f.ghosts.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["b", "c", "d"]);
        assert!(f.ghosts.iter().all(|g| g.at == 1));
        // La que estaba entrando se va desde la escala a la que había llegado (0 aquí).
        assert_eq!(f.ghosts[2].scale, 0.0);
    }

    #[test]
    fn closing_an_earlier_tab_moves_opening_index_left() {
        let now = Instant::now();
        let mut a = TabAnims::default();
        a.on_open(4, now, true);
        a.on_close(0, "a".into(), false, now, true);
        assert_eq!(a.frame(now).opening[0].0, 3);
    }

    #[test]
    fn disabled_animations_finish_immediately() {
        let now = Instant::now();
        let mut a = TabAnims::default();
        a.on_open(1, now, false);
        a.on_close(0, "a".into(), false, now, false);
        a.prune(now);
        assert!(!a.is_animating());
    }
}
