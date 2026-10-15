//! Bringing files into the vault. Files are copied, never modified, and
//! never overwrite an existing attachment (ADR-002).

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::vault::{Vault, ATTACHMENTS_DIR};

impl Vault {
    /// Copy a file from anywhere on disk into `attachments/` (or `folder`)
    /// and return its vault-relative path, the form that goes into Markdown.
    /// A file larger than the vault's large-file limit is not copied: the
    /// result is then a `file://` URL to where it is.
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
        let abs = src.canonicalize()?;
        if let Ok(rel) = self.rel(&abs) {
            return Ok(rel);
        }
        // So is one over the large-file limit (E9): the link points at it
        // on this device rather than copying it into the vault.
        if fs::metadata(&abs)?.len() > self.large_file_bytes() {
            return Ok(file_url(&abs));
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
/// A `file://` URL for an absolute path, with `/` separators and the
/// characters Markdown or a browser would trip over percent-encoded.
pub fn file_url(abs: &Path) -> String {
    let text = abs.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        out.push('/');
    }
    for b in text.bytes() {
        let keep = b.is_ascii_alphanumeric() || b"/-._~:".contains(&b);
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

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
    fn large_files_are_referenced_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        v.save_config(&crate::config::VaultConfig { large_file_mb: 0 }).unwrap();
        let src = tempfile::tempdir().unwrap();
        let file = src.path().join("big movie.mov");
        fs::write(&file, b"mov").unwrap();
        let link = v.import_attachment(&file, None).unwrap();
        assert!(link.starts_with("file:///"), "{link}");
        assert!(link.ends_with("/big%20movie.mov"), "{link}");
        assert!(!dir.path().join("attachments/big movie.mov").exists());
        assert_eq!(
            file_url(Path::new("C:\\Users\\me\\a b.png")),
            "file:///C:/Users/me/a%20b.png"
        );
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
