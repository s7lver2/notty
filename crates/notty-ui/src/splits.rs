//! Modo Paneles (`Files::Splits`), como en kitty: arriba hay pestañas, y cada pestaña
//! se divide en paneles lado a lado con un documento cada uno. Los documentos siguen
//! viviendo en `Workspace` (lista plana); aquí solo se guarda qué documento va en qué
//! panel de qué pestaña. El panel con foco de la pestaña activa siempre muestra
//! `ws.active_index()`, así que todo lo que ya enruta la entrada al documento activo
//! sigue valiendo tal cual.
//!
//! Invariante tras `reconcile`: cada documento está en exactamente un panel, ninguna
//! pestaña está vacía y los anchos de cada pestaña suman 1.

use crate::layout::Rect;

pub const MAX_PANES: usize = 4;
/// Ancho de la línea divisoria entre paneles.
pub const DIVIDER_W: f32 = 1.0;
/// Fracción mínima del ancho que puede quedarle a un panel al acomodarlos.
pub const MIN_WEIGHT: f32 = 0.12;
/// Panel recién partido que espera al documento que se va a abrir para él.
const PENDING: usize = usize::MAX;

#[derive(Debug, Clone, PartialEq)]
struct Tab {
    panes: Vec<usize>,
    weights: Vec<f32>,
    focus: usize,
}

impl Tab {
    fn single(doc: usize) -> Self {
        Self { panes: vec![doc], weights: vec![1.0], focus: 0 }
    }

    /// Quita el panel `i`; su ancho pasa al vecino y el foco sigue en el mismo panel
    /// (o en el de la izquierda si era el quitado y no hay más a la derecha).
    fn remove_pane(&mut self, i: usize) {
        self.panes.remove(i);
        let w = if i < self.weights.len() { self.weights.remove(i) } else { 0.0 };
        let ni = i.min(self.weights.len().saturating_sub(1));
        if let Some(n) = self.weights.get_mut(ni) {
            *n += w;
        }
        if i < self.focus || (i == self.focus && self.focus >= self.panes.len() && self.focus > 0) {
            self.focus -= 1;
        }
    }

    fn normalize(&mut self) {
        if self.weights.len() != self.panes.len() {
            self.weights = vec![1.0; self.panes.len()];
        }
        let sum: f32 = self.weights.iter().sum();
        if sum <= f32::EPSILON {
            self.weights.iter_mut().for_each(|w| *w = 1.0);
        }
        let sum: f32 = self.weights.iter().sum();
        self.weights.iter_mut().for_each(|w| *w /= sum);
        self.focus = self.focus.min(self.panes.len().saturating_sub(1));
    }
}

#[derive(Debug, Clone)]
pub struct Layouts {
    tabs: Vec<Tab>,
    active: usize,
}

impl Default for Layouts {
    fn default() -> Self {
        Self { tabs: vec![Tab::single(0)], active: 0 }
    }
}

impl Layouts {
    /// Deja la estructura coherente con `doc_count` documentos, con `active_doc` en
    /// el panel con foco de la pestaña activa. Los documentos que no están en ningún
    /// panel (recién abiertos) llenan primero un panel recién partido y, si no hay,
    /// cada uno va a su propia pestaña justo después de la activa.
    pub fn reconcile(&mut self, doc_count: usize, active_doc: usize) {
        let doc_count = doc_count.max(1);
        let active_doc = active_doc.min(doc_count - 1);
        let mut seen = vec![false; doc_count];
        for tab in &mut self.tabs {
            let mut i = 0;
            while i < tab.panes.len() {
                let d = tab.panes[i];
                if d == PENDING || (d < doc_count && !seen[d]) {
                    if d != PENDING {
                        seen[d] = true;
                    }
                    i += 1;
                } else {
                    tab.remove_pane(i);
                }
            }
        }
        let mut free: Vec<usize> = (0..doc_count).filter(|&d| !seen[d]).collect();
        // El activo primero: es el que se acaba de abrir para el panel partido.
        if let Some(pos) = free.iter().position(|&d| d == active_doc) {
            let d = free.remove(pos);
            free.insert(0, d);
        }
        for tab in &mut self.tabs {
            for p in tab.panes.iter_mut().filter(|p| **p == PENDING) {
                if !free.is_empty() {
                    *p = free.remove(0);
                }
            }
            while let Some(i) = tab.panes.iter().position(|&p| p == PENDING) {
                tab.remove_pane(i);
            }
        }
        let mut insert_at = (self.active + 1).min(self.tabs.len());
        for d in free {
            self.tabs.insert(insert_at, Tab::single(d));
            insert_at += 1;
        }
        self.tabs.retain(|t| !t.panes.is_empty());
        if self.tabs.is_empty() {
            self.tabs.push(Tab::single(0));
        }
        for t in &mut self.tabs {
            t.normalize();
        }
        if let Some((ti, pi)) = self.locate(active_doc) {
            self.active = ti;
            self.tabs[ti].focus = pi;
        }
        self.active = self.active.min(self.tabs.len() - 1);
    }

