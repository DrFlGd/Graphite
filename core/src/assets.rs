//! Assets: every non-Markdown file in the vault is a first-class node with
//! metadata, a preview and links, while the file itself is never modified
//! (ADR-002). What Graphite derives from a file (a thumbnail, extracted
//! text, a content hash) lives under `.graphite/derived/` and is a cache.
//!
//! Indexing an asset is split in two so opening a large vault stays fast:
//! `Vault::sync` records the cheap facts (size, kind, image dimensions,
//! model bounds) and marks the asset `pending`; `Vault::derive_assets` then
//! hashes the file, runs the optional external tools (ffprobe/ffmpeg for
//! media, poppler for PDFs) and extracts text for search.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::index::Backlink;
use crate::vault::{Vault, STATE_DIR};

/// Folder under `.graphite/` holding thumbnails and extracted text.
pub const DERIVED_DIR: &str = "derived";

/// Files larger than this are not read for text extraction.
const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;
/// Models larger than this skip bounding-box computation at index time.
const MAX_MODEL_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Image,
    Vector,
    Video,
    Audio,
    Pdf,
    Model,
    Code,
    Data,
    Document,
    Archive,
    Other,
}

impl AssetKind {
    pub fn from_ext(ext: &str) -> AssetKind {
        match ext.to_ascii_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "avif" | "heic" | "heif" | "tif" | "tiff" | "ico" => {
                AssetKind::Image
            }
            "svg" | "ai" | "eps" | "dxf" => AssetKind::Vector,
            "mp4" | "m4v" | "mov" | "webm" | "mkv" | "avi" | "mpg" | "mpeg" | "ogv" => AssetKind::Video,
            "mp3" | "m4a" | "wav" | "flac" | "ogg" | "oga" | "aac" | "opus" | "aiff" => AssetKind::Audio,
            "pdf" => AssetKind::Pdf,
            "gltf" | "glb" | "obj" | "stl" | "3mf" | "step" | "stp" | "iges" | "igs" | "fbx" | "ply" | "blend"
            | "usdz" | "f3d" | "scad" => AssetKind::Model,
            "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "c" | "h" | "cpp" | "hpp" | "cs" | "rb"
            | "php" | "swift" | "kt" | "sh" | "bash" | "zsh" | "ps1" | "sql" | "html" | "htm" | "css" | "scss"
            | "lua" | "r" | "m" | "scala" | "dart" | "ex" | "exs" | "hs" | "ml" | "toml" | "ini" | "cfg" | "conf"
            | "mk" | "cmake" | "gradle" | "dockerfile" | "vue" | "svelte" | "ino" => AssetKind::Code,
            "csv" | "tsv" | "json" | "jsonl" | "yaml" | "yml" | "xml" | "xlsx" | "xls" | "ods" | "parquet"
            | "sqlite" | "db" => AssetKind::Data,
            "txt" | "rtf" | "doc" | "docx" | "odt" | "pages" | "ppt" | "pptx" | "odp" | "key" | "epub" | "tex"
            | "org" | "rst" | "adoc" => AssetKind::Document,
            "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "dmg" | "iso" => AssetKind::Archive,
            _ => AssetKind::Other,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AssetKind::Image => "image",
            AssetKind::Vector => "vector",
            AssetKind::Video => "video",
            AssetKind::Audio => "audio",
            AssetKind::Pdf => "pdf",
            AssetKind::Model => "model",
            AssetKind::Code => "code",
            AssetKind::Data => "data",
            AssetKind::Document => "document",
            AssetKind::Archive => "archive",
            AssetKind::Other => "other",
        }
    }

    pub fn parse(s: &str) -> AssetKind {
        match s {
            "image" => AssetKind::Image,
            "vector" => AssetKind::Vector,
            "video" => AssetKind::Video,
            "audio" => AssetKind::Audio,
            "pdf" => AssetKind::Pdf,
            "model" => AssetKind::Model,
            "code" => AssetKind::Code,
            "data" => AssetKind::Data,
            "document" => AssetKind::Document,
            "archive" => AssetKind::Archive,
            _ => AssetKind::Other,
        }
    }

    pub fn all() -> &'static [AssetKind] {
        &[
            AssetKind::Image,
            AssetKind::Vector,
            AssetKind::Video,
            AssetKind::Audio,
            AssetKind::Pdf,
            AssetKind::Model,
            AssetKind::Code,
            AssetKind::Data,
            AssetKind::Document,
            AssetKind::Archive,
            AssetKind::Other,
        ]
    }
}

