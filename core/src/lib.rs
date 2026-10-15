//! Graphite core.
//!
//! The vault on disk is the source of truth (see `docs/spec/vault-format.md`
//! and ADR-002). Everything in here is either a reader/writer of those files
//! or a rebuildable index over them.

pub mod assets;
pub mod attachments;
pub mod capture;
pub mod error;
pub mod graph;
pub mod index;
pub mod links;
pub mod note;
pub mod project;
pub mod query;
pub mod sources;
pub mod tasks;
pub mod templates;
pub mod vault;
pub mod watcher;

pub use assets::{tools, Asset, AssetHit, AssetKind, DerivedState, Tools};
pub use capture::{capture_title, is_url, DAILY_DIR, INBOX_DIR};
pub use error::{Error, Result};
pub use graph::{Graph, GraphEdge, GraphNode};
pub use index::{Backlink, ItemFilter, ItemRow, SearchHit};
pub use links::{Link, LinkKind};
pub use note::{Note, NoteSummary};
pub use project::{LinkedNote, ProjectHub};
pub use query::{Domain, Group, NoteRow, Query, QueryResult};
pub use sources::{classify_url, ImportOptions, ImportReport, SourceKind, SourceRow, UpdateCheck};
pub use tasks::{Item, ItemKind, ItemStatus, Progress};
pub use templates::Template;
pub use vault::{Vault, VaultInfo};
pub use watcher::{VaultEvent, Watcher};
