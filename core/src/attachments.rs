//! Bringing files into the vault. Files are copied, never modified, and
//! never overwrite an existing attachment (ADR-002).

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::vault::{Vault, ATTACHMENTS_DIR};

impl Vault {
    /// Copy a file from anywhere on disk into `attachments/` (or `folder`)
    /// and return its vault-relative path, the form that goes into Markdown.
    pub fn import_attachment(&self, src: &Path, folder: Option<&str>) -> Result<String> {
        let name = src
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::PathEscapesVault(src.to_path_buf()))?;
        if !src.is_file() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                src.display().to_string(),
            )));
        }
        // A file already inside the vault is referenced where it is.
        if let Ok(rel) = self.rel(&src.canonicalize()?) {
            return Ok(rel);
        }
        let (rel, dest) = self.free_attachment_path(folder, name)?;
        fs::copy(src, &dest)?;
        Ok(rel)
    }

    /// Write bytes (a pasted image, a download) as a new attachment.
    pub fn write_attachment(&self, name: &str, data: &[u8], folder: Option<&str>) -> Result<String> {
        let name = safe_attachment_name(name);
        let (rel, dest) = self.free_attachment_path(folder, &name)?;
        fs::write(&dest, data)?;
        Ok(rel)
    }

    /// First unused `<folder>/<name>` as `(rel, abs)`, suffixing `-2`, `-3`… on collision.
    fn free_attachment_path(&self, folder: Option<&str>, name: &str) -> Result<(String, PathBuf)> {
        let folder = folder.unwrap_or(ATTACHMENTS_DIR).trim_matches('/');
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
            _ => (name.to_string(), String::new()),
        };
        let mut rel = format!("{folder}/{name}");
        let mut dest = self.safe_join(&rel)?;
        let mut n = 1;
        while dest.exists() {
            n += 1;
            rel = format!("{folder}/{stem}-{n}{ext}");
            dest = self.safe_join(&rel)?;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok((rel, dest))
    }
}

/// Keep attachment names portable: strip path separators and control
/// characters, fall back to `file` when nothing is left.
pub fn safe_attachment_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "file".into()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_copy_and_never_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        let src = tempfile::tempdir().unwrap();
        let file = src.path().join("photo.png");
        fs::write(&file, b"one").unwrap();
        let a = v.import_attachment(&file, None).unwrap();
        assert_eq!(a, "attachments/photo.png");
        fs::write(&file, b"two").unwrap();
        let b = v.import_attachment(&file, None).unwrap();
        assert_eq!(b, "attachments/photo-2.png");
        assert_eq!(fs::read(dir.path().join(&a)).unwrap(), b"one");
        assert_eq!(fs::read(dir.path().join(&b)).unwrap(), b"two");
        // Already in the vault: referenced, not copied.
        let again = v.import_attachment(&dir.path().join(&a), None).unwrap();
        assert_eq!(again, a);
        assert!(v.import_attachment(&src.path().join("missing.png"), None).is_err());
    }

    #[test]
    fn writes_bytes_into_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        let rel = v
            .write_attachment("clip:board?.png", b"\x89PNG", Some("attachments/Inbox"))
            .unwrap();
        assert_eq!(rel, "attachments/Inbox/clip-board-.png");
        assert!(dir.path().join(&rel).is_file());
        assert_eq!(safe_attachment_name("  ..  "), "file");
    }
}
