//! `graphite` CLI: the same core the desktop app uses, from a shell.
//!
//! The vault is `--vault <dir>` or `$GRAPHITE_VAULT` or the current directory.

use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use graphite_core::{
    assets::format_size, AssetKind, Domain, ImportOptions, ItemFilter, ItemKind, ItemRow, ItemStatus, SourceKind, Vault,
};

#[derive(Parser)]
#[command(name = "graphite", version, about = "Graphite second-brain CLI")]
struct Cli {
    /// Vault folder (defaults to $GRAPHITE_VAULT, then the current directory)
    #[arg(long, env = "GRAPHITE_VAULT", global = true)]
    vault: Option<PathBuf>,
    /// Output JSON instead of text
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum RelayCmd {
    /// Make a new key for a relay and print the invite for the other devices
    Create {
        /// The relay's address, such as https://relay.example.com
        url: String,
    },
    /// Connect this device with an invite shown on another device
    Connect { invite: String },
    /// Print the invite for this vault's relay
    Invite,
    /// Stop syncing through the relay on this device
    Disconnect,
    /// Show the relay and what has been exchanged
    Status,
}

#[derive(Subcommand)]
enum Cmd {
    /// Initialise a vault (creates notes/, attachments/, .graphite/) and index it
    Init,
    /// Rebuild the index from the files on disk
    Reindex,
    /// List notes
    List,
    /// Full-text search
    Search {
        query: Vec<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Create a note. Body is read from stdin if `-` is given.
    New {
        title: String,
        #[arg(long, short = 't')]
        r#type: Option<String>,
        #[arg(long)]
        folder: Option<String>,
        /// Start from a template (see `graphite templates`)
        #[arg(long)]
        template: Option<String>,
        /// Read the body from stdin
        #[arg(long)]
        stdin: bool,
    },
    /// List templates: built-in ones plus templates/*.md in the vault
    Templates,
    /// List files (assets) in the vault, newest first
    Assets {
        /// Only this kind: image, vector, video, audio, pdf, model, code, data, document, archive, other
        #[arg(long, short = 'k')]
        kind: Option<String>,
        /// Only paths containing this text
        filter: Option<String>,
    },
    /// Show one file's metadata, previews and the notes that reference it
    Asset { path: String },
    /// Compute hashes, metadata, thumbnails and searchable text for files not yet processed
    Derive {
        /// Stop after this many files (default: all)
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Which optional tools (ffmpeg, poppler, git, yt-dlp) were found on the PATH
    Tools,
    /// Import a URL: a GitHub repo, a web page, a YouTube video, a PDF or any file
    Import {
        url: String,
        /// Link the source to this project (title or id)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Importer to use instead of guessing from the URL: github, git, video, page, pdf, file
        #[arg(long, short = 'k')]
        kind: Option<String>,
        /// Title for the source note
        #[arg(long, short = 't')]
        title: Option<String>,
    },
    /// List imported sources and whether updates are available
    Sources,
    /// Check one source (or all of them with --all) for changes upstream
    Check {
        /// Source note (title or id)
        source: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Fetch a source again, replacing its files and note body
    Reimport {
        /// Source note (title or id)
        source: String,
    },
    /// Merge this device's notes with the other devices sharing the vault
    /// folder (turns sync on for the vault the first time)
    Sync {
        /// Only show the devices and when they last synced
        #[arg(long)]
        status: bool,
    },
    /// Show this device's id and name
    Device,
    /// Sync through a relay server (for devices without a shared folder)
    Relay {
        #[command(subcommand)]
        action: RelayCmd,
    },
    /// Show or change this vault's settings on this device
    Config {
        /// Files over this many MB are referenced in place and stay on this device
        #[arg(long)]
        large_file_mb: Option<u64>,
    },
    /// List a note's saved versions, newest first
    History {
        /// Note (title or id)
        note: String,
        /// Print one version's text instead of the list
        #[arg(long)]
        show: Option<String>,
    },
    /// Make a saved version the note's current content
    Restore {
        /// Note (title or id)
        note: String,
        /// Version id, as listed by `history`
        version: String,
    },
    /// Zip the vault (everything but the .graphite cache) into the snapshot folder
    Snapshot {
        /// Folder to write into (default: .graphite/snapshots or $GRAPHITE_SNAPSHOT_DIR)
        #[arg(long)]
        to: Option<PathBuf>,
        /// Only list the snapshots already taken
        #[arg(long)]
        list: bool,
    },
    /// Turn a note (an idea, say) into a project, keeping its text
    Promote {
        target: String,
        /// Project template to use (default: `project`)
        #[arg(long)]
        template: Option<String>,
    },
    /// A project's hub: progress, tasks, gaps, decisions, linked notes
    Hub { project: String },
    /// Run a query, e.g. `graphite query 'type:task status:open sort:due'`,
    /// or the saved query in a `type: query` note with `--saved`
    Query {
        query: Vec<String>,
        /// Run the query stored in this note (id, title or path)
        #[arg(long)]
        saved: Option<String>,
    },
    /// Quick capture into the inbox: `graphite capture "a thought"` or pipe text in
    Capture {
        text: Vec<String>,
        /// Files to copy into attachments/ and link from the capture
        #[arg(long = "file", short = 'f')]
        files: Vec<PathBuf>,
    },
    /// Open (create if needed) today's daily note and print its path
    Daily,
    /// Print a note (by id, title, alias or path)
    Show { target: String },
    /// Notes linking to a note
    Backlinks { target: String },
    /// Tasks (`- [ ]`) across the vault, open ones by default
    Tasks {
        /// Only tasks belonging to this project (id, title, alias or path)
        #[arg(long)]
        project: Option<String>,
        /// Only tasks in this note
        #[arg(long)]
        note: Option<String>,
        /// Include done and cancelled tasks
        #[arg(long)]
        all: bool,
        /// Due on or before this date (YYYY-MM-DD)
        #[arg(long)]
        due: Option<String>,
    },
    /// Gaps (`- [?]` information needs), unanswered ones by default
    Gaps {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Tick a task: `graphite done <note> <line>` (line numbers come from `tasks`)
    Done {
        note: String,
        line: usize,
        /// Set a status other than done: open, doing, cancelled
        #[arg(long)]
        status: Option<String>,
    },
    /// Record the note that answers a gap
    Answer { note: String, line: usize, answer: String },
    /// Set an inline field on a task or gap line (`due`, `priority`, `project`, …); an empty value removes it
    Field {
        note: String,
        line: usize,
        key: String,
        #[arg(default_value = "")]
        value: String,
    },
    /// Add a task (or a gap with --gap) to a note
    Add {
        note: String,
        text: Vec<String>,
        #[arg(long)]
        gap: bool,
    },
    /// Task progress of a project
    Progress { project: String },
    /// The link graph (all notes, or the neighbourhood of one)
    Graph {
        /// Only notes within --hops of this note (id, title, alias or path)
        #[arg(long)]
        around: Option<String>,
        #[arg(long, default_value_t = 1)]
        hops: usize,
    },
}

fn print_relay_report(r: &graphite_core::RelayReport) {
    println!(
        "sent {}, received {}, removed {}, notes pulled {}",
        r.uploaded.len(),
        r.downloaded.len(),
        r.deleted.len(),
        r.sync.pulled.len()
    );
    if !r.kept_local.is_empty() {
        println!(
            "{} file(s) over the large-file limit stay on this device",
            r.kept_local.len()
        );
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let root = cli
        .vault
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    let mut vault = Vault::open(&root).with_context(|| format!("opening vault at {}", root.display()))?;

    match cli.cmd {
        Cmd::Init => {
            let info = vault.info()?;
            println!("Vault ready at {} ({} notes)", info.root, info.note_count);
        }
        Cmd::Reindex => {
            let n = vault.rebuild()?;
            println!("Reindexed {n} notes");
        }
        Cmd::List => {
            let notes = vault.list()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&notes)?);
            } else {
                for n in notes {
                    println!("{}\t{}\t{}", n.id, n.note_type, n.title);
                }
            }
        }
        Cmd::Search { query, limit } => {
            let q = query.join(" ");
            let hits = vault.search(&q, limit)?;
            let files = vault.search_assets(&q, limit)?;
            if cli.json {
                println!("{}", serde_json::json!({ "notes": hits, "files": files }));
            } else {
                for h in hits {
                    println!("{}  {}\n    {}", h.id, h.title, plain_snippet(&h.snippet));
                }
                for f in files {
                    println!("{}  {}\n    {}", f.kind.as_str(), f.path, plain_snippet(&f.snippet));
                }
            }
        }
        Cmd::Assets { kind, filter } => {
            let kind = kind.as_deref().map(AssetKind::parse);
            let assets = vault.assets(kind, filter.as_deref())?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&assets)?);
            } else {
                for a in assets {
                    println!("{:<9} {:>9}  {}", a.kind.as_str(), format_size(a.size), a.path);
                }
            }
        }
        Cmd::Asset { path } => {
            let asset = vault.asset(&path)?;
            let refs = vault.asset_references(&asset.path)?;
            if cli.json {
                println!("{}", serde_json::json!({ "asset": asset, "references": refs }));
            } else {
                println!(
                    "{}\n  kind {}  size {}  modified {}",
                    asset.path,
                    asset.kind.as_str(),
                    format_size(asset.size),
                    asset.modified.to_rfc3339()
                );
                if let (Some(w), Some(h)) = (asset.width, asset.height) {
                    println!("  {w} x {h}");
                }
                if let Some(d) = asset.duration {
                    println!("  duration {d:.1} s");
                }
                if let Some(p) = asset.pages {
                    println!("  pages {p}");
                }
                for (k, v) in &asset.meta {
                    println!("  {k} {v}");
                }
                if let Some(h) = &asset.hash {
                    println!("  sha256 {h}");
                }
                if let Some(t) = &asset.thumbnail {
                    println!("  thumbnail {t}");
                }
                if let Some(t) = &asset.text {
                    println!("  text {t}");
                }
                for r in refs {
                    println!("  referenced by {} ({})", r.from.title, r.from.path);
                }
            }
        }
        Cmd::Derive { limit } => {
            let mut total = 0;
            loop {
                let batch = limit.map(|l| (l - total).min(50)).unwrap_or(50);
                if batch == 0 {
                    break;
                }
                let done = vault.derive_assets(batch)?;
                if done.is_empty() {
                    break;
                }
                for a in &done {
                    if !cli.json {
                        println!("{}", a.path);
                    }
                }
                total += done.len();
            }
            if cli.json {
                println!("{}", serde_json::json!({ "derived": total }));
            } else {
                println!("Derived {total} files");
            }
        }
        Cmd::Import {
            url,
            project,
            kind,
            title,
        } => {
            let kind = match kind.as_deref() {
                Some(k) => Some(SourceKind::parse(k).with_context(|| format!("unknown importer '{k}'"))?),
                None => None,
            };
            let project = match project {
                Some(p) => Some(vault.summary(&resolve(&vault, &p)?)?.map(|s| s.title).unwrap_or(p)),
                None => None,
            };
            let report = vault.import_url(&url, &ImportOptions { kind, project, title })?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "{}\n  {}\n  note {}\n  files in {}/",
                    report.summary,
                    report.kind.as_str(),
                    report.note.path,
                    report.folder
                );
            }
        }
        Cmd::Sources => {
            let rows = vault.sources()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else {
                for r in rows {
                    println!(
                        "{:<7} {:<8} {}\n        {}",
                        r.kind.as_str(),
                        if r.update_available { "changed" } else { "current" },
                        r.note.title,
                        r.url
                    );
                }
            }
        }
        Cmd::Check { source, all } => {
            let checks = if all {
                vault.check_all_sources(None)?
            } else {
                let target = source.context("give a source note, or --all")?;
                let id = resolve(&vault, &target)?;
                vec![vault.check_source(&id)?]
            };
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&checks)?);
            } else {
                for c in checks {
                    println!(
                        "{:<8} {}: {}",
                        if c.changed { "changed" } else { "current" },
                        c.title,
                        c.summary
                    );
                }
            }
        }
        Cmd::Reimport { source } => {
            let id = resolve(&vault, &source)?;
            let report = vault.reimport_source(&id)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}\n  note {}", report.summary, report.note.path);
            }
        }
        Cmd::Sync { status } => {
            if status {
                let s = vault.sync_status()?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&s)?);
                } else if !s.enabled {
                    println!("Sync is off for this vault. Run `graphite sync` to turn it on.");
                } else {
                    for d in s.devices {
                        println!(
                            "{:<24} {:<5} notes  last sync {}{}",
                            d.name,
                            d.notes,
                            d.last_sync
                                .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
                                .unwrap_or_else(|| "never".into()),
                            if d.this_device { "  (this device)" } else { "" }
                        );
                    }
                }
            } else if vault.relay_config().is_some() {
                let r = vault.relay_sync()?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&r)?);
                } else {
                    print_relay_report(&r);
                }
            } else {
                let r = vault.sync_now()?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&r)?);
                } else {
                    println!(
                        "published {}, pulled {}, deleted {}, conflicts folded {}, other devices {}",
                        r.pushed,
                        r.pulled.len(),
                        r.deleted.len(),
                        r.conflicts,
                        r.devices
                    );
                    if r.pending > 0 {
                        println!(
                            "{} note(s) changed outside Graphite; run again shortly to record them",
                            r.pending
                        );
                    }
                }
            }
        }
        Cmd::Relay { action } => match action {
            RelayCmd::Create { url } => {
                let config = graphite_core::RelayConfig::new(&url)?;
                vault.set_relay(Some(config.clone()))?;
                let r = vault.relay_sync().inspect_err(|_| {
                    let _ = vault.set_relay(None);
                })?;
                println!("Connected. Paste this invite on your other devices (it is the key to the vault, so keep it private):\n{}", config.invite());
                print_relay_report(&r);
            }
            RelayCmd::Connect { invite } => {
                let config = graphite_core::RelayConfig::from_invite(&invite)?;
                vault.set_relay(Some(config))?;
                let r = vault.relay_sync().inspect_err(|_| {
                    let _ = vault.set_relay(None);
                })?;
                println!("Connected.");
                print_relay_report(&r);
            }
            RelayCmd::Invite => match vault.relay_config() {
                Some(c) => println!("{}", c.invite()),
                None => println!("This vault is not connected to a relay on this device."),
            },
            RelayCmd::Disconnect => {
                vault.set_relay(None)?;
                println!("Disconnected. The relay keeps what was sent; other devices are not affected.");
            }
            RelayCmd::Status => match vault.relay_status() {
                Some(st) => {
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&st)?);
                    } else {
                        println!(
                            "relay {}\n  last exchange {}\n  {} files exchanged",
                            st.url,
                            st.last
                                .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
                                .unwrap_or_else(|| "never".into()),
                            st.files
                        );
                        for p in st.kept_local {
                            println!("  kept on this device (over the large-file limit): {p}");
                        }
                    }
                }
                None => println!("This vault is not connected to a relay on this device."),
            },
        },
        Cmd::Config { large_file_mb } => {
            let mut config = vault.config();
            if let Some(mb) = large_file_mb {
                config.large_file_mb = mb;
                vault.save_config(&config)?;
            }
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&config)?);
            } else {
                println!("large_file_mb = {}", config.large_file_mb);
            }
        }
        Cmd::Device => {
            let d = vault.device();
            if cli.json {
                println!("{}", serde_json::to_string_pretty(d)?);
            } else {
                println!("{}  {}", d.id, d.name);
            }
        }
        Cmd::History { note, show } => {
            let id = resolve(&vault, &note)?;
            if let Some(version) = show {
                print!("{}", vault.version_text(&id, &version)?);
            } else {
                let versions = vault.versions(&id)?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&versions)?);
                } else if versions.is_empty() {
                    println!("No saved versions yet.");
                } else {
                    for v in versions {
                        println!(
                            "{}  {}  {:>7}  {}",
                            v.id,
                            v.time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                            format_size(v.size as i64),
                            v.title
                        );
                    }
                }
            }
        }
        Cmd::Restore { note, version } => {
            let id = resolve(&vault, &note)?;
            let n = vault.restore_version(&id, &version)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&n)?);
            } else {
                println!("Restored {} from {version}", n.title);
            }
        }
        Cmd::Snapshot { to, list } => {
            if list {
                let snaps = vault.snapshots(to.as_deref())?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&snaps)?);
                } else if snaps.is_empty() {
                    println!("No snapshots yet.");
                } else {
                    for s in snaps {
                        println!(
                            "{}  {:>8}  {}",
                            s.time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                            format_size(s.size as i64),
                            s.path
                        );
                    }
                }
            } else {
                let s = vault.snapshot(to.as_deref())?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&s)?);
                } else {
                    println!(
                        "{} ({} files, {})",
                        s.path,
                        s.files.unwrap_or(0),
                        format_size(s.size as i64)
                    );
                }
            }
        }
        Cmd::Tools => {
            let t = graphite_core::tools();
            if cli.json {
                println!("{}", serde_json::to_string_pretty(t)?);
            } else {
                for (name, ok, what) in [
                    ("ffmpeg", t.ffmpeg, "video and image thumbnails"),
                    ("ffprobe", t.ffprobe, "video and audio metadata"),
                    ("pdfinfo", t.pdfinfo, "PDF page count and title"),
                    ("pdftotext", t.pdftotext, "PDF text for search"),
                    ("pdftoppm", t.pdftoppm, "PDF thumbnails"),
                    ("git", t.git, "GitHub importer"),
                    ("yt-dlp", t.yt_dlp, "video importer"),
                    ("browser", t.browser, "web page screenshots (Chromium, Chrome or Edge)"),
                ] {
                    println!("{:<10} {}  {}", name, if ok { "found  " } else { "missing" }, what);
                }
            }
        }
        Cmd::New {
            title,
            r#type,
            folder,
            template,
            stdin,
        } => {
            let mut note = match template {
                Some(t) => vault.create_from_template(&title, &t, folder.as_deref())?,
                None => vault.create(&title, r#type.as_deref(), folder.as_deref())?,
            };
            if stdin {
                let mut body = String::new();
                std::io::stdin().read_to_string(&mut body)?;
                note.body = body;
                note = vault.write(&note)?;
            }
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&note)?);
            } else {
                println!("{}\t{}", note.id, note.path);
            }
        }
        Cmd::Capture { text, files } => {
            let mut body = text.join(" ");
            if body.is_empty() && files.is_empty() {
                std::io::stdin().read_to_string(&mut body)?;
            }
            let mut attachments = Vec::new();
            for f in &files {
                attachments.push(vault.import_attachment(f, Some("attachments/Inbox"))?);
            }
            let note = vault.capture(&body, &attachments)?;
            println!("{}\t{}", note.id, note.path);
        }
        Cmd::Daily => {
            let note = vault.daily_note(chrono::Local::now().date_naive())?;
            println!("{}\t{}", note.id, note.path);
        }
        Cmd::Show { target } => {
            let id = resolve(&vault, &target)?;
            let note = vault.read(&id)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&note)?);
            } else {
                print!("{}", note.to_text());
            }
        }
        Cmd::Graph { around, hops } => {
            let graph = match around {
                Some(t) => {
                    let id = resolve(&vault, &t)?;
                    vault.local_graph(&id, hops)?
                }
                None => vault.graph()?,
            };
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&graph)?);
            } else {
                let title = |id: &str| {
                    graph
                        .nodes
                        .iter()
                        .find(|n| n.id == id)
                        .map(|n| n.title.clone())
                        .unwrap_or_default()
                };
                println!("{} notes, {} links", graph.nodes.len(), graph.edges.len());
                for e in &graph.edges {
                    match &e.relation {
                        Some(r) => println!("{} -> {} ({r})", title(&e.from), title(&e.to)),
                        None => println!("{} -> {}", title(&e.from), title(&e.to)),
                    }
                }
            }
        }
        Cmd::Tasks {
            project,
            note,
            all,
            due,
        } => {
            let filter = ItemFilter {
                kind: Some(ItemKind::Task),
                open_only: !all,
                project: project.map(|p| resolve(&vault, &p)).transpose()?,
                note: note.map(|n| resolve(&vault, &n)).transpose()?,
                due_before: due,
                text: None,
            };
            print_items(&vault.items(&filter)?, cli.json)?;
        }
        Cmd::Gaps { project, all } => {
            let filter = ItemFilter {
                kind: Some(ItemKind::Gap),
                open_only: !all,
                project: project.map(|p| resolve(&vault, &p)).transpose()?,
                ..Default::default()
            };
            print_items(&vault.items(&filter)?, cli.json)?;
        }
        Cmd::Done { note, line, status } => {
            let id = resolve(&vault, &note)?;
            let status = match status.as_deref() {
                None => ItemStatus::Done,
                Some(s) => ItemStatus::parse(s).with_context(|| format!("unknown status '{s}'"))?,
            };
            let item = vault.set_item_status(&id, line, status)?;
            println!("[{}] {}", item.status.task_marker(), item.text);
        }
        Cmd::Field { note, line, key, value } => {
            let id = resolve(&vault, &note)?;
            let item = vault.set_item_field(&id, line, &key, &value)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&item)?);
            } else {
                println!("{}:{}\t{}", item.note_id, item.line, item.raw);
            }
        }
        Cmd::Answer { note, line, answer } => {
            let id = resolve(&vault, &note)?;
            let item = vault.answer_gap(&id, line, &answer)?;
            println!("answered: {} -> [[{}]]", item.text, item.answer.unwrap_or_default());
        }
        Cmd::Add { note, text, gap } => {
            let id = resolve(&vault, &note)?;
            let kind = if gap { ItemKind::Gap } else { ItemKind::Task };
            let item = vault.add_item(&id, kind, &text.join(" "))?;
            println!("{}:{}\t{}", item.note_id, item.line, item.text);
        }
        Cmd::Templates => {
            let templates = vault.templates()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&templates)?);
            } else {
                for t in templates {
                    println!(
                        "{:<12} {:<9} {}{}",
                        t.name,
                        t.note_type,
                        t.description,
                        if t.builtin { "" } else { "  (vault)" }
                    );
                }
            }
        }
        Cmd::Promote { target, template } => {
            let id = resolve(&vault, &target)?;
            let note = vault.promote_to_project(&id, template.as_deref())?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&note)?);
            } else {
                println!("{}\t{}\t{}", note.id, note.note_type, note.path);
            }
        }
        Cmd::Hub { project } => {
            let id = resolve(&vault, &project)?;
            let hub = vault.project_hub(&id)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&hub)?);
            } else {
                let p = &hub.progress;
                println!("{} ({})", hub.project.title, hub.project.path);
                println!(
                    "{}% done: {} of {} tasks, {} open gaps",
                    p.percent, p.done, p.tasks, p.gaps_open
                );
                if !hub.tasks.is_empty() {
                    println!("\nTasks");
                    print_items(&hub.tasks, false)?;
                }
                if !hub.gaps.is_empty() {
                    println!("\nOpen questions");
                    print_items(&hub.gaps, false)?;
                }
                if !hub.decisions.is_empty() {
                    println!("\nDecisions");
                    for d in &hub.decisions {
                        println!("  {}", d.title);
                    }
                }
                if !hub.sources.is_empty() {
                    println!("\nSources");
                    for s in &hub.sources {
                        println!("  {}", s.title);
                    }
                }
                if !hub.linked.is_empty() {
                    println!("\nLinked notes");
                    for l in &hub.linked {
                        let rel = l.relation.as_deref().map(|r| format!(" ({r})")).unwrap_or_default();
                        println!("  {} {}{}", if l.incoming { "<-" } else { "->" }, l.note.title, rel);
                    }
                }
            }
        }
        Cmd::Query { query, saved } => {
            let result = match saved {
                Some(target) => {
                    let id = resolve(&vault, &target)?;
                    vault.run_saved_query(&id)?
                }
                None => vault.query(&query.join(" "))?,
            };
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                match result.domain {
                    Domain::Items => print_items(&result.items, false)?,
                    Domain::Notes => {
                        for n in &result.notes {
                            let props: Vec<String> = n
                                .properties
                                .iter()
                                .filter_map(|(k, v)| match v {
                                    serde_json::Value::String(s) => Some(format!("{k}={s}")),
                                    serde_json::Value::Number(x) => Some(format!("{k}={x}")),
                                    serde_json::Value::Bool(b) => Some(format!("{k}={b}")),
                                    _ => None,
                                })
                                .collect();
                            println!(
                                "{}\t{}\t{}\t{}",
                                n.note.id,
                                n.note.note_type,
                                n.note.title,
                                props.join(" ")
                            );
                        }
                    }
                }
                if !result.groups.is_empty() {
                    let groups: Vec<String> = result
                        .groups
                        .iter()
                        .map(|g| format!("{} ({})", g.key, g.count))
                        .collect();
                    println!("groups: {}", groups.join(", "));
                }
                println!("{} result(s)", result.total);
            }
        }
        Cmd::Progress { project } => {
            let id = resolve(&vault, &project)?;
            let p = vault.progress(&id)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&p)?);
            } else {
                println!(
                    "{}% done: {} of {} tasks ({} doing, {} open, {} cancelled); {} open gaps of {}",
                    p.percent, p.done, p.tasks, p.doing, p.open, p.cancelled, p.gaps_open, p.gaps
                );
            }
        }
        Cmd::Backlinks { target } => {
            let id = resolve(&vault, &target)?;
            let links = vault.backlinks(&id)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&links)?);
            } else {
                for b in links {
                    match b.relation {
                        Some(r) => println!("{}\t{}\t({r})", b.from.id, b.from.title),
                        None => println!("{}\t{}", b.from.id, b.from.title),
                    }
                }
            }
        }
    }
    Ok(())
}

