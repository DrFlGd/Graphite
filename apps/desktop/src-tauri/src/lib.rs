//! Tauri shell: holds the open vault and exposes core operations as
//! commands. The UI never touches the file system directly.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use graphite_core::{
    Asset, AssetHit, AssetKind, Backlink, Graph, Item, ItemFilter, ItemKind, ItemRow, ItemStatus, Note, NoteSummary,
    Progress, ProjectHub, QueryResult, SearchHit, Template, Tools, Vault, VaultInfo, Watcher,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
struct AppState {
    vault: Mutex<Option<Vault>>,
    watcher: Mutex<Option<Watcher>>,
    /// Whether the background thread deriving asset previews is running.
    deriving: AtomicBool,
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
    #[serde(default)]
    assets: Vec<String>,
}

/// Payload of the `assets://derived` event.
#[derive(Serialize, Clone)]
struct DerivedPayload {
    paths: Vec<String>,
    /// Whether more assets are still waiting.
    pending: bool,
}

/// Derive previews, hashes and text for pending assets on a background
/// thread, one small batch per lock so the UI stays responsive. Idempotent:
/// a second call while running does nothing.
fn start_deriver(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.deriving.swap(true, Ordering::SeqCst) {
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        loop {
            let result = match state.vault.lock() {
                Ok(mut guard) => match guard.as_mut() {
                    Some(vault) => vault.derive_assets(4),
                    None => Ok(Vec::new()),
                },
                Err(_) => Ok(Vec::new()),
            };
            match result {
                Ok(done) if !done.is_empty() => {
                    let paths = done.into_iter().map(|a| a.path).collect();
                    let _ = handle.emit("assets://derived", DerivedPayload { paths, pending: true });
                }
                Ok(_) => break,
                Err(e) => {
                    tracing::warn!("derive assets: {e}");
                    break;
                }
            }
        }
        state.deriving.store(false, Ordering::SeqCst);
        let _ = handle.emit(
            "assets://derived",
            DerivedPayload {
                paths: Vec::new(),
                pending: false,
            },
        );
    });
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
        let mut assets = Vec::new();
        if let Ok(mut guard) = state.vault.lock() {
            if let Some(vault) = guard.as_mut() {
                for p in &ev.paths {
                    if p.is_dir() {
                        let _ = vault.sync();
                        continue;
                    }
                    let is_note = p.extension().and_then(|e| e.to_str()) == Some("md");
                    match vault.reindex_path(p) {
                        Ok(Some(id)) => ids.push(id),
                        Ok(None) if !is_note => {
                            if let Ok(rel) = vault.rel(p) {
                                assets.push(rel);
                            }
                        }
                        Ok(None) => {}
                        Err(e) => tracing::warn!("reindex {}: {e}", p.display()),
                    }
                }
            }
        }
        let changed_assets = !assets.is_empty();
        let _ = handle.emit("vault://changed", ChangedPayload { ids, assets });
        if changed_assets {
            start_deriver(&handle);
        }
    })
    .map_err(err)?;
    *state.watcher.lock().map_err(err)? = Some(watcher);
    start_deriver(&app);
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
fn graph(state: State<AppState>) -> CmdResult<Graph> {
    with_vault(&state, |v| v.graph())
}

#[tauri::command]
fn local_graph(state: State<AppState>, id: String, hops: Option<usize>) -> CmdResult<Graph> {
    with_vault(&state, |v| v.local_graph(&id, hops.unwrap_or(1).clamp(1, 4)))
}

#[tauri::command]
fn follow_link(state: State<AppState>, target: String) -> CmdResult<String> {
    with_vault(&state, |v| v.resolve_or_create(&target))
}

#[tauri::command]
fn complete_title(state: State<AppState>, prefix: String) -> CmdResult<Vec<NoteSummary>> {
    with_vault(&state, |v| v.complete_title(&prefix, 12))
}

#[tauri::command]
fn items(state: State<AppState>, filter: ItemFilter) -> CmdResult<Vec<ItemRow>> {
    with_vault(&state, |v| v.items(&filter))
}

#[tauri::command]
fn progress(state: State<AppState>, project_id: String) -> CmdResult<Progress> {
    with_vault(&state, |v| v.progress(&project_id))
}

#[tauri::command]
fn set_item_status(state: State<AppState>, note_id: String, line: usize, status: String) -> CmdResult<Item> {
    let status = ItemStatus::parse(&status).ok_or_else(|| format!("unknown status '{status}'"))?;
    with_vault(&state, |v| v.set_item_status(&note_id, line, status))
}

