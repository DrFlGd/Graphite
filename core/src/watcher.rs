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
    /// Changes under `.graphite/` and non-Markdown files are filtered out.
    pub fn new<F>(root: &Path, mut on_change: F) -> Result<Watcher>
    where
        F: FnMut(VaultEvent) + Send + 'static,
    {
        let state_dir = root.join(STATE_DIR);
        let mut debouncer = new_debouncer(Duration::from_millis(300), move |res: DebounceEventResult| {
            if let Ok(events) = res {
                let mut paths: Vec<PathBuf> = events
                    .into_iter()
                    .map(|e| e.path)
                    .filter(|p| !p.starts_with(&state_dir))
                    .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md") || p.is_dir())
                    .collect();
                paths.sort();
                paths.dedup();
                if !paths.is_empty() {
                    on_change(VaultEvent { paths });
                }
            }
        })?;
        debouncer.watcher().watch(root, RecursiveMode::Recursive)?;
        Ok(Watcher { _debouncer: debouncer })
    }
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
}
