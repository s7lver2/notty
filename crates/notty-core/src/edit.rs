use crate::Buffer;

/// Un cambio reversible: en `at` se quitó `removed` y se puso `inserted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub at: usize,
    pub removed: String,
    pub inserted: String,
}

impl Edit {
    pub fn apply(&self, buf: &mut Buffer) {
        let n = self.removed.chars().count();
        buf.remove(self.at..self.at + n);
        buf.insert(self.at, &self.inserted);
    }

    pub fn inverse(&self) -> Edit {
        Edit { at: self.at, removed: self.inserted.clone(), inserted: self.removed.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_undoes_apply() {
        let mut b = Buffer::new("hola mundo");
        let e = Edit { at: 5, removed: "mundo".into(), inserted: "notty".into() };
        e.apply(&mut b);
        assert_eq!(b.to_string(), "hola notty");
        e.inverse().apply(&mut b);
        assert_eq!(b.to_string(), "hola mundo");
    }
}
