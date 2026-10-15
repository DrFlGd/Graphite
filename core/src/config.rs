//! Per-vault settings on this device: `.graphite/config.json`. Small and
//! plain so a user can edit it by hand; anything missing takes its default.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::vault::{Vault, STATE_DIR};

pub const CONFIG_FILE: &str = "config.json";
/// Files larger than this are referenced where they are instead of copied
/// into the vault, and stay on the device they are on instead of going
/// through a relay (M4, E9).
pub const DEFAULT_LARGE_FILE_MB: u64 = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct VaultConfig {
    pub large_file_mb: u64,
}

impl Default for VaultConfig {
    fn default() -> Self {
        VaultConfig {
            large_file_mb: DEFAULT_LARGE_FILE_MB,
        }
    }
}

impl Vault {
    fn config_path(&self) -> PathBuf {
        self.root().join(STATE_DIR).join(CONFIG_FILE)
    }

    /// The settings file, or defaults when it is missing or unreadable.
    pub fn config(&self) -> VaultConfig {
        fs::read_to_string(self.config_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save_config(&self, config: &VaultConfig) -> Result<()> {
        let text = serde_json::to_string_pretty(config).map_err(std::io::Error::other)?;
        fs::write(self.config_path(), text + "\n")?;
        Ok(())
    }

    /// The large-file threshold in bytes.
    pub fn large_file_bytes(&self) -> u64 {
        self.config().large_file_mb.saturating_mul(1024 * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_apply_and_settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        assert_eq!(v.config().large_file_mb, DEFAULT_LARGE_FILE_MB);
        v.save_config(&VaultConfig { large_file_mb: 5 }).unwrap();
        assert_eq!(v.config().large_file_mb, 5);
        assert_eq!(v.large_file_bytes(), 5 * 1024 * 1024);
        // Unknown keys and missing keys are tolerated.
        fs::write(dir.path().join(".graphite/config.json"), "{\"other\": 1}").unwrap();
        assert_eq!(v.config(), VaultConfig::default());
    }
}
