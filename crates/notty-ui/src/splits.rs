//! Paneles divididos (`Files::Splits`): hasta 3 documentos de `Workspace` visibles a
//! la vez, lado a lado. Cada panel es solo el índice de un documento que ya está
//! abierto; el panel con foco siempre muestra `ws.active_index()`, así que todo lo
//! que ya enruta la entrada al documento activo sigue valiendo tal cual.

use crate::layout::Rect;

pub const MAX_PANES: usize = 3;
/// Ancho de la línea divisoria entre paneles.
pub const DIVIDER_W: f32 = 1.0;

#[derive(Debug, Clone)]
pub struct Splits {
    panes: Vec<usize>,
    focus: usize,
}

impl Default for Splits {
    fn default() -> Self {
        Self { panes: vec![0], focus: 0 }
    }
}

impl Splits {
    pub fn panes(&self) -> &[usize] {
        &self.panes
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    /// El panel con foco sigue al documento activo (Ctrl+Tab, abrir, clic en una pestaña...).
    pub fn sync(&mut self, active: usize) {
        self.panes[self.focus] = active;
    }

    /// Parte el panel con foco: el nuevo aparece a su derecha y toma el foco. Devuelve
    /// el documento que debe mostrar (el primero que no se ve en ningún panel), o
    /// `Some(None)` si ya se ven todos y hay que abrir uno vacío. `None` = ya hay 3.
    pub fn split(&mut self, doc_count: usize) -> Option<Option<usize>> {
        if self.panes.len() >= MAX_PANES {
            return None;
        }
        let doc = (0..doc_count).find(|d| !self.panes.contains(d));
        self.focus += 1;
        // Provisional: quien llama activa `doc` (o abre uno nuevo) y hace `sync`.
        self.panes.insert(self.focus, doc.unwrap_or(0));
        Some(doc)
    }

    /// Cierra el panel con foco (no el documento) y devuelve el documento del panel
    /// que hereda el foco, el de su izquierda si lo hay. `None` si solo queda uno.
    pub fn close_focused(&mut self) -> Option<usize> {
        if self.panes.len() <= 1 {
            return None;
        }
        self.panes.remove(self.focus);
        self.focus = self.focus.saturating_sub(1);
        Some(self.panes[self.focus])
    }

    /// Mueve el foco un panel a la izquierda (`-1`) o derecha (`+1`) sin dar la vuelta.
    /// Devuelve el documento del nuevo panel con foco, o `None` si no hay panel ahí.
    pub fn move_focus(&mut self, dir: isize) -> Option<usize> {
        let to = self.focus.checked_add_signed(dir).filter(|&i| i < self.panes.len())?;
        self.focus = to;
        Some(self.panes[to])
    }

    /// Enfoca el panel `i` (clic dentro de él) y devuelve su documento.
    pub fn focus_pane(&mut self, i: usize) -> Option<usize> {
        let doc = *self.panes.get(i)?;
        self.focus = i;
        Some(doc)
    }

    /// Tras `Workspace::close(idx)` cuando la lista de verdad se acortó: los índices
    /// posteriores bajan uno, y los paneles que mostraban el cerrado pasan al vecino.
    pub fn on_doc_closed(&mut self, idx: usize, doc_count: usize) {
        for d in &mut self.panes {
            if *d > idx || (*d == idx && idx >= doc_count) {
                *d -= 1;
            }
        }
    }
}

/// Reparte `body` en `n` columnas de igual ancho separadas por `DIVIDER_W`.
pub fn pane_rects(body: Rect, n: usize) -> Vec<Rect> {
    let n = n.max(1);
    let col = (body.width() - DIVIDER_W * (n - 1) as f32) / n as f32;
    (0..n)
        .map(|i| {
            let left = body.left + i as f32 * (col + DIVIDER_W);
            Rect::new(left, body.top, left + col, body.bottom)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn splits(panes: &[usize], focus: usize) -> Splits {
        Splits { panes: panes.to_vec(), focus }
    }

    #[test]
    fn split_picks_the_first_hidden_doc_and_caps_at_three() {
        let mut s = Splits::default();
        assert_eq!(s.split(3), Some(Some(1)));
        assert_eq!((s.panes(), s.focus()), (&[0, 1][..], 1));
        assert_eq!(s.split(3), Some(Some(2)));
        assert_eq!(s.split(3), None);
        assert_eq!(s.panes().len(), 3);
    }

    #[test]
    fn split_with_every_doc_visible_asks_for_a_new_one() {
        let mut s = Splits::default();
        assert_eq!(s.split(1), Some(None));
    }

    #[test]
    fn split_inserts_right_of_the_focused_pane() {
        let mut s = splits(&[0, 1], 0);
        s.split(3);
        assert_eq!((s.panes(), s.focus()), (&[0, 2, 1][..], 1));
    }

    #[test]
    fn closing_focuses_the_left_neighbour_and_keeps_the_last_pane() {
        let mut s = splits(&[0, 1, 2], 1);
        assert_eq!(s.close_focused(), Some(0));
        assert_eq!((s.panes(), s.focus()), (&[0, 2][..], 0));
        assert_eq!(s.close_focused(), Some(2));
        assert_eq!(s.close_focused(), None);
        assert_eq!(s.panes(), &[2]);
    }

    #[test]
    fn move_focus_stops_at_the_edges() {
        let mut s = splits(&[4, 5], 0);
        assert_eq!(s.move_focus(-1), None);
        assert_eq!(s.move_focus(1), Some(5));
        assert_eq!(s.move_focus(1), None);
        assert_eq!(s.focus(), 1);
    }

    #[test]
    fn closing_a_doc_shifts_later_indices() {
        let mut s = splits(&[0, 2, 3], 0);
        s.on_doc_closed(1, 3);
        assert_eq!(s.panes(), &[0, 1, 2]);
        // Se cerró el último documento (3 de 0..=3): su panel pasa al anterior.
        let mut s = splits(&[0, 3], 0);
        s.on_doc_closed(3, 3);
        assert_eq!(s.panes(), &[0, 2]);
    }

    #[test]
    fn pane_rects_split_evenly_with_a_divider() {
        let body = Rect::new(10.0, 20.0, 312.0, 100.0);
        let r = pane_rects(body, 3);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].left, 10.0);
        assert_eq!(r[2].right, 312.0);
        assert_eq!(r[1].left - r[0].right, DIVIDER_W);
        assert!((r[0].width() - r[1].width()).abs() < 1e-3);
        assert_eq!(pane_rects(body, 1), vec![body]);
    }
}
