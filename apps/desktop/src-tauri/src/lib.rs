//! Tauri shell: holds the open vault and exposes core operations as
//! commands. The UI never touches the file system directly.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use graphite_core::{
    Backlink, Graph, Item, ItemFilter, ItemKind, ItemRow, ItemStatus, Note, NoteSummary, Progress, ProjectHub,
    QueryResult, SearchHit, Template, Vault, VaultInfo, Watcher,
};
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
fn rebuild_index(state: State<AppState>) -> CmdResult<usize> {
    with_vault(&state, |v| v.rebuild())
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
