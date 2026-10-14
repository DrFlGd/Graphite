//! Debounced file watching so external edits (another app, a sync client)
//! show up in Graphite. The watcher only reports; the caller decides what
//! to reindex (usually `Vault::reindex_path`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};

use crate::error::Result;
use crate::vault::STATE_DIR;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultEvent {
    /// Absolute paths of `.md` files (or directories) that changed.
    pub paths: Vec<PathBuf>,
}

pub struct Watcher {
    _debouncer: Debouncer<notify::RecommendedWatcher>,
}

impl Watcher {
    /// Watch `root` and call `on_change` with batches of changed paths.
    /// Paths are reported under `root` as given (the OS may resolve symlinks,
    /// e.g. `/var` -> `/private/var` on macOS), so callers can strip the
    /// root they opened the vault with. The root itself, anything under
    /// `.graphite/` and non-Markdown files are filtered out.
    pub fn new<F>(root: &Path, mut on_change: F) -> Result<Watcher>
    where
        F: FnMut(VaultEvent) + Send + 'static,
    {
        let root = root.to_path_buf();
        let canon = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
        let state_dir = root.join(STATE_DIR);
        let mut debouncer = new_debouncer(Duration::from_millis(300), {
            let root = root.clone();
            move |res: DebounceEventResult| {
                if let Ok(events) = res {
                    let mut paths: Vec<PathBuf> = events
                        .into_iter()
                        .map(|e| rebase(&root, &canon, e.path))
                        .filter(|p| relevant(&root, &state_dir, p))
                        .collect();
                    paths.sort();
                    paths.dedup();
                    if !paths.is_empty() {
                        on_change(VaultEvent { paths });
                    }
                }
            }
        })?;
        debouncer.watcher().watch(&root, RecursiveMode::Recursive)?;
        Ok(Watcher { _debouncer: debouncer })
    }
}

/// Express `path` under the root the caller gave us when the OS reported it
/// under the canonical (symlink-resolved) root instead.
fn rebase(root: &Path, canon: &Path, path: PathBuf) -> PathBuf {
    if root == canon || path.starts_with(root) {
        return path;
    }
    match path.strip_prefix(canon) {
        Ok(rel) => root.join(rel),
        Err(_) => path,
    }
}

/// Keep Markdown files and directories inside the vault, but never the root
/// itself or anything under `.graphite/`.
fn relevant(root: &Path, state_dir: &Path, path: &Path) -> bool {
    if path == root || path.starts_with(state_dir) {
        return false;
    }
    if path.components().any(|c| c.as_os_str() == STATE_DIR) {
        return false;
    }
    path.extension().and_then(|e| e.to_str()) == Some("md") || path.is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn reports_markdown_changes_and_ignores_state_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(STATE_DIR)).unwrap();
        let (tx, rx) = mpsc::channel();
        let _w = Watcher::new(dir.path(), move |ev| {
            let _ = tx.send(ev);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(dir.path().join(STATE_DIR).join("index.sqlite"), "x").unwrap();
        std::fs::write(dir.path().join("note.md"), "hello").unwrap();
        // The OS may deliver the change in more than one batch (FSEvents on
        // macOS does), so collect until the note shows up.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut seen_note = false;
        while !seen_note {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let ev = rx.recv_timeout(remaining).expect("watcher event with note.md");
            assert!(
                !ev.paths.iter().any(|p| p.to_string_lossy().contains(STATE_DIR)),
                "state dir leaked into {:?}",
                ev.paths
            );
            seen_note = ev.paths.iter().any(|p| p.ends_with("note.md"));
        }
    }

    #[test]
    fn rebases_canonical_paths_onto_the_opened_root() {
        let root = Path::new("/var/vault");
        let canon = Path::new("/private/var/vault");
        assert_eq!(
            rebase(root, canon, PathBuf::from("/private/var/vault/a/b.md")),
            PathBuf::from("/var/vault/a/b.md")
        );
        assert_eq!(
            rebase(root, canon, PathBuf::from("/private/var/vault")),
            PathBuf::from("/var/vault")
        );
        assert_eq!(
            rebase(root, canon, PathBuf::from("/var/vault/a.md")),
            PathBuf::from("/var/vault/a.md")
        );
        assert_eq!(
            rebase(root, root, PathBuf::from("/var/vault/a.md")),
            PathBuf::from("/var/vault/a.md")
        );
        assert_eq!(
            rebase(root, canon, PathBuf::from("/elsewhere/a.md")),
            PathBuf::from("/elsewhere/a.md")
        );
    }

    #[test]
    fn keeps_notes_and_folders_but_not_root_state_dir_or_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let state = root.join(STATE_DIR);
        std::fs::create_dir_all(&state).unwrap();
        std::fs::create_dir_all(root.join("notes")).unwrap();
        assert!(!relevant(root, &state, root));
        assert!(!relevant(root, &state, &state));
        assert!(!relevant(root, &state, &state.join("index.sqlite")));
        assert!(!relevant(root, &state, &root.join("image.png")));
        assert!(!relevant(root, &state, &root.join("missing-dir")));
        assert!(relevant(root, &state, &root.join("notes")));
        assert!(relevant(root, &state, &root.join("notes/a.md")));
        assert!(relevant(root, &state, &root.join("gone.md")));
    }
}
