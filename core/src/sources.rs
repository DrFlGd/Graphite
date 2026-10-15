//! Importers: a URL becomes content the vault keeps.
//!
//! Every import creates a `type: source` note (the **source record**) that
//! holds the URL, the importer that handled it, when it was fetched, a
//! content hash and links to what was produced under
//! `attachments/sources/<name>/`. "Check for updates" compares the remote
//! against the recorded hash and writes the outcome into the note; a
//! re-import refreshes the files and the note body.
//!
//! Importers:
//! - **GitHub / git**: shallow clone, README rendered into the note, commit recorded.
//! - **Web page**: original HTML kept, readable text as Markdown, images
//!   downloaded, screenshot when a Chromium-family browser is on the PATH.
//! - **Video** (YouTube, Vimeo and everything else yt-dlp knows): the video
//!   file, thumbnail, description and chapter list. Needs `yt-dlp`.
//! - **PDF and direct files**: downloaded into the vault as assets.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::assets::{find_tool, is_asset_ext, tools};
use crate::attachments::safe_attachment_name;
use crate::error::{Error, Result};
use crate::note::{Note, NoteSummary};
use crate::vault::{safe_file_stem, Vault};

pub const SOURCES_DIR: &str = "attachments/sources";
pub const SOURCE_NOTES_DIR: &str = "notes/Sources";
pub const SOURCE_TYPE: &str = "source";

/// Largest page, PDF or file the importers download (video goes through yt-dlp).
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;
const MAX_PAGE_IMAGES: usize = 60;
const USER_AGENT: &str = "Graphite/0.3 (+https://github.com/DrFlGd/Graphite)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Github,
    Git,
    Video,
    Page,
    Pdf,
    File,
}

impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceKind::Github => "github",
            SourceKind::Git => "git",
            SourceKind::Video => "video",
            SourceKind::Page => "page",
            SourceKind::Pdf => "pdf",
            SourceKind::File => "file",
        }
    }

    pub fn parse(s: &str) -> Option<SourceKind> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "github" => SourceKind::Github,
            "git" | "repo" => SourceKind::Git,
            "video" | "youtube" => SourceKind::Video,
            "page" | "web" | "article" => SourceKind::Page,
            "pdf" => SourceKind::Pdf,
            "file" => SourceKind::File,
            _ => return None,
        })
    }
}

const VIDEO_HOSTS: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "vimeo.com",
    "twitch.tv",
    "dailymotion.com",
    "ted.com",
    "rumble.com",
    "odysee.com",
    "bilibili.com",
    "tiktok.com",
];

/// Host of a URL, lower-case and without a leading `www.`.
pub fn url_host(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    Some(host.trim_start_matches("www.").to_ascii_lowercase())
}

/// Path part of a URL (no query or fragment).
fn url_path(url: &str) -> &str {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let path = rest.find('/').map(|i| &rest[i..]).unwrap_or("/");
    path.split(['?', '#']).next().unwrap_or(path)
}

/// Pick the importer for a URL (F2). `kind` overrides the guess.
pub fn classify_url(url: &str) -> SourceKind {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("git@") || lower.starts_with("git://") || lower.starts_with("ssh://git@") {
        return SourceKind::Git;
    }
    let host = url_host(url).unwrap_or_default();
    let path = url_path(url);
    if host == "github.com" || host == "gitlab.com" || host == "codeberg.org" || host.ends_with(".github.com") {
        let segs: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
        if segs.len() >= 2 && !matches!(segs[0], "settings" | "marketplace" | "features" | "topics" | "search") {
            return SourceKind::Github;
        }
        return SourceKind::Page;
    }
    if lower.ends_with(".git") {
        return SourceKind::Git;
    }
    if VIDEO_HOSTS
        .iter()
        .any(|h| host == *h || host.ends_with(&format!(".{h}")))
    {
        return SourceKind::Video;
    }
    if host == "arxiv.org" && (path.starts_with("/abs/") || path.starts_with("/pdf/")) {
        return SourceKind::Pdf;
    }
    let ext = path
        .rsplit('/')
        .next()
        .unwrap_or("")
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("pdf") => SourceKind::Pdf,
        Some(e) if is_asset_ext(e) && !matches!(e, "html" | "htm" | "php") => SourceKind::File,
        _ => SourceKind::Page,
    }
}

/// Options for `Vault::import_url`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportOptions {
    /// Importer to use instead of the one guessed from the URL.
    pub kind: Option<SourceKind>,
    /// Project title to link the source to (`project:: [[Title]]`).
    pub project: Option<String>,
    /// Note title override.
    pub title: Option<String>,
}

/// What an import produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportReport {
    pub note: Note,
    pub kind: SourceKind,
    /// Vault-relative folder holding the imported files.
    pub folder: String,
    /// Vault-relative paths of files written.
    pub files: Vec<String>,
    pub summary: String,
}

/// Outcome of a "check for updates".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheck {
    pub note_id: String,
    pub title: String,
    pub kind: SourceKind,
    pub changed: bool,
    pub summary: String,
    pub checked: DateTime<Utc>,
}

/// A source note as the UI lists it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRow {
    #[serde(flatten)]
    pub note: NoteSummary,
    pub url: String,
    pub kind: SourceKind,
    pub fetched: Option<String>,
    pub last_checked: Option<String>,
    pub update_available: bool,
    pub update_summary: Option<String>,
    pub folder: Option<String>,
    pub project: Option<String>,
}

fn prop_str(note: &Note, key: &str) -> Option<String> {
    note.properties.get(key).and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

fn http_err(e: ureq::Error) -> Error {
    Error::Io(std::io::Error::other(format!("download failed: {e}")))
}

fn other(msg: impl Into<String>) -> Error {
    Error::Io(std::io::Error::other(msg.into()))
}

fn agent() -> ureq::Agent {
    // The platform verifier trusts the OS certificate store, so corporate proxies
    // and SSL_CERT_FILE work as they do for the browser.
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .user_agent(USER_AGENT)
        .timeout_global(Some(Duration::from_secs(60)))
        .max_redirects(10)
        .http_status_as_error(true)
        .build()
        .into()
}

/// GET a URL as text (pages, info JSON), capped at 20 MB.
fn fetch_text(url: &str) -> Result<(String, Option<String>)> {
    let mut resp = agent().get(url).call().map_err(http_err)?;
    let ctype = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let text = resp
        .body_mut()
        .with_config()
        .limit(20 * 1024 * 1024)
        .lossy_utf8(true)
        .read_to_string()
        .map_err(http_err)?;
    Ok((text, ctype))
}

/// Stream a URL into a file, returning its SHA-256 and the final file name
/// hint from headers.
fn fetch_to_file(url: &str, dest: &Path) -> Result<(String, Option<String>)> {
    let mut resp = agent().get(url).call().map_err(http_err)?;
    let disposition = resp
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.split(';')
                .map(str::trim)
                .find_map(|p| p.strip_prefix("filename=").map(|f| f.trim_matches('"').to_string()))
        });
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut reader = resp.body_mut().with_config().limit(MAX_DOWNLOAD).reader();
    let mut file = fs::File::create(dest)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| other(format!("download failed: {e}")))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
    }
    Ok((format!("{:x}", hasher.finalize()), disposition))
}

