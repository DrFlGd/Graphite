//! `graphite` CLI: the same core the desktop app uses, from a shell.
//!
//! The vault is `--vault <dir>` or `$GRAPHITE_VAULT` or the current directory.

use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use graphite_core::{ItemFilter, ItemKind, ItemRow, ItemStatus, Vault};

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
        /// Read the body from stdin
        #[arg(long)]
        stdin: bool,
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
            let hits = vault.search(&query.join(" "), limit)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&hits)?);
            } else {
                for h in hits {
                    println!("{}  {}\n    {}", h.id, h.title, h.snippet.replace('\n', " "));
                }
            }
        }
        Cmd::New {
            title,
            r#type,
            folder,
            stdin,
        } => {
            let mut note = vault.create(&title, r#type.as_deref(), folder.as_deref())?;
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
