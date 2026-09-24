use std::time::{Duration, Instant};

use crate::{Buffer, Edit};

const GROUP_WINDOW: Duration = Duration::from_millis(1000);

#[derive(Debug)]
struct Group {
    id: u64,
    edits: Vec<Edit>,
}

/// Deshacer/rehacer por grupos. Cada grupo tiene un id único: el documento
/// compara `top_id()` con el id guardado para saber si está "sucio".
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Group>,
    redo: Vec<Group>,
    next_id: u64,
    last: Option<Instant>,
    sealed: bool,
}

impl History {
    pub fn record(&mut self, edit: Edit, now: Instant) {
        self.redo.clear();
        let recent = self.last.is_some_and(|t| now.saturating_duration_since(t) < GROUP_WINDOW);
        let joins = !self.sealed
            && recent
            && self.undo.last().and_then(|g| g.edits.last()).is_some_and(|prev| continues(prev, &edit));
        if joins {
            self.undo.last_mut().expect("joins implica grupo").edits.push(edit);
        } else {
            self.next_id += 1;
            self.undo.push(Group { id: self.next_id, edits: vec![edit] });
        }
        self.last = Some(now);
        self.sealed = false;
    }

    /// Corta el grupo actual (mover el cursor, guardar, deshacer...).
    pub fn seal(&mut self) {
        self.sealed = true;
    }

    pub fn top_id(&self) -> Option<u64> {
        self.undo.last().map(|g| g.id)
    }

    pub fn undo(&mut self, buf: &mut Buffer) -> Option<usize> {
        let group = self.undo.pop()?;
        for e in group.edits.iter().rev() {
            e.inverse().apply(buf);
        }
        let first = &group.edits[0];
        let cursor = first.at + first.removed.chars().count();
        self.redo.push(group);
        self.sealed = true;
        Some(cursor)
    }

    pub fn redo(&mut self, buf: &mut Buffer) -> Option<usize> {
        let group = self.redo.pop()?;
        for e in &group.edits {
            e.apply(buf);
        }
        let last = group.edits.last().expect("grupo no vacío");
        let cursor = last.at + last.inserted.chars().count();
        self.undo.push(group);
        self.sealed = true;
        Some(cursor)
    }
}

fn continues(prev: &Edit, next: &Edit) -> bool {
    let typing = prev.removed.is_empty()
        && next.removed.is_empty()
        && next.at == prev.at + prev.inserted.chars().count()
        && !next.inserted.contains('\n');
    let erasing = prev.inserted.is_empty()
        && next.inserted.is_empty()
        && next.at + next.removed.chars().count() == prev.at;
    typing || erasing
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn type_at(b: &mut Buffer, h: &mut History, at: usize, s: &str, now: Instant) {
        let e = Edit { at, removed: String::new(), inserted: s.into() };
        e.apply(b);
        h.record(e, now);
    }

    #[test]
    fn fast_typing_is_one_undo_step() {
        let (mut b, mut h, t) = (Buffer::new(""), History::default(), Instant::now());
        type_at(&mut b, &mut h, 0, "a", t);
        type_at(&mut b, &mut h, 1, "b", t + ms(200));
        assert_eq!(h.undo(&mut b), Some(0));
        assert_eq!(b.to_string(), "");
        assert_eq!(h.undo(&mut b), None);
    }

    #[test]
    fn pause_starts_new_group() {
        let (mut b, mut h, t) = (Buffer::new(""), History::default(), Instant::now());
        type_at(&mut b, &mut h, 0, "a", t);
        type_at(&mut b, &mut h, 1, "b", t + ms(1500));
        h.undo(&mut b);
        assert_eq!(b.to_string(), "a");
    }

    #[test]
    fn seal_starts_new_group() {
        let (mut b, mut h, t) = (Buffer::new(""), History::default(), Instant::now());
        type_at(&mut b, &mut h, 0, "a", t);
        h.seal();
        type_at(&mut b, &mut h, 1, "b", t);
        h.undo(&mut b);
        assert_eq!(b.to_string(), "a");
    }

    #[test]
    fn newline_starts_new_group() {
        let (mut b, mut h, t) = (Buffer::new(""), History::default(), Instant::now());
        type_at(&mut b, &mut h, 0, "a", t);
        type_at(&mut b, &mut h, 1, "\r\n", t);
        h.undo(&mut b);
        assert_eq!(b.to_string(), "a");
    }

    #[test]
    fn backspaces_group_and_restore_cursor() {
        let (mut b, mut h, t) = (Buffer::new("abc"), History::default(), Instant::now());
        for (at, ch) in [(2, "c"), (1, "b")] {
            let e = Edit { at, removed: ch.into(), inserted: String::new() };
            e.apply(&mut b);
            h.record(e, t);
        }
        assert_eq!(b.to_string(), "a");
        assert_eq!(h.undo(&mut b), Some(3));
        assert_eq!(b.to_string(), "abc");
    }

    #[test]
    fn redo_reapplies_and_new_edit_clears_redo() {
        let (mut b, mut h, t) = (Buffer::new(""), History::default(), Instant::now());
        type_at(&mut b, &mut h, 0, "ab", t);
        h.undo(&mut b);
        assert_eq!(h.redo(&mut b), Some(2));
        assert_eq!(b.to_string(), "ab");
        h.undo(&mut b);
        type_at(&mut b, &mut h, 0, "x", t);
        assert_eq!(h.redo(&mut b), None);
    }

    #[test]
    fn top_id_follows_undo_and_redo() {
        let (mut b, mut h, t) = (Buffer::new(""), History::default(), Instant::now());
        assert_eq!(h.top_id(), None);
        type_at(&mut b, &mut h, 0, "a", t);
        let id = h.top_id();
        assert!(id.is_some());
        h.undo(&mut b);
        assert_eq!(h.top_id(), None);
        h.redo(&mut b);
        assert_eq!(h.top_id(), id);
    }
}