/// Extensions whose files the editor can show inline and that `![[name]]`
/// embeds refer to.
pub fn is_asset_ext(ext: &str) -> bool {
    !ext.is_empty() && !ext.eq_ignore_ascii_case("md") && AssetKind::from_ext(ext) != AssetKind::Other
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DerivedState {
    /// Hash, tool-based metadata and previews not computed yet.
    Pending,
    /// Everything that can be derived for this file has been.
    Done,
}

/// One file in the vault, as the index knows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    /// Vault-relative path with `/` separators; the asset's identity.
    pub path: String,
    pub name: String,
    /// Lower-case extension without the dot (may be empty).
    pub ext: String,
    pub kind: AssetKind,
    pub size: i64,
    pub modified: DateTime<Utc>,
    /// SHA-256 of the contents, hex. `None` until derived.
    pub hash: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// Seconds, for audio and video.
    pub duration: Option<f64>,
    pub pages: Option<i64>,
    /// Format-specific extras: `bounds` (model bounding box), `triangles`,
    /// `codec`, `rows`/`columns` (CSV), PDF `title`/`author`…
    pub meta: Map<String, Value>,
    /// Vault-relative path of extracted text under `.graphite/derived/`.
    pub text: Option<String>,
    /// Vault-relative path of a JPEG thumbnail under `.graphite/derived/`.
    pub thumbnail: Option<String>,
    pub derived: DerivedState,
}

/// A full-text hit in an asset's name or extracted text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetHit {
    pub path: String,
    pub name: String,
    pub kind: AssetKind,
    pub snippet: String,
}

/// Which optional external programs are on the PATH. Everything degrades
/// gracefully without them; the UI uses this to say what is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tools {
    pub ffmpeg: bool,
    pub ffprobe: bool,
    pub pdfinfo: bool,
    pub pdftotext: bool,
    pub pdftoppm: bool,
    pub git: bool,
    pub yt_dlp: bool,
}

/// Probe the PATH once per process.
pub fn tools() -> &'static Tools {
    static TOOLS: OnceLock<Tools> = OnceLock::new();
    TOOLS.get_or_init(|| Tools {
        ffmpeg: find_tool("ffmpeg").is_some(),
        ffprobe: find_tool("ffprobe").is_some(),
        pdfinfo: find_tool("pdfinfo").is_some(),
        pdftotext: find_tool("pdftotext").is_some(),
        pdftoppm: find_tool("pdftoppm").is_some(),
        git: find_tool("git").is_some(),
        yt_dlp: find_tool("yt-dlp").is_some(),
    })
}