fn sha256_str(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}

/// A path as an external program wants it: on Windows, canonical paths carry a
/// `\\?\` prefix that git refuses as a work tree.
fn plain_path(p: &Path) -> String {
    let s = p.to_string_lossy().into_owned();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s
    }
}

fn run(cmd: &str, args: &[&str], cwd: Option<&Path>) -> std::result::Result<String, String> {
    let mut c = Command::new(cmd);
    c.args(args).stdin(Stdio::null());
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    let out = c.output().map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!(
            "{cmd} {}: {}",
            args.first().unwrap_or(&""),
            err.trim().lines().last().unwrap_or("failed")
        ))
    }
}

// ----- git -----

/// Normalise a GitHub-style URL to something `git clone` accepts: strip
/// `/tree/...`, `/blob/...`, trailing slashes and fragments.
pub fn clone_url(url: &str) -> String {
    let url = url.trim().split(['#', '?']).next().unwrap_or(url);
    if url.starts_with("git@") || url.ends_with(".git") || url.starts_with("file://") {
        return url.to_string();
    }
    let host = url_host(url).unwrap_or_default();
    if host == "github.com" || host == "gitlab.com" || host == "codeberg.org" {
        let scheme = url.split("://").next().unwrap_or("https");
        let segs: Vec<&str> = url_path(url)
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        if segs.len() >= 2 {
            return format!("{scheme}://{host}/{}/{}.git", segs[0], segs[1].trim_end_matches(".git"));
        }
    }
    url.trim_end_matches('/').to_string()
}

/// `owner/repo` for a hosted repository URL, else the last path segment.
fn repo_name(url: &str) -> String {
    let clean = clone_url(url);
    let path = if let Some(rest) = clean.strip_prefix("git@") {
        rest.split_once(':').map(|(_, p)| p).unwrap_or(rest).to_string()
    } else {
        url_path(&clean).to_string()
    };
    let segs: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    let name = match segs.len() {
        0 => "repository".to_string(),
        1 => segs[0].to_string(),
        n => format!("{}/{}", segs[n - 2], segs[n - 1]),
    };
    name.trim_end_matches(".git").to_string()
}

fn readme_in(dir: &Path) -> Option<(String, String)> {
    for name in [
        "README.md",
        "readme.md",
        "Readme.md",
        "README.markdown",
        "README",
        "README.txt",
        "README.rst",
    ] {
        let p = dir.join(name);
        if p.is_file() {
            if let Ok(text) = fs::read_to_string(&p) {
                return Some((name.to_string(), text));
            }
        }
    }
    None
}

