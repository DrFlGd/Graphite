//! Graphite core.
//!
//! The vault on disk is the source of truth (see `docs/spec/vault-format.md`
//! and ADR-002). Everything in here is either a reader/writer of those files
//! or a rebuildable index over them.

pub mod error;
pub mod index;
pub mod links;
pub mod note;
pub mod vault;
pub mod watcher;

pub use error::{Error, Result};
pub use index::{Backlink, SearchHit};
pub use links::{Link, LinkKind};
pub use note::{Note, NoteSummary};
pub use vault::{Vault, VaultInfo};
pub use watcher::{VaultEvent, Watcher};
