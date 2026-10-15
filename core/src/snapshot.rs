//! Vault snapshots (M4, I6): a zip of everything in the vault except the
//! `.graphite/` cache, taken on a schedule into `.graphite/snapshots/` (or
//! a folder of the user's choosing) and pruned to the newest few. A
//! snapshot is a plain archive anything can open; restoring is unzipping.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::error::Result;
use crate::vault::{Vault, STATE_DIR};

pub const SNAPSHOT_DIR: &str = "snapshots";
/// Snapshots kept per folder; older ones are deleted after a new one.
pub const KEEP_SNAPSHOTS: usize = 10;
/// How often the app takes one.
pub const DEFAULT_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(24 * 3600);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    /// Absolute path of the zip.
    pub path: String,
    pub name: String,
    pub time: DateTime<Utc>,
    pub size: u64,
    /// Files in the archive (only known for one just taken).
    pub files: Option<usize>,
}

fn is_snapshot_name(name: &str) -> bool {
    name.ends_with(".zip") && name.contains("-20")
}

impl Vault {
    /// Where snapshots go unless a folder is given: `.graphite/snapshots/`,
    /// or `GRAPHITE_SNAPSHOT_DIR`.
    pub fn snapshot_dir(&self) -> PathBuf {
        std::env::var_os("GRAPHITE_SNAPSHOT_DIR")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| self.root().join(STATE_DIR).join(SNAPSHOT_DIR))
    }

    /// Zip the vault (notes, attachments, everything but `.graphite/`) into
    /// `dir`, then keep only the newest `KEEP_SNAPSHOTS` there.
    pub fn snapshot(&self, dir: Option<&Path>) -> Result<Snapshot> {
        let dir = dir.map(Path::to_path_buf).unwrap_or_else(|| self.snapshot_dir());
        fs::create_dir_all(&dir)?;
        let vault_name = self.info()?.name;
        let now = Utc::now();
        let stamp = now.format("%Y%m%d-%H%M%S");
        let mut name = format!("{vault_name}-{stamp}.zip");
        let mut n = 1;
        while dir.join(&name).exists() {
            n += 1;
            name = format!("{vault_name}-{stamp}-{n}.zip");
        }
        let path = dir.join(&name);
        let tmp = dir.join(format!("{name}.part"));
        let mut zip = ZipWriter::new(File::create(&tmp)?);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .large_file(true);
        let mut files = 0usize;
        let mut outcome = Ok(());
        for entry in ignore::WalkBuilder::new(self.root())
            .hidden(false)
            .git_ignore(false)
            .filter_entry(|e| e.file_name() != STATE_DIR)
            .build()
            .flatten()
        {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            // Never archive the folder being written into.
            if p.starts_with(&dir) {
                continue;
            }
            let rel = match self.rel(p) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let result = (|| -> io::Result<()> {
                zip.start_file(rel, options)?;
                let mut f = File::open(p)?;
                io::copy(&mut f, &mut zip)?;
                Ok(())
            })();
            if let Err(e) = result {
                outcome = Err(e);
                break;
            }
            files += 1;
        }
        if let Err(e) = outcome {
            let _ = fs::remove_file(&tmp);
            return Err(e.into());
        }
        let mut inner = zip.finish().map_err(io::Error::other)?;
        inner.flush()?;
        drop(inner);
        fs::rename(&tmp, &path)?;
        let size = fs::metadata(&path)?.len();
        self.prune_snapshots(&dir)?;
        Ok(Snapshot {
            path: path.to_string_lossy().into_owned(),
            name,
            time: now,
            size,
            files: Some(files),
        })
    }

    /// Snapshots in `dir` (default folder when `None`), newest first.
    pub fn snapshots(&self, dir: Option<&Path>) -> Result<Vec<Snapshot>> {
        let dir = dir.map(Path::to_path_buf).unwrap_or_else(|| self.snapshot_dir());
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(out);
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_snapshot_name(&name) {
                continue;
            }
            let meta = entry.metadata()?;
            let time: DateTime<Utc> = meta.modified().map(DateTime::from).unwrap_or_else(|_| Utc::now());
            out.push(Snapshot {
                path: entry.path().to_string_lossy().into_owned(),
                name,
                time,
                size: meta.len(),
                files: None,
            });
        }
        out.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| b.name.cmp(&a.name)));
        Ok(out)
    }

    /// Take a snapshot if the newest one is older than `interval` (or there
    /// is none). Returns it when one was taken.
    pub fn snapshot_if_due(&self, interval: Duration) -> Result<Option<Snapshot>> {
        let newest = self.snapshots(None)?.into_iter().next();
        let due = match newest {
            Some(s) => Utc::now().signed_duration_since(s.time).to_std().unwrap_or_default() >= interval,
            None => true,
        };
        if due {
            self.snapshot(None).map(Some)
        } else {
            Ok(None)
        }
    }

    fn prune_snapshots(&self, dir: &Path) -> Result<()> {
        for old in self.snapshots(Some(dir))?.into_iter().skip(KEEP_SNAPSHOTS) {
            let _ = fs::remove_file(old.path);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_holds_notes_and_attachments_but_not_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::open(dir.path()).unwrap();
        v.create("Plan", None, None).unwrap();
        fs::write(dir.path().join("attachments/photo.png"), b"png").unwrap();
        let snap = v.snapshot(None).unwrap();
        assert_eq!(snap.files, Some(2));
        assert!(Path::new(&snap.path).is_file());
        let listed = v.snapshots(None).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, snap.name);

        let mut zip = zip::ZipArchive::new(File::open(&snap.path).unwrap()).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"notes/Plan.md".to_string()));
        assert!(names.contains(&"attachments/photo.png".to_string()));
        assert!(names.iter().all(|n| !n.starts_with(".graphite")));

        // Not due again right away; due with a zero interval.
        assert!(v.snapshot_if_due(Duration::from_secs(3600)).unwrap().is_none());
        assert!(v.snapshot_if_due(Duration::ZERO).unwrap().is_some());
        assert_eq!(v.snapshots(None).unwrap().len(), 2);
    }
}
