//! Tauri shell: holds the open vault and exposes core operations as
//! commands. The UI never touches the file system directly.

use std::path::PathBuf;
use std::sync::Mutex;

use graphite_core::{Backlink, Note, NoteSummary, SearchHit, Vault, VaultInfo, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
struct AppState {
    vault: Mutex<Option<Vault>>,
    watcher: Mutex<Option<Watcher>>,
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn with_vault<T>(state: &State<AppState>, f: impl FnOnce(&mut Vault) -> graphite_core::Result<T>) -> CmdResult<T> {
    let mut guard = state.vault.lock().map_err(err)?;
    let vault = guard.as_mut().ok_or_else(|| "no vault open".to_string())?;
    f(vault).map_err(err)
}

/// Payload of the `vault://changed` event.
#[derive(Serialize, Clone)]
struct ChangedPayload {
    ids: Vec<String>,
}

#[tauri::command]
fn open_vault(app: AppHandle, state: State<AppState>, path: String) -> CmdResult<VaultInfo> {
    let root = PathBuf::from(&path);
    let vault = Vault::open(&root).map_err(err)?;
    let info = vault.info().map_err(err)?;
    let root = vault.root().to_path_buf();
    // Let the webview load images and other attachments from this vault via the asset protocol.
    app.asset_protocol_scope().allow_directory(&root, true).map_err(err)?;
    *state.vault.lock().map_err(err)? = Some(vault);

    // External edits: reindex the touched files and tell the UI which notes changed.
    let handle = app.clone();
    let watcher = Watcher::new(&root, move |ev| {
        let state = handle.state::<AppState>();
        let mut ids = Vec::new();
        if let Ok(mut guard) = state.vault.lock() {
            if let Some(vault) = guard.as_mut() {
                for p in &ev.paths {
                    if p.is_dir() {
                        let _ = vault.sync();
                        continue;
                    }
                    match vault.reindex_path(p) {
                        Ok(Some(id)) => ids.push(id),
                        Ok(None) => {}
                        Err(e) => tracing::warn!("reindex {}: {e}", p.display()),
                    }
                }
            }
        }
        let _ = handle.emit("vault://changed", ChangedPayload { ids });
    })
    .map_err(err)?;
    *state.watcher.lock().map_err(err)? = Some(watcher);
    Ok(info)
}

#[tauri::command]
fn vault_info(state: State<AppState>) -> CmdResult<Option<VaultInfo>> {
    let guard = state.vault.lock().map_err(err)?;
    match guard.as_ref() {
        Some(v) => v.info().map(Some).map_err(err),
        None => Ok(None),
    }
}

#[tauri::command]
fn list_notes(state: State<AppState>) -> CmdResult<Vec<NoteSummary>> {
    with_vault(&state, |v| v.list())
}

#[tauri::command]
fn read_note(state: State<AppState>, id: String) -> CmdResult<Note> {
    with_vault(&state, |v| v.read(&id))
}

#[tauri::command]
fn save_note(state: State<AppState>, note: Note) -> CmdResult<Note> {
    with_vault(&state, |v| v.write(&note))
}

#[tauri::command]
fn create_note(
    state: State<AppState>,
    title: String,
    note_type: Option<String>,
    folder: Option<String>,
) -> CmdResult<Note> {
    with_vault(&state, |v| v.create(&title, note_type.as_deref(), folder.as_deref()))
}

#[tauri::command]
fn delete_note(state: State<AppState>, id: String) -> CmdResult<()> {
    with_vault(&state, |v| v.delete(&id))
}

#[tauri::command]
fn search(state: State<AppState>, query: String, limit: Option<usize>) -> CmdResult<Vec<SearchHit>> {
    with_vault(&state, |v| v.search(&query, limit.unwrap_or(30)))
}

#[tauri::command]
fn backlinks(state: State<AppState>, id: String) -> CmdResult<Vec<Backlink>> {
    with_vault(&state, |v| v.backlinks(&id))
}

#[tauri::command]
fn unlinked_mentions(state: State<AppState>, id: String) -> CmdResult<Vec<NoteSummary>> {
    with_vault(&state, |v| v.unlinked_mentions(&id))
}

#[derive(Serialize)]
struct Outgoing {
    target: String,
    id: Option<String>,
}

#[tauri::command]
fn outgoing_links(state: State<AppState>, id: String) -> CmdResult<Vec<Outgoing>> {
    with_vault(&state, |v| v.outgoing(&id)).map(|v| v.into_iter().map(|(target, id)| Outgoing { target, id }).collect())
}

/// Follow a `[[link]]`: returns the id, creating the note if it does not exist.
#[tauri::command]
fn follow_link(state: State<AppState>, target: String) -> CmdResult<String> {
    with_vault(&state, |v| v.resolve_or_create(&target))
}

#[tauri::command]
fn complete_title(state: State<AppState>, prefix: String) -> CmdResult<Vec<NoteSummary>> {
    with_vault(&state, |v| v.complete_title(&prefix, 12))
}

#[tauri::command]
fn rebuild_index(state: State<AppState>) -> CmdResult<usize> {
    with_vault(&state, |v| v.rebuild())
}

/// Copy a file from anywhere on disk into the vault's attachments folder and
/// return its vault-relative path (what goes into the Markdown).
#[tauri::command]
fn attach_file(state: State<AppState>, src: String, folder: Option<String>) -> CmdResult<String> {
    with_vault(&state, |v| {
        let src = PathBuf::from(&src);
        let name = src
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| graphite_core::Error::PathEscapesVault(src.clone()))?;
        let folder = folder.unwrap_or_else(|| graphite_core::vault::ATTACHMENTS_DIR.to_string());
        let mut rel = format!("{folder}/{name}");
        let mut dest = v.safe_join(&rel)?;
        // Never overwrite: suffix the name until it is free.
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) => (s.to_string(), format!(".{e}")),
            None => (name.to_string(), String::new()),
        };
        let mut n = 1;
        while dest.exists() {
            n += 1;
            rel = format!("{folder}/{stem}-{n}{ext}");
            dest = v.safe_join(&rel)?;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src, &dest)?;
        Ok(rel)
    })
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_vault,
            vault_info,
            list_notes,
            read_note,
            save_note,
            create_note,
            delete_note,
            search,
            backlinks,
            unlinked_mentions,
            outgoing_links,
            follow_link,
            complete_title,
            rebuild_index,
            attach_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Graphite");
}
