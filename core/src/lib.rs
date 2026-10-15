//! Graphite core.
//!
//! The vault on disk is the source of truth (see `docs/spec/vault-format.md`
//! and ADR-002). Everything in here is either a reader/writer of those files
//! or a rebuildable index over them.

pub mod attachments;
pub mod capture;
pub mod error;
pub mod graph;
pub mod index;
pub mod links;
pub mod note;
pub mod tasks;
pub mod vault;
pub mod watcher;

pub use capture::{capture_title, is_url, DAILY_DIR, INBOX_DIR};
pub use error::{Error, Result};
pub use graph::{Graph, GraphEdge, GraphNode};
pub use index::{Backlink, ItemFilter, ItemRow, SearchHit};
pub use links::{Link, LinkKind};
pub use note::{Note, NoteSummary};
pub use tasks::{Item, ItemKind, ItemStatus, Progress};
pub use vault::{Vault, VaultInfo};
pub use watcher::{VaultEvent, Watcher};
