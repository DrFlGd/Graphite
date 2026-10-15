use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a vault: {0}")]
    NotAVault(PathBuf),
    #[error("note not found: {0}")]
    NoteNotFound(String),
    #[error("a note already exists at {0}")]
    NoteExists(PathBuf),
    #[error("invalid front-matter in {path}: {reason}")]
    FrontMatter { path: PathBuf, reason: String },
    #[error("nothing to capture")]
    EmptyCapture,
    #[error("line {line} of note {note} is not a task or gap")]
    NotAnItem { note: String, line: usize },
    #[error("path escapes the vault: {0}")]
    PathEscapesVault(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
    #[error(transparent)]
    Watch(#[from] notify::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