/// Locate a program on the PATH (with `.exe`/`.cmd` on Windows).
pub fn find_tool(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: Vec<String> = if cfg!(windows) {
        vec![
            format!("{name}.exe"),
            format!("{name}.cmd"),
            format!("{name}.bat"),
            name.to_string(),
        ]
    } else {
        vec![name.to_string()]
    };
    for dir in std::env::split_paths(&path) {
        for n in &names {
            let p = dir.join(n);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Name and lower-case extension of a vault-relative path.
pub fn split_name(path: &str) -> (String, String) {
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    let ext = match name.rsplit_once('.') {
        Some((stem, e)) if !stem.is_empty() => e.to_ascii_lowercase(),
        _ => String::new(),
    };
    (name, ext)
}

/// Build the index-time view of a file: cheap facts only, no hashing and no
/// external tools. `modified` and `size` come from the caller's `stat`.
pub fn probe(abs: &Path, rel: &str, size: i64, modified: DateTime<Utc>) -> Asset {
    let (name, ext) = split_name(rel);
    let kind = AssetKind::from_ext(&ext);
    let mut asset = Asset {
        path: rel.to_string(),
        name,
        ext: ext.clone(),
        kind,
        size,
        modified,
        hash: None,
        width: None,
        height: None,
        duration: None,
        pages: None,
        meta: Map::new(),
        text: None,
        thumbnail: None,
        derived: DerivedState::Pending,
    };
    match kind {
        AssetKind::Image => {
            if let Ok(dim) = imagesize::size(abs) {
                asset.width = Some(dim.width as i64);
                asset.height = Some(dim.height as i64);
            }
        }
        AssetKind::Vector if ext == "svg" => {
            if let Ok(text) = read_prefix(abs, 64 * 1024) {
                let (w, h) = svg_size(&text);
                asset.width = w;
                asset.height = h;
            }
        }
        AssetKind::Model if (size as u64) <= MAX_MODEL_BYTES => {
            if let Some(m) = model_meta(abs, &ext) {
                asset.meta = m;
            }
        }
        AssetKind::Data if ext == "csv" || ext == "tsv" => {
            if let Ok(text) = read_prefix(abs, MAX_TEXT_BYTES as usize) {
                let sep = if ext == "tsv" { '\t' } else { ',' };
                let mut lines = text.lines().filter(|l| !l.trim().is_empty());
                if let Some(first) = lines.next() {
                    asset.meta.insert("columns".into(), json!(first.split(sep).count()));
                    if (size as u64) <= MAX_TEXT_BYTES {
                        asset.meta.insert("rows".into(), json!(lines.count()));
                    }
                }
            }
        }
        _ => {}
    }
    asset
}

fn read_prefix(abs: &Path, max: usize) -> std::io::Result<String> {
    let mut f = fs::File::open(abs)?;
    let mut buf = Vec::new();
    f.by_ref().take(max as u64).read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Width and height of an SVG from its root attributes or viewBox.
fn svg_size(text: &str) -> (Option<i64>, Option<i64>) {
    let start = match text.find("<svg") {
        Some(i) => i,
        None => return (None, None),
    };
    let end = text[start..].find('>').map(|e| start + e).unwrap_or(text.len());
    let tag = &text[start..end];
    let attr = |name: &str| -> Option<String> {
        let key = format!("{name}=\"");
        let i = tag.find(&key)?;
        let rest = &tag[i + key.len()..];
        let j = rest.find('"')?;
        Some(rest[..j].to_string())
    };
    let num = |s: String| -> Option<i64> {
        let digits: String = s.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        digits.parse::<f64>().ok().map(|f| f.round() as i64)
    };
    let w = attr("width").and_then(num);
    let h = attr("height").and_then(num);
    if w.is_some() && h.is_some() {
        return (w, h);
    }
    if let Some(vb) = attr("viewBox") {
        let parts: Vec<f64> = vb
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter_map(|p| p.parse().ok())
            .collect();
        if parts.len() == 4 {
            return (w.or(Some(parts[2].round() as i64)), h.or(Some(parts[3].round() as i64)));
        }
    }
    (w, h)
}

/// Bounding box (and triangle count where cheap) of STL, OBJ, PLY and glTF/GLB files.
fn model_meta(abs: &Path, ext: &str) -> Option<Map<String, Value>> {
    let data = fs::read(abs).ok()?;
    let mut meta = Map::new();
    let mut bounds = Bounds::new();
    match ext {
        "stl" => {
            let ascii = data.starts_with(b"solid") && !looks_binary_stl(&data);
            if ascii {
                let text = String::from_utf8_lossy(&data);
                let mut tris = 0u64;
                for line in text.lines() {
                    let line = line.trim();
                    if let Some(rest) = line.strip_prefix("vertex") {
                        let v: Vec<f64> = rest.split_whitespace().filter_map(|p| p.parse().ok()).collect();
                        if v.len() == 3 {
                            bounds.add(v[0], v[1], v[2]);
                        }
                    } else if line.starts_with("facet") {
                        tris += 1;
                    }
                }
                meta.insert("triangles".into(), json!(tris));
            } else {
                if data.len() < 84 {
                    return None;
                }
                let tris = u32::from_le_bytes([data[80], data[81], data[82], data[83]]) as usize;
                meta.insert("triangles".into(), json!(tris));
                let mut off = 84;
                for _ in 0..tris {
                    if off + 50 > data.len() {
                        break;
                    }
                    for v in 0..3 {
                        let b = off + 12 + v * 12;
                        let x = f32::from_le_bytes([data[b], data[b + 1], data[b + 2], data[b + 3]]) as f64;
                        let y = f32::from_le_bytes([data[b + 4], data[b + 5], data[b + 6], data[b + 7]]) as f64;
                        let z = f32::from_le_bytes([data[b + 8], data[b + 9], data[b + 10], data[b + 11]]) as f64;
                        bounds.add(x, y, z);
                    }
                    off += 50;
                }
            }
        }
        "obj" => {
            let text = String::from_utf8_lossy(&data);
            let mut faces = 0u64;
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("v ") {
                    let v: Vec<f64> = rest.split_whitespace().filter_map(|p| p.parse().ok()).collect();
                    if v.len() >= 3 {
                        bounds.add(v[0], v[1], v[2]);
                    }
                } else if line.starts_with("f ") {
                    faces += 1;
                }
            }
            meta.insert("faces".into(), json!(faces));
        }
        "ply" => {
            let text = String::from_utf8_lossy(&data[..data.len().min(4096)]);
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("element vertex ") {
                    if let Ok(n) = rest.trim().parse::<u64>() {
                        meta.insert("vertices".into(), json!(n));
                    }
                } else if let Some(rest) = line.strip_prefix("element face ") {
                    if let Ok(n) = rest.trim().parse::<u64>() {
                        meta.insert("faces".into(), json!(n));
                    }
                }
            }
        }
        "gltf" | "glb" => {
            let json_bytes: &[u8] = if ext == "glb" {
                // GLB: 12-byte header, then chunks; the first chunk is JSON.
                if data.len() < 20 || &data[0..4] != b"glTF" {
                    return None;
                }
                let len = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;
                if data.len() < 20 + len {
                    return None;
                }
                &data[20..20 + len]
            } else {
                &data
            };
            let doc: Value = serde_json::from_slice(json_bytes).ok()?;
            let count = |k: &str| doc.get(k).and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            meta.insert("meshes".into(), json!(count("meshes")));
            meta.insert("nodes".into(), json!(count("nodes")));
            meta.insert("materials".into(), json!(count("materials")));
            meta.insert("animations".into(), json!(count("animations")));
            if let Some(gen) = doc.pointer("/asset/generator").and_then(|v| v.as_str()) {
                meta.insert("generator".into(), json!(gen));
            }
            // POSITION accessors carry min/max per the spec: the union is the bounding box.
            if let (Some(meshes), Some(accessors)) = (
                doc.get("meshes").and_then(|v| v.as_array()),
                doc.get("accessors").and_then(|v| v.as_array()),
            ) {
                for mesh in meshes {
                    for prim in mesh.get("primitives").and_then(|v| v.as_array()).into_iter().flatten() {
                        let Some(idx) = prim.pointer("/attributes/POSITION").and_then(|v| v.as_u64()) else {
                            continue;
                        };
                        let Some(acc) = accessors.get(idx as usize) else {
                            continue;
                        };
                        let vec3 = |k: &str| -> Option<[f64; 3]> {
                            let a = acc.get(k)?.as_array()?;
                            Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?])
                        };
                        if let (Some(min), Some(max)) = (vec3("min"), vec3("max")) {
                            bounds.add(min[0], min[1], min[2]);
                            bounds.add(max[0], max[1], max[2]);
                        }
                    }
                }
            }
        }
        _ => return None,
    }
    if let Some(b) = bounds.finish() {
        meta.insert("bounds".into(), json!(b));
        meta.insert("dimensions".into(), json!([b[3] - b[0], b[4] - b[1], b[5] - b[2]]));
    }
    Some(meta)
}

