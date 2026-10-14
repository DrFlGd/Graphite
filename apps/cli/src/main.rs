//! `graphite` CLI: the same core the desktop app uses, from a shell.
//!
//! The vault is `--vault <dir>` or `$GRAPHITE_VAULT` or the current directory.

use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use graphite_core::Vault;

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

fn resolve(vault: &Vault, target: &str) -> Result<String> {
    if vault.summary(target)?.is_some() {
        return Ok(target.to_string());
    }
    vault
        .resolve(target)?
        .with_context(|| format!("no note matches '{target}'"))
}