#[tauri::command]
fn set_item_field(state: State<AppState>, note_id: String, line: usize, key: String, value: String) -> CmdResult<Item> {
    with_vault(&state, |v| v.set_item_field(&note_id, line, &key, &value))
}

#[tauri::command]
fn answer_gap(state: State<AppState>, note_id: String, line: usize, answer: String) -> CmdResult<Item> {
    with_vault(&state, |v| v.answer_gap(&note_id, line, &answer))
}

#[tauri::command]
fn add_item(state: State<AppState>, note_id: String, kind: String, text: String) -> CmdResult<Item> {
    let kind = ItemKind::parse(&kind).ok_or_else(|| format!("unknown item kind '{kind}'"))?;
    with_vault(&state, |v| v.add_item(&note_id, kind, &text))
}

#[tauri::command]
fn query(state: State<AppState>, query: String) -> CmdResult<QueryResult> {
    with_vault(&state, |v| v.query(&query))
}

#[tauri::command]
fn run_saved_query(state: State<AppState>, id: String) -> CmdResult<QueryResult> {
    with_vault(&state, |v| v.run_saved_query(&id))
}

#[tauri::command]
fn templates(state: State<AppState>) -> CmdResult<Vec<Template>> {
    with_vault(&state, |v| v.templates())
}

#[tauri::command]
fn create_from_template(
    state: State<AppState>,
    title: String,
    template: String,
    folder: Option<String>,
) -> CmdResult<Note> {
    with_vault(&state, |v| v.create_from_template(&title, &template, folder.as_deref()))
}

#[tauri::command]
fn promote_to_project(state: State<AppState>, id: String, template: Option<String>) -> CmdResult<Note> {
    with_vault(&state, |v| v.promote_to_project(&id, template.as_deref()))
}

#[tauri::command]
fn project_hub(state: State<AppState>, id: String) -> CmdResult<ProjectHub> {
    with_vault(&state, |v| v.project_hub(&id))
}

#[tauri::command]
fn rebuild_index(app: AppHandle, state: State<AppState>) -> CmdResult<usize> {
    let n = with_vault(&state, |v| v.rebuild())?;
    start_deriver(&app);
    Ok(n)
}

#[tauri::command]
fn assets(state: State<AppState>, kind: Option<String>, filter: Option<String>) -> CmdResult<Vec<Asset>> {
    with_vault(&state, |v| {
        v.assets(kind.as_deref().map(AssetKind::parse), filter.as_deref())
    })
}

#[tauri::command]
fn asset(state: State<AppState>, path: String) -> CmdResult<Asset> {
    with_vault(&state, |v| v.asset(&path))
}

#[tauri::command]
fn asset_references(state: State<AppState>, path: String) -> CmdResult<Vec<Backlink>> {
    with_vault(&state, |v| v.asset_references(&path))
}

#[tauri::command]
fn asset_text(state: State<AppState>, path: String) -> CmdResult<Option<String>> {
    with_vault(&state, |v| v.asset_text(&path))
}

#[tauri::command]
fn asset_counts(state: State<AppState>) -> CmdResult<HashMap<String, usize>> {
    with_vault(&state, |v| {
        Ok(v.asset_counts()?
            .into_iter()
            .map(|(k, n)| (k.as_str().to_string(), n))
            .collect())
    })
}

#[tauri::command]
fn search_assets(state: State<AppState>, query: String, limit: Option<usize>) -> CmdResult<Vec<AssetHit>> {
    with_vault(&state, |v| v.search_assets(&query, limit.unwrap_or(20)))
}

/// Absolute path of a file in the vault, for opening it in another app or
/// revealing it in the file manager.
#[tauri::command]
fn asset_path(state: State<AppState>, path: String) -> CmdResult<String> {
    with_vault(&state, |v| Ok(v.asset_abs_path(&path)?.to_string_lossy().into_owned()))
}

/// Re-derive one asset now (after "regenerate preview").
#[tauri::command]
fn derive_asset(state: State<AppState>, path: String) -> CmdResult<Asset> {
    with_vault(&state, |v| v.derive_asset(&path))
}

/// Kick the background deriver (e.g. after attaching files).
#[tauri::command]
fn derive_assets(app: AppHandle) {
    start_deriver(&app);
}

#[tauri::command]
fn tools() -> Tools {
    graphite_core::tools().clone()
}