    /// Antes de `reconcile`, tras `Workspace::close(idx)` si la lista se acortó: los
    /// índices posteriores bajan uno y el panel del cerrado desaparece.
    pub fn on_doc_closed(&mut self, idx: usize) {
        for tab in &mut self.tabs {
            if let Some(pos) = tab.panes.iter().position(|&d| d == idx) {
                tab.remove_pane(pos);
            }
            for d in &mut tab.panes {
                if *d != PENDING && *d > idx {
                    *d -= 1;
                }
            }
        }
    }

    fn locate(&self, doc: usize) -> Option<(usize, usize)> {
        self.tabs.iter().enumerate().find_map(|(ti, t)| t.panes.iter().position(|&d| d == doc).map(|pi| (ti, pi)))
    }

    fn cur(&self) -> &Tab {
        &self.tabs[self.active.min(self.tabs.len() - 1)]
    }

    fn cur_mut(&mut self) -> &mut Tab {
        let i = self.active.min(self.tabs.len() - 1);
        &mut self.tabs[i]
    }

    fn focused_doc(&self) -> usize {
        let t = self.cur();
        t.panes[t.focus]
    }

    // --- Lo que se ve ---------------------------------------------------------------

    /// Documentos de los paneles de la pestaña activa, de izquierda a derecha.
    pub fn panes(&self) -> &[usize] {
        &self.cur().panes
    }

    pub fn weights(&self) -> &[f32] {
        &self.cur().weights
    }

    pub fn focus(&self) -> usize {
        self.cur().focus
    }