fn print_items(rows: &[ItemRow], json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(rows)?);
        return Ok(());
    }
    let mut last_note = "";
    for r in rows {
        if r.note_path != last_note {
            println!("{}", r.note_title);
            last_note = &r.note_path;
        }
        let marker = match r.item.kind {
            ItemKind::Gap if r.item.status == ItemStatus::Answered => '!',
            ItemKind::Gap => '?',
            ItemKind::Task => r.item.status.task_marker(),
        };
        let mut extra = Vec::new();
        if let Some(d) = &r.item.due {
            extra.push(format!("due {d}"));
        }
        if let Some(p) = r.item.priority {
            extra.push(format!("p{p}"));
        }
        if let Some(a) = &r.item.answer {
            extra.push(format!("answered in [[{a}]]"));
        }
        if let Some(p) = &r.project_title {
            if *p != r.note_title {
                extra.push(format!("project {p}"));
            }
        }
        let extra = if extra.is_empty() {
            String::new()
        } else {
            format!("  ({})", extra.join(", "))
        };
        println!(
            "  {:>4}  {}[{marker}] {}{extra}",
            r.item.line,
            "  ".repeat(r.item.depth),
            r.item.text
        );
    }
    Ok(())
}

fn resolve(vault: &Vault, target: &str) -> Result<String> {
    if vault.summary(target)?.is_some() {
        return Ok(target.to_string());
    }
    vault
        .resolve(target)?
        .with_context(|| format!("no note matches '{target}'"))
}

/// A search snippet for the terminal: matches in `[` `]`, one line.
fn plain_snippet(s: &str) -> String {
    s.replace(graphite_core::index::SNIPPET_START, "[")
        .replace(graphite_core::index::SNIPPET_END, "]")
        .replace('\n', " ")
}
