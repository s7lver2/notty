use std::io;
use std::path::{Path, PathBuf};

use notty_io::{Opened, RawBytes};

pub struct RawDoc {
    path: PathBuf,
    view: RawBytes,
    writable_fs: bool,
    edit_buf: Option<Vec<u8>>,
    dirty: bool,
}

pub fn open_raw_doc(path: &Path) -> io::Result<RawDoc> {
    match notty_io::open_raw(path)? {
        Opened::Raw { bytes, writable } => {
            Ok(RawDoc { path: path.to_path_buf(), view: bytes, writable_fs: writable, edit_buf: None, dirty: false })
        }
        Opened::Text { .. } => Err(io::Error::new(io::ErrorKind::InvalidData, "open_raw debería devolver siempre Raw")),
    }
}

impl RawDoc {
    pub fn len(&self) -> usize {
        self.edit_buf.as_ref().map_or_else(|| self.view.len(), |v| v.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn byte(&self, i: usize) -> u8 {
        self.edit_buf.as_ref().map_or_else(|| self.view[i], |v| v[i])
    }

    pub fn writable_fs(&self) -> bool {
        self.writable_fs
    }

    pub fn is_editing(&self) -> bool {
        self.edit_buf.is_some()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn enable_write(&mut self) -> bool {
        if !self.writable_fs {
            return false;
        }
        if self.edit_buf.is_none() {
            self.edit_buf = Some(self.view.to_vec());
        }
        true
    }

    pub fn set_byte(&mut self, i: usize, value: u8) -> bool {
        match &mut self.edit_buf {
            Some(buf) if i < buf.len() => {
                buf[i] = value;
                self.dirty = true;
                true
            }
            _ => false,
        }
    }

    /// Guarda los bytes editados. Suelta la vista mapeada del archivo *antes* de
    /// escribir, para no intentar nunca renombrar encima de un `Mmap` abierto.
    pub fn save(&mut self) -> io::Result<()> {
        let Some(buf) = self.edit_buf.take() else { return Ok(()) };
        self.view = RawBytes::Owned(Vec::new());
        notty_io::atomic_write(&self.path, &buf)?;
        self.edit_buf = Some(buf);
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn opens_with_the_same_bytes_as_the_file() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8, 2, 3, 4]).unwrap();
        let doc = open_raw_doc(&p).unwrap();
        assert_eq!(doc.len(), 4);
        assert_eq!(doc.byte(2), 3);
        assert!(!doc.is_editing());
        assert!(!doc.is_dirty());
    }

    #[test]
    fn writable_file_reports_writable_fs() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8]).unwrap();
        assert!(open_raw_doc(&p).unwrap().writable_fs());
    }

    #[test]
    #[allow(clippy::permissions_set_readonly_false)]
    fn readonly_file_reports_not_writable_and_enable_write_fails() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8]).unwrap();
        let mut perms = fs::metadata(&p).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&p, perms.clone()).unwrap();
        let mut doc = open_raw_doc(&p).unwrap();
        assert!(!doc.writable_fs());
        assert!(!doc.enable_write());
        assert!(!doc.is_editing());
        perms.set_readonly(false);
        fs::set_permissions(&p, perms).unwrap();
    }

    #[test]
    fn enable_write_then_set_byte_updates_in_memory_only() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8, 2, 3]).unwrap();
        let mut doc = open_raw_doc(&p).unwrap();
        assert!(doc.enable_write());
        assert!(doc.set_byte(1, 0xFF));
        assert_eq!(doc.byte(1), 0xFF);
        assert!(doc.is_dirty());
        assert_eq!(fs::read(&p).unwrap(), vec![1, 2, 3]); // el disco no cambia todavía
    }

    #[test]
    fn set_byte_without_enable_write_fails() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8]).unwrap();
        let mut doc = open_raw_doc(&p).unwrap();
        assert!(!doc.set_byte(0, 9));
    }

    #[test]
    fn save_writes_the_edited_bytes_and_clears_dirty() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8, 2, 3]).unwrap();
        let mut doc = open_raw_doc(&p).unwrap();
        doc.enable_write();
        doc.set_byte(0, 9);
        doc.save().unwrap();
        assert!(!doc.is_dirty());
        assert_eq!(fs::read(&p).unwrap(), vec![9, 2, 3]);
    }

    #[test]
    fn save_without_edits_is_a_no_op() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, [1u8]).unwrap();
        let mut doc = open_raw_doc(&p).unwrap();
        assert!(doc.save().is_ok());
        assert_eq!(fs::read(&p).unwrap(), vec![1]);
    }
}