/// Demote headings so a README's `#` title does not fight the note's own.
fn demote_headings(md: &str) -> String {
    let mut in_fence = false;
    md.lines()
        .map(|l| {
            let t = l.trim_start();
            if t.starts_with("```") || t.starts_with("~~~") {
                in_fence = !in_fence;
            }
            if !in_fence && l.starts_with('#') && l.len() < 200 {
                format!("#{l}")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn remote_head(url: &str) -> std::result::Result<String, String> {
    let out = run("git", &["ls-remote", &clone_url(url), "HEAD"], None)?;
    out.split_whitespace()
        .next()
        .map(|s| s.to_string())
        .ok_or_else(|| "no HEAD on remote".to_string())
}

// ----- web pages -----

/// Everything we keep from a fetched page.
struct PageExtract {
    title: String,
    markdown: String,
    /// Absolute image URLs in document order.
    images: Vec<String>,
    description: Option<String>,
}

/// Resolve `href` against `base` (enough for the cases pages use).
fn resolve_url(base: &str, href: &str) -> String {
    let href = href.trim();
    if href.contains("://") || href.starts_with("data:") {
        return href.to_string();
    }
    let scheme = base.split("://").next().unwrap_or("https");
    if let Some(rest) = href.strip_prefix("//") {
        return format!("{scheme}://{rest}");
    }
    let host = url_host(base).unwrap_or_default();
    let origin_host = base
        .split("://")
        .nth(1)
        .and_then(|r| r.split('/').next())
        .map(|h| h.to_string())
        .unwrap_or(host);
    let origin = format!("{scheme}://{origin_host}");
    if href.starts_with('/') {
        return format!("{origin}{href}");
    }
    let path = url_path(base);
    let dir = match path.rfind('/') {
        Some(i) => &path[..=i],
        None => "/",
    };
    format!("{origin}{dir}{href}")
}

fn extract_page(url: &str, html: &str) -> PageExtract {
    use scraper::{Html, Selector};
    let doc = Html::parse_document(html);
    let sel = |s: &str| Selector::parse(s).unwrap();
    let meta = |name: &str| -> Option<String> {
        doc.select(&sel(&format!("meta[property=\"{name}\"], meta[name=\"{name}\"]")))
            .next()
            .and_then(|m| m.attr("content"))
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
    };
    let title = meta("og:title")
        .or_else(|| {
            doc.select(&sel("title"))
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
        })
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| url_host(url).unwrap_or_else(|| "Page".into()));
    let description = meta("description").or_else(|| meta("og:description"));

    // The readable part: an <article>, else <main>, else the body.
    let root_html = [
        "article",
        "main",
        "[role=main]",
        "#content",
        ".post",
        ".entry-content",
        "body",
    ]
    .iter()
    .find_map(|s| {
        doc.select(&sel(s))
            .max_by_key(|el| el.text().map(|t| t.len()).sum::<usize>())
            .and_then(|el| {
                let len: usize = el.text().map(|t| t.trim().len()).sum();
                (len > 200 || *s == "body").then(|| el.inner_html())
            })
    })
    .unwrap_or_else(|| doc.root_element().inner_html());

    let converter = htmd::HtmlToMarkdown::builder()
        .skip_tags(vec![
            "script", "style", "nav", "header", "footer", "aside", "form", "iframe", "noscript", "svg", "button",
            "input", "select", "textarea", "video", "audio", "canvas", "template",
        ])
        .build();
    let mut markdown = converter.convert(&root_html).unwrap_or_default();
    // Absolute links and images, and collect the images.
    let mut images = Vec::new();
    markdown = rewrite_links(&markdown, url, &mut images);
    markdown = collapse_blank_lines(&markdown);
    PageExtract {
        title,
        markdown,
        images,
        description,
    }
}

/// Make `[text](rel)` and `![alt](rel)` absolute; collect image URLs.
fn rewrite_links(md: &str, base: &str, images: &mut Vec<String>) -> String {
    let re = regex::Regex::new(r#"(!?)\[([^\]]*)\]\(([^)\s]+)((?:\s+"[^"]*")?)\)"#).unwrap();
    re.replace_all(md, |cap: &regex::Captures| {
        let bang = &cap[1];
        let text = &cap[2];
        let target = &cap[3];
        let title = &cap[4];
        if target.starts_with('#') || target.starts_with("mailto:") || target.starts_with("javascript:") {
            return if bang.is_empty() {
                format!("[{text}]({target}{title})")
            } else {
                String::new()
            };
        }
        let abs = resolve_url(base, target);
        if bang == "!" {
            if abs.starts_with("data:") {
                return String::new();
            }
            if !images.contains(&abs) {
                images.push(abs.clone());
            }
        }
        format!("{bang}[{text}]({abs}{title})")
    })
    .into_owned()
}

fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank = 0;
    for line in s.lines() {
        if is_noise_line(line) {
            continue;
        }
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out.trim().to_string()
}

/// Lines the converter leaves behind for icon-only links and empty list items.
fn is_noise_line(line: &str) -> bool {
    let t = line.trim();
    if matches!(t, "-" | "*" | "+") {
        return true;
    }
    let mut rest = t;
    let mut saw_link = false;
    while let Some(after) = rest.strip_prefix("[](").or_else(|| rest.strip_prefix("[ ](")) {
        match after.find(')') {
            Some(i) => {
                rest = after[i + 1..].trim_start();
                saw_link = true;
            }
            None => return false,
        }
    }
    saw_link && rest.is_empty()
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// A Chromium-based browser for screenshots: `GRAPHITE_BROWSER`, then the PATH,
/// then the usual install locations.
pub(crate) fn browser_binary() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("GRAPHITE_BROWSER") {
        // A path to use, or "none" (or empty) to never take screenshots.
        let p = PathBuf::from(&v);
        return if p.is_file() { Some(p) } else { None };
    }
    [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
        "chrome",
        "brave",
        "msedge",
        "microsoft-edge",
    ]
    .iter()
    .find_map(|b| find_tool(b))
    .or_else(|| {
        [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
            "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
            "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
    })
}

fn screenshot(url: &str, dest: &Path) -> bool {
    let Some(bin) = browser_binary() else { return false };
    let shot = format!("--screenshot={}", plain_path(dest));
    let status = Command::new(bin)
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-sandbox",
            "--hide-scrollbars",
            "--window-size=1280,1800",
            "--virtual-time-budget=8000",
            "--timeout=25000",
            &shot,
            url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    matches!(status, Ok(s) if s.success()) && dest.is_file()
}

fn image_name(url: &str, i: usize) -> String {
    let last = url_path(url).rsplit('/').next().unwrap_or("").to_string();
    let last = safe_attachment_name(&last);
    let has_ext = last
        .rsplit_once('.')
        .map(|(_, e)| e.len() <= 5 && !e.is_empty())
        .unwrap_or(false);
    if last.is_empty() || last == "file" || !has_ext {
        format!("image-{}.img", i + 1)
    } else {
        format!("{:02}-{last}", i + 1)
    }
}

// ----- video -----

#[derive(Default)]
struct VideoInfo {
    id: String,
    title: String,
    uploader: Option<String>,
    upload_date: Option<String>,
    duration: Option<f64>,
    description: Option<String>,
    chapters: Vec<(f64, String)>,
    webpage_url: Option<String>,
}

fn parse_video_info(v: &Value) -> VideoInfo {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string());
    VideoInfo {
        id: s("id").unwrap_or_default(),
        title: s("title").unwrap_or_else(|| "Video".into()),
        uploader: s("uploader").or_else(|| s("channel")),
        upload_date: s("upload_date").map(|d| {
            if d.len() == 8 {
                format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..])
            } else {
                d
            }
        }),
        duration: v.get("duration").and_then(|d| d.as_f64()),
        description: s("description"),
        chapters: v
            .get("chapters")
            .and_then(|c| c.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|c| Some((c.get("start_time")?.as_f64()?, c.get("title")?.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default(),
        webpage_url: s("webpage_url"),
    }
}

fn hms(secs: f64) -> String {
    let s = secs.round() as u64;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

fn md_escape_title(t: &str) -> String {
    t.replace(['[', ']'], " ").trim().to_string()
}

// ----- the vault side -----

impl Vault {
    /// Import a URL (F2–F6, F9): fetch it, write files under
    /// `attachments/sources/<name>/`, and create the source note.
    pub fn import_url(&mut self, url: &str, opts: &ImportOptions) -> Result<ImportReport> {
        let url = url.trim();
        if url.is_empty() || !(url.contains("://") || url.starts_with("git@")) {
            return Err(other(format!("not a URL: {url}")));
        }
        let kind = opts.kind.unwrap_or_else(|| classify_url(url));
        let mut note = self.new_source_note(url, kind, opts)?;
        match self.run_import(url, kind, &mut note, opts) {
            Ok(report) => Ok(report),
            Err(e) => {
                // Leave no half-made record behind: the note and the folder made for it.
                let _ = self.delete(&note.id);
                if let Some(folder) = prop_str(&note, "folder") {
                    if let Ok(abs) = self.safe_join(&folder) {
                        let _ = fs::remove_dir_all(abs);
                    }
                }
                Err(e)
            }
        }
    }

    /// Re-fetch a source and refresh its files and note body (F7).
    pub fn reimport_source(&mut self, id: &str) -> Result<ImportReport> {
        let mut note = self.read(id)?;
        if note.note_type != SOURCE_TYPE {
            return Err(other(format!("{} is not a source note", note.title)));
        }
        let url = prop_str(&note, "url").ok_or_else(|| other("source note has no url"))?;
        let kind = prop_str(&note, "source_type")
            .and_then(|k| SourceKind::parse(&k))
            .unwrap_or_else(|| classify_url(&url));
        let opts = ImportOptions {
            kind: Some(kind),
            project: None,
            title: Some(note.title.clone()),
        };
        self.run_import(&url, kind, &mut note, &opts)
    }

    /// Every `type: source` note with its record fields.
    pub fn sources(&self) -> Result<Vec<SourceRow>> {
        let mut out = Vec::new();
        for s in self.list()? {
            if s.note_type != SOURCE_TYPE {
                continue;
            }
            let note = match self.read(&s.id) {
                Ok(n) => n,
                Err(_) => continue,
            };
            let url = prop_str(&note, "url").unwrap_or_default();
            out.push(SourceRow {
                kind: prop_str(&note, "source_type")
                    .and_then(|k| SourceKind::parse(&k))
                    .unwrap_or_else(|| classify_url(&url)),
                url,
                fetched: prop_str(&note, "fetched"),
                last_checked: prop_str(&note, "last_checked"),
                update_available: note
                    .properties
                    .get("update_available")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                update_summary: prop_str(&note, "update_summary"),
                folder: prop_str(&note, "folder"),
                project: prop_str(&note, "project"),
                note: s,
            });
        }
        out.sort_by(|a, b| b.fetched.cmp(&a.fetched));
        Ok(out)
    }

    /// Compare one source with its remote and record the outcome in the note.
    pub fn check_source(&mut self, id: &str) -> Result<UpdateCheck> {
        let mut note = self.read(id)?;
        if note.note_type != SOURCE_TYPE {
            return Err(other(format!("{} is not a source note", note.title)));
        }
        let url = prop_str(&note, "url").ok_or_else(|| other("source note has no url"))?;
        let kind = prop_str(&note, "source_type")
            .and_then(|k| SourceKind::parse(&k))
            .unwrap_or_else(|| classify_url(&url));
        let hash = prop_str(&note, "hash").unwrap_or_default();
        let (changed, summary) = match kind {
            SourceKind::Github | SourceKind::Git => {
                let head = remote_head(&url).map_err(other)?;
                if head == hash {
                    (false, format!("Up to date at {}", &head[..7.min(head.len())]))
                } else {
                    (
                        true,
                        format!(
                            "New commits: {} (have {})",
                            &head[..7.min(head.len())],
                            &hash[..7.min(hash.len())]
                        ),
                    )
                }
            }
            SourceKind::Page => {
                let (html, _) = fetch_text(&url)?;
                let page = extract_page(&url, &html);
                let new_hash = sha256_str(&page.markdown);
                if new_hash == hash {
                    (false, "Page text unchanged".into())
                } else {
                    let old_words = prop_str(&note, "words")
                        .and_then(|w| w.parse::<i64>().ok())
                        .unwrap_or(0);
                    let new_words = word_count(&page.markdown) as i64;
                    let delta = new_words - old_words;
                    (
                        true,
                        format!(
                            "Page text changed ({} words, {}{})",
                            new_words,
                            if delta >= 0 { "+" } else { "" },
                            delta
                        ),
                    )
                }
            }
            SourceKind::Video => {
                if !tools().yt_dlp {
                    return Err(other("yt-dlp is not installed"));
                }
                let out = run(
                    "yt-dlp",
                    &["--dump-single-json", "--no-playlist", "--no-warnings", &url],
                    None,
                )
                .map_err(other)?;
                let info: Value = serde_json::from_str(&out).map_err(|e| other(format!("yt-dlp output: {e}")))?;
                let v = parse_video_info(&info);
                let file_ok = prop_str(&note, "video")
                    .and_then(|p| self.safe_join(&p).ok())
                    .map(|p| p.is_file())
                    .unwrap_or(false);
                if !file_ok {
                    (true, "Video file is missing from the vault".into())
                } else if v.id != hash && !hash.is_empty() {
                    (true, format!("Video id changed to {}", v.id))
                } else if Some(v.title.as_str()) != prop_str(&note, "video_title").as_deref() {
                    (true, format!("Title changed: {}", v.title))
                } else {
                    (false, "Video unchanged".into())
                }
            }
            SourceKind::Pdf | SourceKind::File => {
                let tmp = self.derived_dir().join(format!("check-{}", sha256_str(&url)));
                let (new_hash, _) = fetch_to_file(&resolve_pdf_url(&url), &tmp)?;
                let _ = fs::remove_file(&tmp);
                if new_hash == hash {
                    (false, "File unchanged".into())
                } else {
                    (true, "File changed on the server".into())
                }
            }
        };
        let now = Utc::now();
        note.properties.insert("last_checked".into(), json!(now.to_rfc3339()));
        note.properties.insert("update_available".into(), json!(changed));
        note.properties.insert("update_summary".into(), json!(summary));
        let note = self.write(&note)?;
        Ok(UpdateCheck {
            note_id: note.id,
            title: note.title,
            kind,
            changed,
            summary,
            checked: now,
        })
    }

    /// Check every source not checked within `max_age` (all of them when
    /// `None`). Errors are reported per source, not raised.
    pub fn check_all_sources(&mut self, max_age: Option<Duration>) -> Result<Vec<UpdateCheck>> {
        let mut out = Vec::new();
        let now = Utc::now();
        for row in self.sources()? {
            if let (Some(age), Some(last)) = (
                max_age,
                row.last_checked
                    .as_deref()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok()),
            ) {
                if now
                    .signed_duration_since(last.with_timezone(&Utc))
                    .to_std()
                    .map(|d| d < age)
                    .unwrap_or(true)
                {
                    continue;
                }
            }
            match self.check_source(&row.note.id) {
                Ok(c) => out.push(c),
                Err(e) => out.push(UpdateCheck {
                    note_id: row.note.id,
                    title: row.note.title,
                    kind: row.kind,
                    changed: false,
                    summary: format!("Check failed: {e}"),
                    checked: now,
                }),
            }
        }
        Ok(out)
    }

    fn new_source_note(&mut self, url: &str, kind: SourceKind, opts: &ImportOptions) -> Result<Note> {
        let title = opts.title.clone().unwrap_or_else(|| match kind {
            SourceKind::Github | SourceKind::Git => repo_name(url),
            _ => {
                let host = url_host(url).unwrap_or_default();
                let last = url_path(url)
                    .trim_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_string();
                if last.is_empty() {
                    host
                } else {
                    format!("{host} {last}")
                }
            }
        });
        let title = unique_title(self, &title);
        let mut note = self.create(&title, Some(SOURCE_TYPE), Some(SOURCE_NOTES_DIR))?;
        note.properties.insert("url".into(), json!(url));
        note.properties.insert("source_type".into(), json!(kind.as_str()));
        if let Some(p) = &opts.project {
            note.properties.insert("project".into(), json!(format!("[[{p}]]")));
        }
        Ok(note)
    }

    /// When an import picked up a better title (the page's `<title>`, the PDF's
    /// metadata), move the note file so its name matches, if that name is free.
    fn rename_to_title(&mut self, note: Note) -> Result<Note> {
        let (dir, file) = note.path.rsplit_once('/').unwrap_or(("", note.path.as_str()));
        let want = format!("{}.md", safe_file_stem(&note.title));
        if file == want || want == ".md" {
            return Ok(note);
        }
        let new_rel = if dir.is_empty() { want } else { format!("{dir}/{want}") };
        let to = self.safe_join(&new_rel)?;
        if to.exists() {
            return Ok(note);
        }
        fs::rename(self.safe_join(&note.path)?, &to)?;
        self.index_remove_path(&note.path)?;
        let id = self.index_file(&new_rel)?;
        self.read(&id)
    }

    /// Folder for a source's files, created on first use and recorded in the note.
    fn source_folder(&self, note: &mut Note) -> Result<(String, PathBuf)> {
        if let Some(rel) = prop_str(note, "folder") {
            let abs = self.safe_join(&rel)?;
            fs::create_dir_all(&abs)?;
            return Ok((rel, abs));
        }
        let stem = safe_file_stem(&note.title).replace('/', "-");
        let stem = if stem.is_empty() { "source".to_string() } else { stem };
        let mut rel = format!("{SOURCES_DIR}/{stem}");
        let mut n = 1;
        while self.safe_join(&rel)?.exists() {
            n += 1;
            rel = format!("{SOURCES_DIR}/{stem}-{n}");
        }
        let abs = self.safe_join(&rel)?;
        fs::create_dir_all(&abs)?;
        note.properties.insert("folder".into(), json!(rel));
        Ok((rel, abs))
    }

    fn run_import(
        &mut self,
        url: &str,
        kind: SourceKind,
        note: &mut Note,
        opts: &ImportOptions,
    ) -> Result<ImportReport> {
        let (folder, dir) = self.source_folder(note)?;
        let mut files = Vec::new();
        let project_line = opts
            .project
            .clone()
            .or_else(|| prop_str(note, "project").map(|p| p.trim_matches(['[', ']']).to_string()))
            .map(|p| format!("project:: [[{p}]]\n"))
            .unwrap_or_default();
        let summary = match kind {
            SourceKind::Github | SourceKind::Git => {
                if !tools().git {
                    return Err(other("git is not installed"));
                }
                let repo = dir.join("repo");
                let clone = clone_url(url);
                if repo.join(".git").is_dir() {
                    run("git", &["fetch", "--depth", "1", "origin", "HEAD"], Some(&repo)).map_err(other)?;
                    run("git", &["reset", "--hard", "FETCH_HEAD"], Some(&repo)).map_err(other)?;
                } else {
                    let _ = fs::remove_dir_all(&repo);
                    run(
                        "git",
                        &["clone", "--depth", "1", "--quiet", &clone, &plain_path(&repo)],
                        None,
                    )
                    .map_err(other)?;
                }
                let commit = run("git", &["rev-parse", "HEAD"], Some(&repo))
                    .map_err(other)?
                    .trim()
                    .to_string();
                let old = prop_str(note, "hash").unwrap_or_default();
                let log = run("git", &["log", "-1", "--format=%cI%n%s"], Some(&repo)).unwrap_or_default();
                let mut log_lines = log.lines();
                let committed = log_lines.next().unwrap_or("").to_string();
                let subject = log_lines.next().unwrap_or("").to_string();
                let readme = readme_in(&repo);
                let readme_rel = readme.as_ref().map(|(n, _)| format!("{folder}/repo/{n}"));
                let mut body = format!("# {}\n\nsource:: {url}\n{project_line}\n", note.title);
                let short = &commit[..7.min(commit.len())];
                let subject_part = if subject.is_empty() {
                    String::new()
                } else {
                    format!(" (\"{subject}\")")
                };
                let date_part = if committed.is_empty() {
                    String::new()
                } else {
                    format!(", committed {}", &committed[..10.min(committed.len())])
                };
                let repo_rel = format!("{folder}/repo");
                body.push_str(&format!(
                    "Cloned at commit `{short}`{subject_part}{date_part}. Files are in [{repo_rel}]({}).\n",
                    encode_md_path(&repo_rel)
                ));
                if let Some((name, text)) = &readme {
                    body.push_str(&format!("\n## {name}\n\n{}\n", demote_headings(text.trim())));
                }
                note.body = body;
                note.properties.insert("hash".into(), json!(commit));
                note.properties.insert("commit".into(), json!(commit));
                if let Some(r) = readme_rel {
                    files.push(r);
                }
                if old.is_empty() {
                    format!("Cloned {} at {}", note.title, &commit[..7.min(commit.len())])
                } else if old == commit {
                    "Already at the latest commit".to_string()
                } else {
                    format!(
                        "Updated {} → {}",
                        &old[..7.min(old.len())],
                        &commit[..7.min(commit.len())]
                    )
                }
            }
            SourceKind::Page => {
                let (html, _) = fetch_text(url)?;
                let page = extract_page(url, &html);
                let page_html = dir.join("page.html");
                fs::write(&page_html, &html)?;
                files.push(format!("{folder}/page.html"));
                // Images: download what the readable text references, rewrite to local paths.
                let mut markdown = page.markdown.clone();
                let img_dir = dir.join("images");
                for (i, img) in page.images.iter().take(MAX_PAGE_IMAGES).enumerate() {
                    let name = image_name(img, i);
                    let dest = img_dir.join(&name);
                    if dest.is_file() || fetch_to_file(img, &dest).is_ok() {
                        let rel = format!("{folder}/images/{name}");
                        files.push(rel.clone());
                        markdown = markdown.replace(&format!("]({img})"), &format!("]({})", encode_md_path(&rel)));
                    }
                }
                let shot = dir.join("screenshot.png");
                let has_shot = shot.is_file() || screenshot(url, &shot);
                if has_shot {
                    files.push(format!("{folder}/screenshot.png"));
                }
                if opts.title.is_none() && note.title != page.title && !page.title.is_empty() {
                    note.title = unique_title(self, &md_escape_title(&page.title));
                }
                let words = word_count(&markdown);
                let mut body = format!("# {}\n\nsource:: {url}\n{project_line}", note.title);
                if let Some(d) = &page.description {
                    body.push_str(&format!("\n> {}\n", d.replace('\n', " ")));
                }
                body.push_str(&format!(
                    "\nArchived copy: [page.html]({}){}. {} word{}.\n\n---\n\n{}\n",
                    encode_md_path(&format!("{folder}/page.html")),
                    if has_shot {
                        format!(
                            ", [screenshot]({})",
                            encode_md_path(&format!("{folder}/screenshot.png"))
                        )
                    } else {
                        String::new()
                    },
                    words,
                    if words == 1 { "" } else { "s" },
                    markdown
                ));
                note.body = body;
                note.properties.insert("hash".into(), json!(sha256_str(&page.markdown)));
                note.properties.insert("words".into(), json!(words));
                format!(
                    "Saved \"{}\" ({} words, {} images)",
                    note.title,
                    words,
                    page.images.len().min(MAX_PAGE_IMAGES)
                )
            }
            SourceKind::Video => {
                if !tools().yt_dlp {
                    return Err(other(
                        "yt-dlp is not installed; install it (pip install yt-dlp) to import videos",
                    ));
                }
                let template = dir.join("%(title).120B [%(id)s].%(ext)s");
                let template = template.to_string_lossy().into_owned();
                let mut args = vec![
                    "--no-playlist",
                    "--no-warnings",
                    "--no-overwrites",
                    "-f",
                    "bv*[height<=1080][ext=mp4]+ba[ext=m4a]/b[ext=mp4]/bv*+ba/b",
                    "--write-thumbnail",
                    "--write-info-json",
                    "--restrict-filenames",
                    "-o",
                    &template,
                ];
                if tools().ffmpeg {
                    args.extend(["--merge-output-format", "mp4", "--convert-thumbnails", "jpg"]);
                }
                args.push(url);
                run("yt-dlp", &args, None).map_err(other)?;
                // Find what it wrote.
                let mut info_path = None;
                let mut video_path = None;
                let mut thumb_path = None;
                for entry in fs::read_dir(&dir)?.flatten() {
                    let p = entry.path();
                    let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    if name.ends_with(".info.json") {
                        info_path = Some(p);
                    } else if name.ends_with(".jpg") || name.ends_with(".webp") || name.ends_with(".png") {
                        thumb_path = Some(p);
                    } else if matches!(
                        p.extension().and_then(|e| e.to_str()),
                        Some("mp4" | "mkv" | "webm" | "m4a" | "mp3")
                    ) {
                        video_path = Some(p);
                    }
                }
                let info: Value = info_path
                    .as_ref()
                    .and_then(|p| fs::read_to_string(p).ok())
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(Value::Null);
                let v = parse_video_info(&info);
                let video_rel = video_path
                    .as_ref()
                    .map(|p| format!("{folder}/{}", p.file_name().unwrap().to_string_lossy()));
                let thumb_rel = thumb_path
                    .as_ref()
                    .map(|p| format!("{folder}/{}", p.file_name().unwrap().to_string_lossy()));
                if opts.title.is_none() && !v.title.is_empty() {
                    note.title = unique_title(self, &md_escape_title(&v.title));
                }
                let mut body = format!(
                    "# {}\n\nsource:: {}\n{project_line}",
                    note.title,
                    v.webpage_url.as_deref().unwrap_or(url)
                );
                let mut facts = Vec::new();
                if let Some(u) = &v.uploader {
                    facts.push(u.clone());
                }
                if let Some(d) = &v.upload_date {
                    facts.push(d.clone());
                }
                if let Some(d) = v.duration {
                    facts.push(hms(d));
                }
                if !facts.is_empty() {
                    body.push_str(&format!("\n{}\n", facts.join(" · ")));
                }
                if let Some(t) = &thumb_rel {
                    body.push_str(&format!("\n![thumbnail]({})\n", encode_md_path(t)));
                }
                if let Some(vp) = &video_rel {
                    body.push_str(&format!("\n![video]({})\n", encode_md_path(vp)));
                }
                if !v.chapters.is_empty() {
                    body.push_str("\n## Chapters\n\n");
                    for (t, name) in &v.chapters {
                        body.push_str(&format!("- {} {}\n", hms(*t), name));
                    }
                }
                if let Some(d) = &v.description {
                    if !d.trim().is_empty() {
                        body.push_str(&format!("\n## Description\n\n{}\n", d.trim()));
                    }
                }
                note.body = body;
                note.properties.insert("hash".into(), json!(v.id));
                note.properties.insert("video_title".into(), json!(v.title));
                if let Some(d) = v.duration {
                    note.properties.insert("duration".into(), json!(d));
                }
                if let Some(vp) = &video_rel {
                    note.properties.insert("video".into(), json!(vp));
                    files.push(vp.clone());
                }
                if let Some(t) = &thumb_rel {
                    files.push(t.clone());
                }
                format!(
                    "Downloaded \"{}\"{}",
                    v.title,
                    v.duration.map(|d| format!(" ({})", hms(d))).unwrap_or_default()
                )
            }
            SourceKind::Pdf | SourceKind::File => {
                let fetch_url = if kind == SourceKind::Pdf {
                    resolve_pdf_url(url)
                } else {
                    url.to_string()
                };
                let guess = url_path(&fetch_url).rsplit('/').next().unwrap_or("").to_string();
                let mut name = safe_attachment_name(&guess);
                if kind == SourceKind::Pdf && !name.to_ascii_lowercase().ends_with(".pdf") {
                    name = format!("{}.pdf", name.trim_end_matches('.'));
                }
                let tmp = dir.join("download.part");
                let (hash, disposition) = fetch_to_file(&fetch_url, &tmp)?;
                if let Some(d) = disposition {
                    let d = safe_attachment_name(&d);
                    if d != "file" && d.contains('.') {
                        name = d;
                    }
                }
                if name == "file" || name.is_empty() {
                    name = format!("download-{}", &hash[..8]);
                }
                let dest = dir.join(&name);
                let _ = fs::remove_file(&dest);
                fs::rename(&tmp, &dest)?;
                let rel = format!("{folder}/{name}");
                files.push(rel.clone());
                let is_asset_embed = matches!(
                    crate::assets::AssetKind::from_ext(name.rsplit('.').next().unwrap_or("")),
                    crate::assets::AssetKind::Image
                        | crate::assets::AssetKind::Vector
                        | crate::assets::AssetKind::Video
                        | crate::assets::AssetKind::Audio
                        | crate::assets::AssetKind::Pdf
                        | crate::assets::AssetKind::Model
                );
                if kind == SourceKind::Pdf && opts.title.is_none() {
                    if let Some(t) = pdf_title(&dest) {
                        note.title = unique_title(self, &md_escape_title(&t));
                    }
                }
                let mut body = format!("# {}\n\nsource:: {url}\n{project_line}\n", note.title);
                body.push_str(&format!(
                    "{}[{}]({})\n",
                    if is_asset_embed { "!" } else { "" },
                    name,
                    encode_md_path(&rel)
                ));
                note.body = body;
                note.properties.insert("hash".into(), json!(hash));
                note.properties.insert("file".into(), json!(rel));
                format!("Downloaded {name}")
            }
        };
        note.properties.insert("fetched".into(), json!(Utc::now().to_rfc3339()));
        note.properties.insert("update_available".into(), json!(false));
        note.properties.remove("update_summary");
        let note = self.write(note)?;
        let note = self.rename_to_title(note)?;
        // The files just written are assets: index them now rather than at the next sync.
        self.sync()?;
        Ok(ImportReport {
            note,
            kind,
            folder,
            files,
            summary,
        })
    }
}

/// arXiv abstract pages have a sibling PDF; direct PDF links pass through.
fn resolve_pdf_url(url: &str) -> String {
    if url_host(url).as_deref() == Some("arxiv.org") {
        let path = url_path(url);
        if let Some(id) = path.strip_prefix("/abs/") {
            return format!("https://arxiv.org/pdf/{}", id.trim_end_matches('/'));
        }
    }
    url.to_string()
}

fn pdf_title(path: &Path) -> Option<String> {
    if !tools().pdfinfo {
        return None;
    }
    let out = run("pdfinfo", &[&path.to_string_lossy()], None).ok()?;
    out.lines().find_map(|l| {
        l.strip_prefix("Title:")
            .map(|t| t.trim().trim_matches('\0').to_string())
            .filter(|t| t.len() > 3 && !t.eq_ignore_ascii_case("untitled"))
    })
}

/// Percent-encode spaces and parentheses so the path works in Markdown links.
pub fn encode_md_path(rel: &str) -> String {
    rel.replace('%', "%25")
        .replace(' ', "%20")
        .replace('(', "%28")
        .replace(')', "%29")
}

/// A title no existing note has, suffixing `(2)`, `(3)`… when needed.
fn unique_title(vault: &Vault, title: &str) -> String {
    let title = title.trim();
    let title = if title.is_empty() { "Source" } else { title };
    let mut candidate = title.to_string();
    let mut n = 1;
    while vault.resolve(&candidate).ok().flatten().is_some() {
        n += 1;
        candidate = format!("{title} ({n})");
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[test]
    fn classifies_urls() {
        assert_eq!(classify_url("https://github.com/DrFlGd/Graphite"), SourceKind::Github);
        assert_eq!(
            classify_url("https://github.com/DrFlGd/Graphite/tree/main/core"),
            SourceKind::Github
        );
        assert_eq!(classify_url("https://github.com/features"), SourceKind::Page);
        assert_eq!(classify_url("git@github.com:DrFlGd/Graphite.git"), SourceKind::Git);
        assert_eq!(classify_url("https://example.com/repo.git"), SourceKind::Git);
        assert_eq!(classify_url("https://www.youtube.com/watch?v=abc"), SourceKind::Video);
        assert_eq!(classify_url("https://youtu.be/abc"), SourceKind::Video);
        assert_eq!(classify_url("https://vimeo.com/123"), SourceKind::Video);
        assert_eq!(classify_url("https://arxiv.org/abs/2301.00001"), SourceKind::Pdf);
        assert_eq!(classify_url("https://example.com/paper.pdf?dl=1"), SourceKind::Pdf);
        assert_eq!(classify_url("https://example.com/model.stl"), SourceKind::File);
        assert_eq!(classify_url("https://example.com/post.html"), SourceKind::Page);
        assert_eq!(classify_url("https://example.com/blog/post"), SourceKind::Page);
        assert_eq!(
            clone_url("https://github.com/DrFlGd/Graphite/tree/main/core"),
            "https://github.com/DrFlGd/Graphite.git"
        );
        assert_eq!(repo_name("https://github.com/DrFlGd/Graphite/"), "DrFlGd/Graphite");
        assert_eq!(repo_name("git@github.com:DrFlGd/Graphite.git"), "DrFlGd/Graphite");
        assert_eq!(
            resolve_pdf_url("https://arxiv.org/abs/2301.00001"),
            "https://arxiv.org/pdf/2301.00001"
        );
        assert_eq!(
            resolve_url("https://a.b/c/d.html", "../x.png"),
            "https://a.b/c/../x.png"
        );
        assert_eq!(resolve_url("https://a.b/c/d.html", "/x.png"), "https://a.b/x.png");
        assert_eq!(
            resolve_url("https://a.b/c/d.html", "//cdn.c/x.png"),
            "https://cdn.c/x.png"
        );
        assert_eq!(resolve_url("https://a.b/c/d.html", "e.png"), "https://a.b/c/e.png");
    }

    #[test]
    fn extracts_readable_text() {
        let html = r#"<html><head><title>Hello Page</title><meta name="description" content="A test"></head>
        <body><nav><a href="/">Home</a></nav><article><h1>Hello</h1><p>Some <b>bold</b> text and a <a href="/next">link</a>.</p>
        <img src="images/pic.png" alt="pic"><script>var x = 1;</script></article><footer>foot</footer></body></html>"#;
        let p = extract_page("https://site.test/blog/post.html", html);
        assert_eq!(p.title, "Hello Page");
        assert_eq!(p.description.as_deref(), Some("A test"));
        assert!(p.markdown.contains("# Hello"), "{}", p.markdown);
        assert!(p.markdown.contains("**bold**"));
        assert!(p.markdown.contains("(https://site.test/next)"), "{}", p.markdown);
        assert!(!p.markdown.contains("var x"));
        assert!(!p.markdown.contains("Home"));
        assert_eq!(p.images, vec!["https://site.test/blog/images/pic.png"]);
        assert_eq!(
            demote_headings("# A\n```\n# not a heading\n```\n## B"),
            "## A\n```\n# not a heading\n```\n### B"
        );
    }

    /// Minimal HTTP server: serves `body` (shared, so tests can change it) for every GET.
    fn serve(body: Arc<Mutex<(String, String)>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let body = body.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                    loop {
                        let mut h = String::new();
                        if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                            break;
                        }
                    }
                    let (html, img) = body.lock().unwrap().clone();
                    let (ctype, data): (&str, Vec<u8>) = if path.ends_with(".png") {
                        ("image/png", img.into_bytes())
                    } else {
                        ("text/html; charset=utf-8", html.into_bytes())
                    };
                    let mut s = stream;
                    let _ = write!(
                        s,
                        "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        data.len()
                    );
                    let _ = s.write_all(&data);
                });
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn imports_a_web_page_and_checks_for_updates() {
        // No screenshots in tests: a headless browser is slow and not what is under test.
        std::env::set_var("GRAPHITE_BROWSER", "none");
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::open(dir.path()).unwrap();
        let content = Arc::new(Mutex::new((
            "<html><head><title>Motor Notes</title></head><body><article><h1>Motors</h1><p>Brushless motors are efficient and quiet in the garden.</p><img src=\"/pic.png\" alt=\"p\"></article></body></html>".to_string(),
            "PNGDATA".to_string(),
        )));
        let base = serve(content.clone());
        let url = format!("{base}/blog/motors");
        let report = v
            .import_url(
                &url,
                &ImportOptions {
                    kind: None,
                    project: Some("Garden Robot".into()),
                    title: None,
                },
            )
            .unwrap();
        assert_eq!(report.kind, SourceKind::Page);
        assert_eq!(report.note.title, "Motor Notes");
        assert_eq!(report.note.note_type, "source");
        assert!(report.note.body.contains("Brushless motors"), "{}", report.note.body);
        assert!(report.note.body.contains("project:: [[Garden Robot]]"));
        assert!(report.files.iter().any(|f| f.ends_with("page.html")));
        assert!(
            report.files.iter().any(|f| f.ends_with("images/01-pic.png")),
            "{:?}",
            report.files
        );
        assert!(dir.path().join(&report.folder).join("images/01-pic.png").is_file());
        assert!(report.note.body.contains("images/01-pic.png"));
        assert_eq!(prop_str(&report.note, "source_type").as_deref(), Some("page"));
        // The page files are assets now.
        assert!(v.asset(&format!("{}/page.html", report.folder)).is_ok());

        let rows = v.sources().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].url, url);
        assert!(!rows[0].update_available);

        // Unchanged → not flagged; changed → flagged with a summary; reimport clears it.
        let c = v.check_source(&report.note.id).unwrap();
        assert!(!c.changed, "{}", c.summary);
        content.lock().unwrap().0 = "<html><head><title>Motor Notes</title></head><body><article><h1>Motors</h1><p>Brushless motors are efficient and quiet in the garden. Now with a second paragraph about torque curves.</p></article></body></html>".into();
        let c = v.check_source(&report.note.id).unwrap();
        assert!(c.changed);
        assert!(c.summary.contains("changed"), "{}", c.summary);
        assert!(v.sources().unwrap()[0].update_available);
        let again = v.reimport_source(&report.note.id).unwrap();
        assert!(again.note.body.contains("torque curves"));
        assert_eq!(again.note.id, report.note.id);
        assert!(!v.sources().unwrap()[0].update_available);
        assert!(
            v.check_all_sources(Some(Duration::from_secs(3600))).unwrap().is_empty(),
            "just checked"
        );
        assert_eq!(v.check_all_sources(None).unwrap().len(), 1);

        // A second import of the same page gets its own title and folder.
        let second = v.import_url(&url, &ImportOptions::default()).unwrap();
        assert_eq!(second.note.title, "Motor Notes (2)");
        assert_ne!(second.folder, report.folder);

        // A direct file URL is downloaded as an asset.
        let file = v
            .import_url(&format!("{base}/data/pic.png"), &ImportOptions::default())
            .unwrap();
        assert_eq!(file.kind, SourceKind::File);
        assert!(file.files[0].ends_with("/pic.png"), "{:?}", file.files);
        assert!(file.note.body.contains("![pic.png]("));
        let c = v.check_source(&file.note.id).unwrap();
        assert!(!c.changed);
        content.lock().unwrap().1 = "PNGDATA2".into();
        assert!(v.check_source(&file.note.id).unwrap().changed);

        // Bad input leaves nothing behind.
        assert!(v.import_url("not a url", &ImportOptions::default()).is_err());
        assert_eq!(v.sources().unwrap().len(), 3);
    }

    #[test]
    fn imports_a_git_repository_and_notices_new_commits() {
        if !tools().git {
            eprintln!("git not installed; skipping");
            return;
        }
        let upstream = tempfile::tempdir().unwrap();
        let up = upstream.path();
        let git = |args: &[&str]| run("git", args, Some(up)).unwrap();
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        fs::write(up.join("README.md"), "# Widget\n\nA widget library.\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "first"]);
        let first = git(&["rev-parse", "HEAD"]).trim().to_string();

        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::open(dir.path()).unwrap();
        let url = format!("file://{}", up.display());
        let report = v
            .import_url(
                &url,
                &ImportOptions {
                    kind: Some(SourceKind::Git),
                    project: None,
                    title: Some("widget".into()),
                },
            )
            .unwrap();
        assert_eq!(report.note.title, "widget");
        assert!(report.note.body.contains("## README.md"));
        assert!(report.note.body.contains("## Widget"), "{}", report.note.body);
        assert!(report.note.body.contains("A widget library."));
        assert_eq!(prop_str(&report.note, "commit").as_deref(), Some(first.as_str()));
        assert!(dir.path().join(&report.folder).join("repo/README.md").is_file());
        assert!(!v.check_source(&report.note.id).unwrap().changed);

        fs::write(up.join("README.md"), "# Widget\n\nA widget library, now with gears.\n").unwrap();
        git(&["commit", "-q", "-am", "gears"]);
        let c = v.check_source(&report.note.id).unwrap();
        assert!(c.changed, "{}", c.summary);
        assert!(c.summary.starts_with("New commits"));
        let again = v.reimport_source(&report.note.id).unwrap();
        assert!(again.note.body.contains("now with gears"));
        assert!(again.summary.starts_with("Updated"), "{}", again.summary);
        assert!(!v.check_source(&report.note.id).unwrap().changed);
    }
}