/// Copy a file from anywhere on disk into the vault's attachments folder and
/// return its vault-relative path (what goes into the Markdown).
#[tauri::command]
fn attach_file(state: State<AppState>, src: String, folder: Option<String>) -> CmdResult<String> {
    with_vault(&state, |v| v.import_attachment(Path::new(&src), folder.as_deref()))
}

/// Write pasted bytes (base64) as a new attachment; returns the vault-relative path.
#[tauri::command]
fn attach_data(state: State<AppState>, name: String, data_base64: String, folder: Option<String>) -> CmdResult<String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(err)?;
    with_vault(&state, |v| v.write_attachment(&name, &bytes, folder.as_deref()))
}

#[tauri::command]
fn capture(app: AppHandle, state: State<AppState>, text: String, attachments: Vec<String>) -> CmdResult<Note> {
    let note = with_vault(&state, |v| v.capture(&text, &attachments))?;
    let _ = app.emit(
        "vault://changed",
        ChangedPayload {
            ids: vec![note.id.clone()],
            assets: Vec::new(),
        },
    );
    Ok(note)
}

#[tauri::command]
fn daily_note(state: State<AppState>) -> CmdResult<Note> {
    with_vault(&state, |v| v.daily_note(chrono::Local::now().date_naive()))
}

#[tauri::command]
fn move_note(state: State<AppState>, id: String, folder: String) -> CmdResult<Note> {
    with_vault(&state, |v| v.move_note(&id, &folder))
}

#[tauri::command]
fn set_note_type(state: State<AppState>, id: String, note_type: String) -> CmdResult<Note> {
    with_vault(&state, |v| v.set_type(&id, &note_type))
}

#[tauri::command]
fn folders(state: State<AppState>) -> CmdResult<Vec<String>> {
    with_vault(&state, |v| v.folders())
}

/// Hide the quick-capture window (Esc / after saving).
#[tauri::command]
fn hide_capture_window(app: AppHandle) -> CmdResult<()> {
    if let Some(w) = app.get_webview_window(CAPTURE_WINDOW) {
        w.hide().map_err(err)?;
    }
    Ok(())
}

/// Show or hide the quick-capture window (same as the global shortcut).
#[tauri::command]
fn toggle_capture_window(app: AppHandle) {
    toggle_capture_window_impl(&app);
}

/// Bring the main window forward and open a note there (from the capture window).
#[tauri::command]
fn open_in_main(app: AppHandle, id: String) -> CmdResult<()> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    app.emit_to("main", "note://open", id).map_err(err)
}

const CAPTURE_WINDOW: &str = "capture";
pub const CAPTURE_SHORTCUT: &str = "CmdOrCtrl+Shift+Space";

/// Show the capture window (or hide it if it is already in front).
fn toggle_capture_window_impl(app: &AppHandle) {
    let Some(w) = app.get_webview_window(CAPTURE_WINDOW) else {
        tracing::warn!("no capture window");
        return;
    };
    if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    let _ = w.center();
    let _ = w.show();
    let _ = w.set_focus();
    let _ = app.emit_to(CAPTURE_WINDOW, "capture://shown", ());
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    // The global shortcut registers at plugin setup; on platforms that refuse
    // (some Wayland sessions) the app still runs, only without the hotkey.
    let shortcuts = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                toggle_capture_window_impl(app);
            }
        })
        .with_shortcut(CAPTURE_SHORTCUT)
        .unwrap_or_else(|e| {
            tracing::warn!("capture shortcut not registered: {e}");
            tauri_plugin_global_shortcut::Builder::new()
        })
        .build();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(shortcuts)
        .manage(AppState::default())
        // The hidden capture window would otherwise keep the process alive after
        // the main window closes.
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
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
            items,
            progress,
            set_item_status,
            set_item_field,
            answer_gap,
            add_item,
            query,
            run_saved_query,
            templates,
            create_from_template,
            promote_to_project,
            project_hub,
            graph,
            local_graph,
            follow_link,
            complete_title,
            rebuild_index,
            assets,
            asset,
            asset_references,
            asset_text,
            asset_counts,
            search_assets,
            asset_path,
            derive_asset,
            derive_assets,
            tools,
            attach_file,
            attach_data,
            capture,
            daily_note,
            move_note,
            set_note_type,
            folders,
            hide_capture_window,
            toggle_capture_window,
            open_in_main,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Graphite");
}
