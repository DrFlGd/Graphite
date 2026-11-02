//! Drawings (E11): sketches and diagrams made in Graphite's Excalidraw
//! editor. Each is a plain `.excalidraw` file (the JSON that
//! excalidraw.com saves and opens), by default under
//! `attachments/drawings/`, and a note shows it with an ordinary embed:
//! `![Sketch](attachments/drawings/Sketch.excalidraw)`.
//!
//! Unlike other attachments, Graphite writes these files: the drawing
//! editor saves into them.

use std::fs;
use std::path::Path;

use chrono::Local;
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::vault::Vault;

pub const DRAWINGS_DIR: &str = "attachments/drawings";
pub const DRAWING_EXT: &str = "excalidraw";

/// The file a new, empty drawing starts as.
pub fn empty_drawing() -> String {
    let doc = json!({
        "type": "excalidraw",
        "version": 2,
        "source": "graphite",
        "elements": [],
        "appState": { "viewBackgroundColor": "#ffffff", "gridSize": null },
        "files": {}
    });
    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

/// Whether `rel` names a drawing file.
pub fn is_drawing(rel: &str) -> bool {
    Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(DRAWING_EXT))
}

/// The words in a drawing (its text elements), for search.
pub fn drawing_text(json: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(json).ok()?;
    let texts: Vec<&str> = doc
        .get("elements")?
        .as_array()?
        .iter()
        .filter(|e| e.get("isDeleted").and_then(Value::as_bool) != Some(true))
        .filter_map(|e| e.get("text").and_then(Value::as_str))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    Some(texts.join("\n"))
}

fn check(json: &str) -> Result<()> {
    let doc: Value =
        serde_json::from_str(json).map_err(|e| Error::Io(std::io::Error::other(format!("not a drawing: {e}"))))?;
    if doc.get("type").and_then(Value::as_str) != Some("excalidraw")
        || !doc.get("elements").is_some_and(Value::is_array)
    {
        return Err(Error::Io(std::io::Error::other("not an Excalidraw drawing")));
    }
    Ok(())
}

impl Vault {
    /// Start a new, empty drawing and return its vault-relative path. The
    /// name defaults to `Drawing <date> <time>`; an existing file is never
    /// overwritten.
    pub fn new_drawing(&mut self, name: Option<&str>) -> Result<String> {
        let stem = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(|n| n.trim_end_matches(".excalidraw").to_string())
            .unwrap_or_else(|| format!("Drawing {}", Local::now().format("%Y-%m-%d %H%M")));
        self.write_attachment(
            &format!("{stem}.{DRAWING_EXT}"),
            empty_drawing().as_bytes(),
            Some(DRAWINGS_DIR),
        )
    }

    /// A drawing's JSON.
    pub fn read_drawing(&self, rel: &str) -> Result<String> {
        if !is_drawing(rel) {
            return Err(Error::Io(std::io::Error::other(format!("{rel} is not a drawing"))));
        }
        Ok(fs::read_to_string(self.safe_join(rel)?)?)
    }

    /// Save a drawing from the editor. The JSON must be an Excalidraw
    /// scene; the file is replaced in one step so a reader never sees half
    /// of it.
    pub fn write_drawing(&mut self, rel: &str, json: &str) -> Result<()> {
        if !is_drawing(rel) {
            return Err(Error::Io(std::io::Error::other(format!("{rel} is not a drawing"))));
        }
        check(json)?;
        let abs = self.safe_join(rel)?;
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = abs.with_extension("excalidraw.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &abs)?;
        self.reindex_path(Path::new(rel))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawings_are_created_saved_indexed_and_searchable() {
        let dir = tempfile::tempdir().unwrap();
        let mut vault = Vault::open(dir.path()).unwrap();
        let rel = vault.new_drawing(Some("Wiring")).unwrap();
        assert_eq!(rel, "attachments/drawings/Wiring.excalidraw");
        assert_eq!(
            vault.new_drawing(Some("Wiring")).unwrap(),
            "attachments/drawings/Wiring-2.excalidraw"
        );
        assert!(vault
            .new_drawing(None)
            .unwrap()
            .starts_with("attachments/drawings/Drawing "));
        assert!(vault.read_drawing(&rel).unwrap().contains("\"excalidraw\""));

        let scene = r#"{"type":"excalidraw","version":2,"elements":[
            {"type":"rectangle","id":"a"},
            {"type":"text","id":"b","text":"Battery pack"},
            {"type":"text","id":"c","text":"old label","isDeleted":true}
        ],"appState":{},"files":{}}"#;
        vault.write_drawing(&rel, scene).unwrap();
        assert_eq!(drawing_text(scene).unwrap(), "Battery pack");
        assert!(vault.write_drawing(&rel, "{\"type\":\"other\"}").is_err());
        assert!(vault.write_drawing("notes/x.md", scene).is_err());
        assert!(vault.read_drawing("../outside.excalidraw").is_err());

        while !vault.derive_assets(10).unwrap().is_empty() {}
        assert_eq!(vault.asset_text(&rel).unwrap().as_deref(), Some("Battery pack"));
    }
}