/// A binary STL that happens to start with "solid": its triangle count
/// matches the file size while an ASCII file has "facet" in the header.
fn looks_binary_stl(data: &[u8]) -> bool {
    if data.len() < 84 {
        return false;
    }
    let tris = u32::from_le_bytes([data[80], data[81], data[82], data[83]]) as usize;
    data.len() == 84 + tris * 50
}

struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
    any: bool,
}

impl Bounds {
    fn new() -> Bounds {
        Bounds {
            min: [f64::MAX; 3],
            max: [f64::MIN; 3],
            any: false,
        }
    }
    fn add(&mut self, x: f64, y: f64, z: f64) {
        for (i, v) in [x, y, z].into_iter().enumerate() {
            self.min[i] = self.min[i].min(v);
            self.max[i] = self.max[i].max(v);
        }
        self.any = true;
    }
    fn finish(&self) -> Option<[f64; 6]> {
        self.any.then_some([
            self.min[0],
            self.min[1],
            self.min[2],
            self.max[0],
            self.max[1],
            self.max[2],
        ])
    }
}

fn sha256_file(abs: &Path) -> std::io::Result<String> {
    let mut f = fs::File::open(abs)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Parse `ffprobe -of json` output into duration, dimensions and codecs.
fn parse_ffprobe(out: &str, asset: &mut Asset) {
    let Ok(doc) = serde_json::from_str::<Value>(out) else {
        return;
    };
    if let Some(d) = doc
        .pointer("/format/duration")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
    {
        asset.duration = Some((d * 1000.0).round() / 1000.0);
    }
    if let Some(bits) = doc
        .pointer("/format/bit_rate")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u64>().ok())
    {
        asset.meta.insert("bitrate".into(), json!(bits));
    }
    let mut codecs = Vec::new();
    for s in doc.get("streams").and_then(|v| v.as_array()).into_iter().flatten() {
        let ty = s.get("codec_type").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(c) = s.get("codec_name").and_then(|v| v.as_str()) {
            codecs.push(format!("{ty}:{c}"));
        }
        if ty == "video" && asset.width.is_none() {
            asset.width = s.get("width").and_then(|v| v.as_i64());
            asset.height = s.get("height").and_then(|v| v.as_i64());
            if let Some(fr) = s.get("avg_frame_rate").and_then(|v| v.as_str()) {
                if let Some((n, d)) = fr.split_once('/') {
                    if let (Ok(n), Ok(d)) = (n.parse::<f64>(), d.parse::<f64>()) {
                        if d > 0.0 && n > 0.0 {
                            asset.meta.insert("fps".into(), json!((n / d * 100.0).round() / 100.0));
                        }
                    }
                }
            }
        }
        if ty == "audio" {
            if let Some(ch) = s.get("channels").and_then(|v| v.as_i64()) {
                asset.meta.insert("channels".into(), json!(ch));
            }
            if let Some(sr) = s
                .get("sample_rate")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
            {
                asset.meta.insert("sample_rate".into(), json!(sr));
            }
        }
    }
    if !codecs.is_empty() {
        asset.meta.insert("codecs".into(), json!(codecs));
    }
    if let Some(tags) = doc.pointer("/format/tags").and_then(|v| v.as_object()) {
        for key in ["title", "artist", "album", "creation_time"] {
            if let Some(v) = tags.get(key).and_then(|v| v.as_str()) {
                asset.meta.insert(key.into(), json!(v));
            }
        }
    }
}

