//! The identity of this machine. Sync keeps one folder of documents and
//! one index per device inside the vault, so each device needs a stable id
//! that survives restarts and is unique across the machines sharing a
//! vault.
//!
//! The id lives outside the vault (in the OS config directory) because the
//! vault is the thing being shared; two machines must never read the same
//! id file. `GRAPHITE_DEVICE_ID` overrides it, which tests and the CLI use.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Device {
    /// 32 lowercase hex characters (a 16-byte random id).
    pub id: String,
    /// Human-readable name shown next to the id (the host name by default).
    pub name: String,
}

impl Device {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Device {
        Device {
            id: id.into(),
            name: name.into(),
        }
    }
}

/// The device Graphite is running on. Reads (or mints and stores) the id,
/// honouring `GRAPHITE_DEVICE_ID` and `GRAPHITE_DEVICE_NAME`.
pub fn local_device() -> Device {
    let id = std::env::var("GRAPHITE_DEVICE_ID")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| is_valid_id(s))
        .unwrap_or_else(stored_or_new_id);
    let name = std::env::var("GRAPHITE_DEVICE_NAME")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(host_name);
    Device { id, name }
}

/// Ids are hex so they double as Automerge actor ids and as folder names.
pub fn is_valid_id(s: &str) -> bool {
    (8..=64).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

pub fn new_id() -> String {
    automerge::ActorId::random().to_hex_string()
}

/// Where Graphite keeps per-device state outside any vault: the device id
/// and relay settings. `GRAPHITE_CONFIG_DIR` overrides the OS config
/// directory (the Android app sets it to its private storage, which has
/// no home directory).
pub fn config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("GRAPHITE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d));
    }
    dirs::config_dir().or_else(dirs::home_dir).map(|d| d.join("graphite"))
}

fn id_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("device-id"))
}

fn stored_or_new_id() -> String {
    let Some(file) = id_file() else {
        return new_id();
    };
    if let Ok(text) = fs::read_to_string(&file) {
        let s = text.trim().to_string();
        if is_valid_id(&s) {
            return s;
        }
    }
    let id = new_id();
    if let Some(dir) = file.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Err(e) = fs::write(&file, format!("{id}\n")) {
        tracing::warn!("could not store device id in {}: {e}", file.display());
    }
    id
}

fn host_name() -> String {
    for key in ["COMPUTERNAME", "HOSTNAME", "HOST"] {
        if let Ok(v) = std::env::var(key) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return v;
            }
        }
    }
    if let Ok(text) = fs::read_to_string("/etc/hostname") {
        let v = text.trim().to_string();
        if !v.is_empty() {
            return v;
        }
    }
    "This device".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_hex_and_unique() {
        let a = new_id();
        let b = new_id();
        assert!(is_valid_id(&a));
        assert_ne!(a, b);
        assert!(!is_valid_id("not hex"));
        assert!(!is_valid_id("ABCDEF01"));
        assert!(!is_valid_id("abc"));
    }
}
