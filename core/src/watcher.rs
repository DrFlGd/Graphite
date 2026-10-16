//! Debounced file watching so external edits (another app, a sync client)
//! show up in Graphite. The watcher only reports; the caller decides what
//! to reindex (usually `Vault::reindex_path`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

use crate::error::Result;
use crate::vault::STATE_DIR;

/// Quiet time before a batch of changes is reported.
const DEBOUNCE: Duration = Duration::from_millis(300);
/// A batch is reported after this long even while changes keep coming.
const MAX_BATCH: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultEvent {
    /// Absolute paths of files (notes or assets) or directories that changed.
    pub paths: Vec<PathBuf>,
}

pub struct Watcher {
    _watcher: RecommendedWatcher,
}

impl Watcher {
    /// Watch `root` and call `on_change` with batches of changed paths.
    /// Paths are reported under `root` as given (the OS may resolve symlinks,
    /// e.g. `/var` -> `/private/var` on macOS), so callers can strip the
    /// root they opened the vault with. The root itself, anything under
    /// `.graphite/` and hidden files are filtered out, and so is a file
    /// merely being opened or read: on Linux those are reported too, and
    /// reacting to them (by reading the file again) would never settle.
    pub fn new<F>(root: &Path, mut on_change: F) -> Result<Watcher>
    where
        F: FnMut(VaultEvent) + Send + 'static,
    {
        let root = root.to_path_buf();
        let canon = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
        let state_dir = root.join(STATE_DIR);
        let (tx, rx) = mpsc::channel::<Event>();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                if changes_content(&event.kind) {
                    let _ = tx.send(event);
                }
            }
        })?;
        watcher.watch(&root, RecursiveMode::Recursive)?;
        // The loop ends when the watcher (and with it the sender) is dropped.
        std::thread::spawn({
            let root = root.clone();
            move || {
                let mut pending = BTreeSet::new();
                let mut first: Option<Instant> = None;
                loop {
                    let wait = match first {
                        Some(t) => DEBOUNCE.min(MAX_BATCH.saturating_sub(t.elapsed())),
                        None => Duration::from_secs(3600),
                    };
                    match rx.recv_timeout(wait) {
                        Ok(event) => {
                            first.get_or_insert_with(Instant::now);
                            pending.extend(
                                event
                                    .paths
                                    .into_iter()
                                    .map(|p| rebase(&root, &canon, p))
                                    .filter(|p| relevant(&root, &state_dir, p)),
                            );
                            if first.is_some_and(|t| t.elapsed() < MAX_BATCH) {
                                continue;
                            }
                        }
                        Err(RecvTimeoutError::Timeout) if first.is_none() => continue,
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    first = None;
                    if !pending.is_empty() {
                        on_change(VaultEvent {
                            paths: std::mem::take(&mut pending).into_iter().collect(),
                        });
                    }
                }
            }
        });
        Ok(Watcher { _watcher: watcher })
    }
}

/// Whether an event can mean a file's content, name or presence changed.
/// Opening, reading and closing a file without writing it cannot.
fn changes_content(kind: &EventKind) -> bool {
    match kind {
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) => false,
        _ => true,
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

/// Keep files and directories inside the vault, but never the root itself,
/// hidden entries, repository checkouts or anything under `.graphite/`, except the sync
/// documents other devices publish under `.graphite/sync/`, which the app
/// wants to know about.
fn relevant(root: &Path, state_dir: &Path, path: &Path) -> bool {
    if path == root {
        return false;
    }
    if path.starts_with(state_dir) {
        return is_sync_document(state_dir, path);
    }
    let rel = path.strip_prefix(root).unwrap_or(path);
    !rel.components()
        .any(|c| c.as_os_str() == STATE_DIR || c.as_os_str().to_string_lossy().starts_with('.'))
        && !crate::vault::is_source_checkout(&crate::vault::to_rel_string(rel))
}

/// `.graphite/sync/<device>/<something>` (not the `sync` folder itself).
pub fn is_sync_document(state_dir: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(state_dir) else {
        return false;
    };
    let mut parts = rel.components();
    parts.next().is_some_and(|c| c.as_os_str() == crate::sync::SYNC_DIR) && parts.count() >= 2
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
    fn reading_a_file_is_not_a_change() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "hello").unwrap();
        let (tx, rx) = mpsc::channel();
        let _w = Watcher::new(dir.path(), move |ev| {
            let _ = tx.send(ev);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        for _ in 0..3 {
            std::fs::read_to_string(dir.path().join("note.md")).unwrap();
        }
        match rx.recv_timeout(Duration::from_millis(1500)) {
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            other => panic!("reading reported a change: {other:?}"),
        }
        assert!(!changes_content(&EventKind::Access(AccessKind::Open(
            notify::event::AccessMode::Any
        ))));
        assert!(!changes_content(&EventKind::Access(AccessKind::Close(
            AccessMode::Read
        ))));
        assert!(changes_content(&EventKind::Access(AccessKind::Close(
            AccessMode::Write
        ))));
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
    fn keeps_files_and_folders_but_not_root_state_dir_or_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let state = root.join(STATE_DIR);
        std::fs::create_dir_all(&state).unwrap();
        std::fs::create_dir_all(root.join("notes")).unwrap();
        assert!(!relevant(root, &state, root));
        assert!(!relevant(root, &state, &state));
        assert!(!relevant(root, &state, &state.join("index.sqlite")));
        assert!(!relevant(root, &state, &state.join("sync")));
        assert!(!relevant(root, &state, &state.join("devices/abc/index.sqlite")));
        assert!(relevant(root, &state, &state.join("sync/abc/01X.automerge")));
        assert!(relevant(root, &state, &state.join("sync/abc/seen.json")));
        assert!(relevant(root, &state, &root.join("image.png")));
        assert!(!relevant(root, &state, &root.join(".hidden/x.png")));
        assert!(relevant(root, &state, &root.join("notes")));
        assert!(relevant(root, &state, &root.join("notes/a.md")));
        assert!(relevant(root, &state, &root.join("gone.md")));
    }
}