/// Parse `pdfinfo` output: page count, title, author, page size.
fn parse_pdfinfo(out: &str, asset: &mut Asset) {
    for line in out.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim().trim_matches('\0').trim();
        match k.trim() {
            "Pages" => asset.pages = v.parse().ok(),
            "Title" if !v.is_empty() => {
                asset.meta.insert("title".into(), json!(v));
            }
            "Author" if !v.is_empty() => {
                asset.meta.insert("author".into(), json!(v));
            }
            "Page size" if !v.is_empty() => {
                asset.meta.insert("page_size".into(), json!(v));
            }
            _ => {}
        }
    }
}

impl Vault {
    /// Absolute path of the derived-files folder.
    pub fn derived_dir(&self) -> PathBuf {
        self.root().join(STATE_DIR).join(DERIVED_DIR)
    }

    /// Every indexed asset, optionally filtered by kind and a name/path
    /// substring, newest first.
    pub fn assets(&self, kind: Option<AssetKind>, filter: Option<&str>) -> Result<Vec<Asset>> {
        self.index.assets(kind, filter)
    }

    /// One asset by vault-relative path.
    pub fn asset(&self, path: &str) -> Result<Asset> {
        let rel = path.trim_start_matches("./").replace('\\', "/");
        self.index.asset(&rel)?.ok_or_else(|| Error::NoteNotFound(rel))
    }

    /// Notes that embed or link to this file.
    pub fn asset_references(&self, path: &str) -> Result<Vec<Backlink>> {
        self.index.asset_references(path)
    }

    /// Search file names and extracted text (PDF text, code, data files).
    pub fn search_assets(&self, query: &str, limit: usize) -> Result<Vec<AssetHit>> {
        self.index.search_assets(query, limit)
    }

    /// Counts per kind, for a gallery's filter bar.
    pub fn asset_counts(&self) -> Result<HashMap<AssetKind, usize>> {
        self.index.asset_counts()
    }

    /// Extracted text of an asset, if any was derived.
    pub fn asset_text(&self, path: &str) -> Result<Option<String>> {
        let asset = self.asset(path)?;
        match asset.text {
            Some(rel) => Ok(Some(fs::read_to_string(self.safe_join(&rel)?)?)),
            None => Ok(None),
        }
    }

    /// Derive hash, tool-based metadata, thumbnails and text for up to
    /// `limit` pending assets. Returns the assets that were updated; call
    /// until it returns an empty list.
    pub fn derive_assets(&mut self, limit: usize) -> Result<Vec<Asset>> {
        let pending = self.index.pending_assets(limit)?;
        let mut done = Vec::with_capacity(pending.len());
        for asset in pending {
            let abs = match self.safe_join(&asset.path) {
                Ok(p) if p.is_file() => p,
                _ => {
                    self.index.remove_asset(&asset.path)?;
                    continue;
                }
            };
            let derived = self.derive_one(asset, &abs);
            self.upsert_asset(&derived)?;
            done.push(derived);
        }
        Ok(done)
    }

    /// Re-derive one asset now (e.g. after an external edit), regardless of state.
    pub fn derive_asset(&mut self, path: &str) -> Result<Asset> {
        let asset = self.asset(path)?;
        let abs = self.safe_join(&asset.path)?;
        let derived = self.derive_one(asset, &abs);
        self.upsert_asset(&derived)?;
        Ok(derived)
    }