    pub fn active_tab(&self) -> usize {
        self.active
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Por pestaña: el documento que la representa (el de su panel con foco) y todos
    /// los suyos. Es lo que dibuja la fila de pestañas en modo Paneles.
    pub fn tab_groups(&self) -> Vec<(usize, Vec<usize>)> {
        self.tabs.iter().map(|t| (t.panes[t.focus], t.panes.clone())).collect()
    }

    /// Documentos de la pestaña que contiene a `doc`.
    pub fn tab_docs_of(&self, doc: usize) -> Vec<usize> {
        self.locate(doc).map(|(ti, _)| self.tabs[ti].panes.clone()).unwrap_or_else(|| vec![doc])
    }

    // --- Paneles --------------------------------------------------------------------

    /// Parte el panel con foco: el nuevo aparece a su derecha, con la mitad de su
    /// ancho, y toma el foco. Quien llama abre el documento que lo llenará
    /// (`reconcile` lo coloca). `false` si ya hay `MAX_PANES`.
    pub fn split(&mut self) -> bool {
        let t = self.cur_mut();
        if t.panes.len() >= MAX_PANES {
            return false;
        }
        let half = t.weights[t.focus] / 2.0;
        t.weights[t.focus] = half;
        t.focus += 1;
        t.panes.insert(t.focus, PENDING);
        t.weights.insert(t.focus, half);
        true
    }

    /// Mueve el foco un panel a la izquierda (`-1`) o derecha (`+1`) sin dar la
    /// vuelta. Devuelve el documento del nuevo panel con foco.
    pub fn move_focus(&mut self, dir: isize) -> Option<usize> {
        let t = self.cur_mut();
        let to = t.focus.checked_add_signed(dir).filter(|&i| i < t.panes.len())?;
        t.focus = to;
        Some(t.panes[to])
    }

    /// Como `move_focus`, pero dando la vuelta (Tab en el modo acomodar).
    pub fn cycle_focus(&mut self, dir: isize) -> usize {
        let t = self.cur_mut();
        let n = t.panes.len() as isize;
        t.focus = (t.focus as isize + dir).rem_euclid(n) as usize;
        t.panes[t.focus]
    }

    /// Enfoca el panel `i` de la pestaña activa (clic, Alt+número).
    pub fn focus_pane(&mut self, i: usize) -> Option<usize> {
        let t = self.cur_mut();
        let doc = *t.panes.get(i)?;
        t.focus = i;
        Some(doc)
    }

    /// Intercambia el panel con foco con su vecino; el foco se va con él.
    pub fn swap_pane(&mut self, dir: isize) -> bool {
        let t = self.cur_mut();
        let Some(to) = t.focus.checked_add_signed(dir).filter(|&i| i < t.panes.len()) else { return false };
        t.panes.swap(t.focus, to);
        t.weights.swap(t.focus, to);
        t.focus = to;
        true
    }

    /// Modo acomodar: ensancha (`delta > 0`) o estrecha el panel con foco a costa
    /// del vecino de la derecha (o el de la izquierda si es el último).
    pub fn resize(&mut self, delta: f32) -> bool {
        let t = self.cur_mut();
        if t.panes.len() < 2 {
            return false;
        }
        let f = t.focus;
        let n = if f + 1 < t.panes.len() { f + 1 } else { f - 1 };
        let delta = delta.min(t.weights[n] - MIN_WEIGHT).max(MIN_WEIGHT - t.weights[f]);
        if delta.abs() < 1e-4 {
            return false;
        }
        t.weights[f] += delta;
        t.weights[n] -= delta;
        true
    }

    /// Todos los paneles de la pestaña activa con el mismo ancho.
    pub fn equalize(&mut self) {
        let t = self.cur_mut();
        let n = t.panes.len() as f32;
        t.weights.iter_mut().for_each(|w| *w = 1.0 / n);
    }

    // --- Pestañas -------------------------------------------------------------------

    /// Pestaña `i` (Ctrl+número). Devuelve el documento que pasa a ser el activo.
    pub fn goto_tab(&mut self, i: usize) -> Option<usize> {
        (i < self.tabs.len()).then(|| {
            self.active = i;
            self.focused_doc()
        })
    }

    /// Siguiente/anterior pestaña, dando la vuelta.
    pub fn step_tab(&mut self, dir: isize) -> usize {
        let n = self.tabs.len() as isize;
        self.active = (self.active as isize + dir).rem_euclid(n) as usize;
        self.focused_doc()
    }

    /// Mueve la pestaña activa un puesto a la izquierda o derecha.
    pub fn move_tab(&mut self, dir: isize) -> bool {
        let Some(to) = self.active.checked_add_signed(dir).filter(|&i| i < self.tabs.len()) else { return false };
        self.tabs.swap(self.active, to);
        self.active = to;
        true
    }
}

/// Reparte `body` en columnas según `weights` (que suman 1), separadas por `DIVIDER_W`.
pub fn pane_rects(body: Rect, weights: &[f32]) -> Vec<Rect> {
    let n = weights.len().max(1);
    if weights.len() < 2 {
        return vec![body];
    }
    let avail = body.width() - DIVIDER_W * (n - 1) as f32;
    let mut left = body.left;
    let mut out = Vec::with_capacity(n);
    for (i, w) in weights.iter().enumerate() {
        let right = if i + 1 == n { body.right } else { left + avail * w };
        out.push(Rect::new(left, body.top, right, body.bottom));
        left = right + DIVIDER_W;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lay(n: usize) -> Layouts {
        let mut l = Layouts::default();
        l.reconcile(n, 0);
        l
    }

    #[test]
    fn every_open_doc_becomes_its_own_tab() {
        let l = lay(3);
        assert_eq!(l.tab_count(), 3);
        assert_eq!(l.tab_groups(), vec![(0, vec![0]), (1, vec![1]), (2, vec![2])]);
        assert_eq!(l.active_tab(), 0);
    }

    #[test]
    fn a_doc_opened_after_split_fills_the_new_pane() {
        let mut l = lay(1);
        assert!(l.split());
        // `Workspace::open` añade el documento 1 y lo activa.
        l.reconcile(2, 1);
        assert_eq!(l.tab_count(), 1);
        assert_eq!(l.panes(), &[0, 1]);
        assert_eq!(l.focus(), 1);
        assert!((l.weights()[0] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn a_doc_opened_without_split_goes_to_a_new_tab_after_the_active() {
        let mut l = lay(3);
        l.goto_tab(0);
        l.reconcile(4, 3);
        assert_eq!(l.tab_groups().iter().map(|g| g.0).collect::<Vec<_>>(), vec![0, 3, 1, 2]);
        assert_eq!(l.active_tab(), 1);
    }

    #[test]
    fn split_is_capped() {
        let mut l = lay(1);
        for d in 1..MAX_PANES {
            assert!(l.split());
            l.reconcile(d + 1, d);
        }
        assert!(!l.split());
        assert_eq!(l.panes().len(), MAX_PANES);
    }

    #[test]
    fn closing_a_pane_doc_shifts_indices_and_drops_empty_tabs() {
        let mut l = lay(1);
        l.split();
        l.reconcile(2, 1);
        l.reconcile(3, 2); // doc 2 en su propia pestaña
        assert_eq!(l.tab_count(), 2);
        // Se cierra el doc 1 (panel derecho de la primera pestaña).
        l.on_doc_closed(1);
        l.reconcile(2, 0);
        assert_eq!(l.tab_groups(), vec![(0, vec![0]), (1, vec![1])]);
        // Y ahora el único de la segunda pestaña: la pestaña desaparece.
        l.on_doc_closed(1);
        l.reconcile(1, 0);
        assert_eq!(l.tab_count(), 1);
    }

    #[test]
    fn activating_a_doc_selects_its_tab_and_pane() {
        let mut l = lay(1);
        l.split();
        l.reconcile(2, 1);
        l.reconcile(3, 2);
        l.reconcile(3, 0);
        assert_eq!((l.active_tab(), l.focus()), (0, 0));
        l.reconcile(3, 1);
        assert_eq!((l.active_tab(), l.focus()), (0, 1));
    }

    #[test]
    fn focus_moves_and_swaps_stop_at_the_edges() {
        let mut l = lay(1);
        l.split();
        l.reconcile(2, 1);
        assert_eq!(l.move_focus(1), None);
        assert_eq!(l.move_focus(-1), Some(0));
        assert!(!l.swap_pane(-1));
        assert!(l.swap_pane(1));
        assert_eq!((l.panes(), l.focus()), (&[1, 0][..], 1));
        assert_eq!(l.cycle_focus(1), 1);
    }

    #[test]
    fn resize_respects_the_minimum_and_equalize_resets() {
        let mut l = lay(1);
        l.split();
        l.reconcile(2, 1);
        l.focus_pane(0);
        assert!(l.resize(0.2));
        assert!((l.weights()[0] - 0.7).abs() < 1e-5);
        l.resize(5.0);
        assert!((l.weights()[1] - MIN_WEIGHT).abs() < 1e-5);
        assert!((l.weights().iter().sum::<f32>() - 1.0).abs() < 1e-5);
        l.equalize();
        assert!((l.weights()[0] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn tabs_step_goto_and_move() {
        let mut l = lay(3);
        assert_eq!(l.step_tab(-1), 2);
        assert_eq!(l.step_tab(1), 0);
        assert_eq!(l.goto_tab(1), Some(1));
        assert_eq!(l.goto_tab(9), None);
        assert!(l.move_tab(1));
        assert_eq!(l.tab_groups().iter().map(|g| g.0).collect::<Vec<_>>(), vec![0, 2, 1]);
        assert_eq!(l.active_tab(), 2);
        assert!(!l.move_tab(1));
    }

    #[test]
    fn pane_rects_follow_the_weights() {
        let body = Rect::new(0.0, 0.0, 301.0, 100.0);
        let r = pane_rects(body, &[0.25, 0.75]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].left, 0.0);
        assert!((r[0].width() - 75.0).abs() < 1e-3);
        assert_eq!(r[1].left - r[0].right, DIVIDER_W);
        assert_eq!(r[1].right, 301.0);
        assert_eq!(pane_rects(body, &[1.0]), vec![body]);
    }
}
