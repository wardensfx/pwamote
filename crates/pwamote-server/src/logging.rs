//! Journal sur disque pour l'application de bureau, qui n'a pas de console.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// Au-delà, le journal est mis de côté (un seul ancien fichier conservé).
pub const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

/// `<données>\logs\pwamote.log`.
pub fn log_path(data_dir: &Path) -> PathBuf {
    data_dir.join("logs").join("pwamote.log")
}

/// Ouvre le journal en ajout, après avoir renommé en `pwamote.old.log` un fichier trop gros.
pub fn open_log(data_dir: &Path) -> io::Result<File> {
    let path = log_path(data_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        fs::rename(&path, path.with_file_name("pwamote.old.log"))?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn cree_le_dossier_et_ajoute() {
        let dir = tempfile::tempdir().unwrap();
        writeln!(open_log(dir.path()).unwrap(), "un").unwrap();
        writeln!(open_log(dir.path()).unwrap(), "deux").unwrap();
        let text = fs::read_to_string(log_path(dir.path())).unwrap();
        assert_eq!(text, "un\ndeux\n");
    }

    #[test]
    fn met_de_cote_un_journal_trop_gros() {
        let dir = tempfile::tempdir().unwrap();
        let path = log_path(dir.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![b'x'; MAX_LOG_BYTES as usize + 1]).unwrap();
        writeln!(open_log(dir.path()).unwrap(), "neuf").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "neuf\n");
        let old = path.with_file_name("pwamote.old.log");
        assert_eq!(fs::metadata(old).unwrap().len(), MAX_LOG_BYTES + 1);
    }
}