    fn derive_one(&self, mut asset: Asset, abs: &Path) -> Asset {
        let tools = tools();
        let dir = self.derived_dir();
        let _ = fs::create_dir_all(&dir);
        let hash = sha256_file(abs).ok();
        let key = hash
            .as_deref()
            .map(|h| h[..24].to_string())
            .unwrap_or_else(|| format!("{:x}", Sha256::digest(asset.path.as_bytes()))[..24].to_string());
        asset.hash = hash;
        let rel_derived = |name: &str| format!("{STATE_DIR}/{DERIVED_DIR}/{name}");
        let abs_str = abs.to_string_lossy().into_owned();
        let thumb_abs = dir.join(format!("{key}.jpg"));
        let text_abs = dir.join(format!("{key}.txt"));
        asset.thumbnail = None;
        asset.text = None;

        match asset.kind {
            AssetKind::Video | AssetKind::Audio => {
                if tools.ffprobe {
                    if let Some(out) = run(
                        "ffprobe",
                        &["-v", "error", "-show_entries", "format:stream", "-of", "json", &abs_str],
                    ) {
                        parse_ffprobe(&out, &mut asset);
                    }
                }
                if asset.kind == AssetKind::Video && tools.ffmpeg {
                    // A frame a little way in: the first frame is often black.
                    let at = asset.duration.map(|d| (d * 0.1).min(5.0)).unwrap_or(1.0).max(0.0);
                    let at = format!("{at:.2}");
                    if thumb_abs.exists() || make_thumb(&abs_str, &thumb_abs, Some(&at)) {
                        asset.thumbnail = Some(rel_derived(&format!("{key}.jpg")));
                    }
                }
            }
            AssetKind::Pdf => {
                if tools.pdfinfo {
                    if let Some(out) = run("pdfinfo", &[&abs_str]) {
                        parse_pdfinfo(&out, &mut asset);
                    }
                }
                if tools.pdftotext {
                    let text_str = text_abs.to_string_lossy().into_owned();
                    if text_abs.exists() || run("pdftotext", &["-layout", &abs_str, &text_str]).is_some() {
                        asset.text = Some(rel_derived(&format!("{key}.txt")));
                    }
                }
                if tools.pdftoppm {
                    let stem = dir.join(&key);
                    let stem_str = stem.to_string_lossy().into_owned();
                    if thumb_abs.exists()
                        || run(
                            "pdftoppm",
                            &[
                                "-jpeg",
                                "-r",
                                "40",
                                "-f",
                                "1",
                                "-l",
                                "1",
                                "-singlefile",
                                &abs_str,
                                &stem_str,
                            ],
                        )
                        .is_some()
                    {
                        asset.thumbnail = Some(rel_derived(&format!("{key}.jpg")));
                    }
                }
            }
            AssetKind::Image => {
                if tools.ffmpeg && asset.ext != "gif" && (thumb_abs.exists() || make_thumb(&abs_str, &thumb_abs, None))
                {
                    asset.thumbnail = Some(rel_derived(&format!("{key}.jpg")));
                }
            }
            AssetKind::Code | AssetKind::Data | AssetKind::Document => {
                let textual = matches!(asset.kind, AssetKind::Code)
                    || matches!(
                        asset.ext.as_str(),
                        "csv"
                            | "tsv"
                            | "json"
                            | "jsonl"
                            | "yaml"
                            | "yml"
                            | "xml"
                            | "txt"
                            | "tex"
                            | "org"
                            | "rst"
                            | "adoc"
                    );
                if textual && (asset.size as u64) <= MAX_TEXT_BYTES {
                    if let Ok(bytes) = fs::read(abs) {
                        if let Ok(s) = String::from_utf8(bytes) {
                            if fs::write(&text_abs, s).is_ok() {
                                asset.text = Some(rel_derived(&format!("{key}.txt")));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        asset.derived = DerivedState::Done;
        asset
    }

    /// Render one page of a PDF (1-based) to a JPEG under `.graphite/derived/`
    /// and return its vault-relative path. Needs `pdftoppm`; pages are
    /// rendered once and reused.
    pub fn asset_page(&self, path: &str, page: u32) -> Result<String> {
        let asset = self.asset(path)?;
        if asset.kind != AssetKind::Pdf {
            return Err(Error::NoteNotFound(format!("{path} is not a PDF")));
        }
        if !tools().pdftoppm {
            return Err(Error::Io(std::io::Error::other("pdftoppm (poppler) is not installed")));
        }
        let page = page.max(1);
        let key = asset
            .hash
            .as_deref()
            .map(|h| h[..24].to_string())
            .unwrap_or_else(|| format!("{:x}", Sha256::digest(asset.path.as_bytes()))[..24].to_string());
        let dir = self.derived_dir();
        fs::create_dir_all(&dir)?;
        let name = format!("{key}-p{page}");
        let out = dir.join(format!("{name}.jpg"));
        if !out.is_file() {
            let abs = self.safe_join(&asset.path)?;
            let p = page.to_string();
            let stem = dir.join(&name);
            run(
                "pdftoppm",
                &[
                    "-jpeg",
                    "-r",
                    "110",
                    "-f",
                    &p,
                    "-l",
                    &p,
                    "-singlefile",
                    &abs.to_string_lossy(),
                    &stem.to_string_lossy(),
                ],
            )
            .ok_or_else(|| Error::Io(std::io::Error::other(format!("could not render page {page}"))))?;
        }
        Ok(format!("{STATE_DIR}/{DERIVED_DIR}/{name}.jpg"))
    }

    /// Absolute path of a file-system location inside the vault, for
    /// "open in default app" and "reveal in file manager".
    pub fn asset_abs_path(&self, path: &str) -> Result<PathBuf> {
        let abs = self.safe_join(path)?;
        if !abs.exists() {
            return Err(Error::NoteNotFound(path.to_string()));
        }
        Ok(abs)
    }
}

/// Write a JPEG thumbnail with ffmpeg, at most 480px wide.
fn make_thumb(src: &str, dest: &Path, at: Option<&str>) -> bool {
    let dest_str = dest.to_string_lossy().into_owned();
    let mut args: Vec<&str> = vec!["-v", "error", "-y"];
    if let Some(at) = at {
        args.extend(["-ss", at]);
    }
    args.extend([
        "-i",
        src,
        "-frames:v",
        "1",
        "-vf",
        "scale='min(480,iw)':-2",
        "-q:v",
        "4",
        "-update",
        "1",
        &dest_str,
    ]);
    let ok = run("ffmpeg", &args).is_some();
    if !ok && at.is_some() {
        // Shorter than the seek point: take the first frame instead.
        return make_thumb(src, dest, None);
    }
    ok && dest.is_file()
}

/// Human-readable size.
pub fn format_size(bytes: i64) -> String {
    let b = bytes as f64;
    if b < 1024.0 {
        format!("{bytes} B")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", b / 1024.0 / 1024.0)
    } else {
        format!("{:.2} GB", b / 1024.0 / 1024.0 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_indexes_assets_references_and_search() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("attachments/Garden")).unwrap();
        fs::create_dir_all(dir.path().join("notes")).unwrap();
        // A 2x3 PNG header is enough for imagesize.
        let png: Vec<u8> = {
            let mut v = vec![
                0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
            ];
            v.extend_from_slice(&2u32.to_be_bytes());
            v.extend_from_slice(&3u32.to_be_bytes());
            v.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
            v
        };
        fs::write(dir.path().join("attachments/Garden/plan.png"), &png).unwrap();
        fs::write(dir.path().join("attachments/notes.txt"), "Motor torque table\n").unwrap();
        fs::write(dir.path().join("attachments/.DS_Store"), "x").unwrap();
        fs::write(
            dir.path().join("notes/Garden.md"),
            "# Garden\n![plan](attachments/Garden/plan.png)\nSee [torque](attachments/notes.txt) and ![[plan.png]]",
        )
        .unwrap();
        let mut v = Vault::open(dir.path()).unwrap();
        let all = v.assets(None, None).unwrap();
        let paths: Vec<&str> = all.iter().map(|a| a.path.as_str()).collect();
        assert!(paths.contains(&"attachments/Garden/plan.png"), "{paths:?}");
        assert!(paths.contains(&"attachments/notes.txt"));
        assert!(!paths.iter().any(|p| p.contains(".DS_Store")));
        let plan = v.asset("attachments/Garden/plan.png").unwrap();
        assert_eq!(
            (plan.kind, plan.width, plan.height),
            (AssetKind::Image, Some(2), Some(3))
        );
        assert_eq!(plan.derived, DerivedState::Pending);
        assert_eq!(v.info().unwrap().asset_count, 2);

        // Both the Markdown link and the wiki embed count as references.
        let refs = v.asset_references("attachments/Garden/plan.png").unwrap();
        assert_eq!(refs.len(), 2, "{refs:?}");
        assert_eq!(refs[0].from.title, "Garden");
        assert_eq!(v.asset_references("attachments/notes.txt").unwrap().len(), 1);
        // File links never show up as note backlinks or graph nodes.
        let g = v.graph().unwrap();
        assert!(g.nodes.iter().all(|n| !n.missing), "{:?}", g.nodes);

        // Deriving hashes the file and extracts text for search.
        let done = v.derive_assets(10).unwrap();
        assert_eq!(done.len(), 2);
        assert!(v.derive_assets(10).unwrap().is_empty());
        let txt = v.asset("attachments/notes.txt").unwrap();
        assert_eq!(txt.derived, DerivedState::Done);
        assert_eq!(txt.hash.as_deref().map(|h| h.len()), Some(64));
        assert!(txt.text.as_deref().unwrap().starts_with(".graphite/derived/"));
        assert_eq!(
            v.asset_text("attachments/notes.txt").unwrap().as_deref(),
            Some("Motor torque table\n")
        );
        let hits = v.search_assets("torque", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "attachments/notes.txt");
        assert_eq!(v.search_assets("plan", 10).unwrap().len(), 1);
        assert_eq!(v.asset_counts().unwrap()[&AssetKind::Image], 1);
        assert_eq!(v.assets(Some(AssetKind::Document), None).unwrap().len(), 1);
        assert_eq!(v.assets(None, Some("garden")).unwrap().len(), 1);

        // An external edit re-indexes and goes back to pending; deletion drops the row.
        fs::write(
            dir.path().join("attachments/notes.txt"),
            "Motor torque table, revised\n",
        )
        .unwrap();
        v.reindex_path(Path::new("attachments/notes.txt")).unwrap();
        assert_eq!(v.asset("attachments/notes.txt").unwrap().derived, DerivedState::Pending);
        fs::remove_file(dir.path().join("attachments/notes.txt")).unwrap();
        v.reindex_path(Path::new("attachments/notes.txt")).unwrap();
        assert!(v.asset("attachments/notes.txt").is_err());
        assert!(v.asset_abs_path("attachments/Garden/plan.png").unwrap().is_file());
        assert!(v.asset_abs_path("../etc/passwd").is_err());
        // Rebuild wipes derived files and re-finds everything.
        v.rebuild().unwrap();
        assert_eq!(v.assets(None, None).unwrap().len(), 1);
    }

    #[test]
    fn kinds_follow_extensions() {
        assert_eq!(AssetKind::from_ext("PNG"), AssetKind::Image);
        assert_eq!(AssetKind::from_ext("glb"), AssetKind::Model);
        assert_eq!(AssetKind::from_ext("xyz"), AssetKind::Other);
        assert!(is_asset_ext("pdf"));
        assert!(!is_asset_ext("md"));
        assert!(!is_asset_ext(""));
        assert_eq!(split_name("attachments/a b/c.tar.GZ"), ("c.tar.GZ".into(), "gz".into()));
        assert_eq!(split_name(".hidden"), (".hidden".into(), "".into()));
        assert_eq!(format_size(1536), "1.5 KB");
    }

    #[test]
    fn svg_and_csv_probe() {
        let dir = tempfile::tempdir().unwrap();
        let svg = dir.path().join("a.svg");
        fs::write(
            &svg,
            "<?xml version=\"1.0\"?><svg xmlns=\"x\" viewBox=\"0 0 120 80\"><rect/></svg>",
        )
        .unwrap();
        let a = probe(&svg, "a.svg", 10, Utc::now());
        assert_eq!((a.kind, a.width, a.height), (AssetKind::Vector, Some(120), Some(80)));
        let csv = dir.path().join("t.csv");
        fs::write(&csv, "a,b,c\n1,2,3\n4,5,6\n").unwrap();
        let c = probe(&csv, "t.csv", 18, Utc::now());
        assert_eq!(c.meta.get("columns"), Some(&json!(3)));
        assert_eq!(c.meta.get("rows"), Some(&json!(2)));
    }

    #[test]
    fn stl_bounds_binary_and_ascii() {
        let dir = tempfile::tempdir().unwrap();
        // One binary triangle spanning (0,0,0)-(1,2,3).
        let mut data = vec![0u8; 80];
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&[0u8; 12]); // normal
        for v in [[0f32, 0.0, 0.0], [1.0, 0.0, 3.0], [0.0, 2.0, 0.0]] {
            for c in v {
                data.extend_from_slice(&c.to_le_bytes());
            }
        }
        data.extend_from_slice(&[0u8; 2]);
        let bin = dir.path().join("b.stl");
        fs::write(&bin, &data).unwrap();
        let a = probe(&bin, "b.stl", data.len() as i64, Utc::now());
        assert_eq!(a.meta.get("triangles"), Some(&json!(1)));
        assert_eq!(a.meta.get("bounds"), Some(&json!([0.0, 0.0, 0.0, 1.0, 2.0, 3.0])));
        let ascii = dir.path().join("a.stl");
        fs::write(
            &ascii,
            "solid t\nfacet normal 0 0 1\nouter loop\nvertex -1 0 0\nvertex 1 0 0\nvertex 0 1 5\nendloop\nendfacet\nendsolid",
        )
        .unwrap();
        let a = probe(&ascii, "a.stl", 10, Utc::now());
        assert_eq!(a.meta.get("dimensions"), Some(&json!([2.0, 1.0, 5.0])));
    }

    #[test]
    fn glb_bounds_from_accessors() {
        let dir = tempfile::tempdir().unwrap();
        let json = r#"{"asset":{"generator":"test"},"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"accessors":[{"min":[-1,-2,-3],"max":[1,2,3]}],"nodes":[{}]}"#;
        let mut glb = b"glTF".to_vec();
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&((20 + json.len()) as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(json.as_bytes());
        let p = dir.path().join("m.glb");
        fs::write(&p, &glb).unwrap();
        let a = probe(&p, "m.glb", glb.len() as i64, Utc::now());
        assert_eq!(a.meta.get("meshes"), Some(&json!(1)));
        assert_eq!(a.meta.get("generator"), Some(&json!("test")));
        assert_eq!(a.meta.get("dimensions"), Some(&json!([2.0, 4.0, 6.0])));
    }

    #[test]
    fn parses_tool_output() {
        let mut a = probe(Path::new("/nonexistent"), "v.mp4", 0, Utc::now());
        parse_ffprobe(
            r#"{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"avg_frame_rate":"30000/1001"},{"codec_type":"audio","codec_name":"aac","channels":2,"sample_rate":"48000"}],"format":{"duration":"12.345678","bit_rate":"1000","tags":{"title":"Clip"}}}"#,
            &mut a,
        );
        assert_eq!(a.duration, Some(12.346));
        assert_eq!((a.width, a.height), (Some(1920), Some(1080)));
        assert_eq!(a.meta.get("fps"), Some(&json!(29.97)));
        assert_eq!(a.meta.get("title"), Some(&json!("Clip")));
        let mut p = probe(Path::new("/nonexistent"), "d.pdf", 0, Utc::now());
        parse_pdfinfo(
            "Title:          Paper\nAuthor:         Me\nPages:          12\nPage size:      595 x 842 pts (A4)\n",
            &mut p,
        );
        assert_eq!(p.pages, Some(12));
        assert_eq!(p.meta.get("author"), Some(&json!("Me")));
    }
}
