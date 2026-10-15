//! Tauri shell: holds the open vault and exposes core operations as
//! commands. The UI never touches the file system directly.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use graphite_core::{
    Asset, AssetHit, AssetKind, Backlink, Graph, ImportOptions, ImportReport, Item, ItemFilter, ItemKind, ItemRow,
    ItemStatus, Note, NoteSummary, Progress, ProjectHub, QueryResult, RelayConfig, SearchHit, Snapshot, SourceKind,
    SourceRow, SyncReport, SyncStatus, Template, Tools, UpdateCheck, Vault, VaultConfig, VaultInfo, Version, Watcher,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
struct AppState {
    vault: Mutex<Option<Vault>>,
    watcher: Mutex<Option<Watcher>>,
    /// Whether the background thread deriving asset previews is running.
    deriving: AtomicBool,
    /// Whether a sync pass is running (a second request waits for the next one).
    syncing: AtomicBool,
    /// Whether the periodic sync thread has been started.
    sync_timer: AtomicBool,
    /// Whether the daily snapshot thread has been started.
    snapshot_timer: AtomicBool,
    /// Whether the thread listening to the relay has been started.
    relay_listener: AtomicBool,
    /// Bumped when the relay settings change so the listener reconnects.
    relay_generation: std::sync::atomic::AtomicU64,
}

/// Take a vault snapshot if the newest is a day old, and check again every
/// hour while the app runs.
fn start_snapshot_timer(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.snapshot_timer.swap(true, Ordering::SeqCst) {
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || loop {
        // Give the open a moment to settle before a possibly long zip.
        std::thread::sleep(std::time::Duration::from_secs(30));
        let state = handle.state::<AppState>();
        let taken = state.vault.lock().ok().and_then(|guard| {
            guard
                .as_ref()
                .map(|v| v.snapshot_if_due(graphite_core::snapshot::DEFAULT_SNAPSHOT_INTERVAL))
        });
        match taken {
            Some(Ok(Some(snapshot))) => {
                let _ = handle.emit("snapshot://taken", snapshot);
            }
            Some(Err(e)) => tracing::warn!("snapshot: {e}"),
            _ => {}
        }
        std::thread::sleep(std::time::Duration::from_secs(3600));
    });
}

/// Payload of the `sync://done` event, sent after every automatic pass.
#[derive(Serialize, Clone)]
struct SyncPayload {
    report: SyncReport,
    status: SyncStatus,
}

/// One sync pass, if sync is on for the open vault. Tells the UI which
/// notes changed and how the pass went.
fn run_sync(app: &AppHandle) -> Option<SyncReport> {
    let state = app.state::<AppState>();
    if state.syncing.swap(true, Ordering::SeqCst) {
        return None;
    }
    let outcome = (|| {
        let mut guard = state.vault.lock().ok()?;
        let vault = guard.as_mut()?;
        let result = if vault.relay_config().is_some() {
            // The relay pass reconciles notes itself.
            vault.relay_sync().map(|r| {
                let mut assets: Vec<String> = r
                    .downloaded
                    .iter()
                    .chain(r.deleted.iter())
                    .filter(|p| !p.starts_with(".graphite"))
                    .cloned()
                    .collect();
                assets.sort();
                (r.sync, assets)
            })
        } else if vault.sync_enabled() {
            vault.sync_now().map(|r| (r, Vec::new()))
        } else {
            return None;
        };
        match result {
            Ok((report, assets)) => Some((report, assets, vault.sync_status().ok())),
            Err(e) => {
                tracing::warn!("sync: {e}");
                None
            }
        }
    })();
    state.syncing.store(false, Ordering::SeqCst);
    let (report, assets, status) = outcome?;
    let mut ids = report.pulled.clone();
    ids.extend(report.deleted.iter().cloned());
    if !ids.is_empty() || !assets.is_empty() {
        let _ = app.emit("vault://changed", ChangedPayload { ids, assets });
    }
    if let Some(status) = status {
        let _ = app.emit(
            "sync://done",
            SyncPayload {
                report: report.clone(),
                status,
            },
        );
    }
    Some(report)
}

/// Keep a WebSocket open to the relay (when one is configured) and run a
/// pass whenever it announces a change, so edits from other devices show
/// up within a second or two instead of at the next minute.
fn start_relay_listener(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.relay_listener.swap(true, Ordering::SeqCst) {
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        let mut backoff = 2u64;
        loop {
            let state = handle.state::<AppState>();
            let generation = state.relay_generation.load(Ordering::SeqCst);
            let target = state
                .vault
                .lock()
                .ok()
                .and_then(|g| g.as_ref().and_then(|v| v.relay_config().cloned()))
                .and_then(|c| Some((c.socket_url().ok()?, c.auth_token().ok()?)));
            let Some((url, token)) = target else {
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            };
            match listen_to_relay(&handle, &url, &token, generation) {
                Ok(()) => backoff = 2,
                Err(e) => {
                    tracing::debug!("relay socket: {e}");
                    std::thread::sleep(std::time::Duration::from_secs(backoff));
                    backoff = (backoff * 2).min(60);
                }
            }
        }
    });
}

/// One connection to the relay's socket; returns when it closes or the
/// relay settings change.
fn listen_to_relay(app: &AppHandle, url: &str, token: &str, generation: u64) -> Result<(), String> {
    use tungstenite::client::IntoClientRequest;
    let mut request = url.into_client_request().map_err(err)?;
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().map_err(err)?);
    let (rest, default_port) = url
        .strip_prefix("wss://")
        .map(|r| (r, 443))
        .or_else(|| url.strip_prefix("ws://").map(|r| (r, 80)))
        .ok_or("bad relay URL")?;
    let authority = rest.split('/').next().unwrap_or("");
    let host = if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:{default_port}")
    };
    let stream = std::net::TcpStream::connect(&host).map_err(err)?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(45)))
        .map_err(err)?;
    let (mut socket, _) = tungstenite::client_tls(request, stream).map_err(err)?;
    loop {
        let state = app.state::<AppState>();
        if state.relay_generation.load(Ordering::SeqCst) != generation {
            let _ = socket.close(None);
            return Ok(());
        }
        match socket.read() {
            Ok(tungstenite::Message::Text(_)) => {
                run_sync(app);
            }
            Ok(tungstenite::Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) =>
            {
                socket
                    .send(tungstenite::Message::Ping(Default::default()))
                    .map_err(err)?;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Sync every minute while the app runs, so edits made on other devices
/// show up even when the file watcher misses the sync client's writes.
fn start_sync_timer(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.sync_timer.swap(true, Ordering::SeqCst) {
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
        run_sync(&handle);
    });
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
    let device_id = vault.device().id.clone();
    // Let the webview load images and other attachments from this vault via the asset protocol.
    app.asset_protocol_scope().allow_directory(&root, true).map_err(err)?;
    *state.vault.lock().map_err(err)? = Some(vault);

    // External edits: reindex the touched files and tell the UI which notes
    // changed. Documents arriving from other devices trigger a sync pass.
    let handle = app.clone();
    let state_dir = root.join(graphite_core::vault::STATE_DIR);
    let own_sync_dir = state_dir.join(graphite_core::sync::SYNC_DIR).join(&device_id);
    let watcher = Watcher::new(&root, move |ev| {
        let state = handle.state::<AppState>();
        let mut ids = Vec::new();
        let mut assets = Vec::new();
        let mut sync_needed = false;
        if let Ok(mut guard) = state.vault.lock() {
            if let Some(vault) = guard.as_mut() {
                for p in &ev.paths {
                    if graphite_core::is_sync_document(&state_dir, p) {
                        sync_needed |= !p.starts_with(&own_sync_dir);
                        continue;
                    }
                    // A note changed by the user or the sync client: the
                    // next pass records or merges it.
                    sync_needed |= p.extension().and_then(|e| e.to_str()) == Some("md");
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
        if !ids.is_empty() || changed_assets {
            let _ = handle.emit("vault://changed", ChangedPayload { ids, assets });
        }
        if changed_assets {
            start_deriver(&handle);
        }
        if sync_needed {
            let h = handle.clone();
            std::thread::spawn(move || run_sync(&h));
        }
    })
    .map_err(err)?;
    *state.watcher.lock().map_err(err)? = Some(watcher);
    start_deriver(&app);
    let h = app.clone();
    std::thread::spawn(move || run_sync(&h));
    start_sync_timer(&app);
    start_snapshot_timer(&app);
    start_relay_listener(&app);
    Ok(info)
}

#[tauri::command]
fn versions(state: State<AppState>, id: String) -> CmdResult<Vec<Version>> {
    with_vault(&state, |v| v.versions(&id))
}

#[tauri::command]
fn version_note(state: State<AppState>, id: String, version: String) -> CmdResult<Note> {
    with_vault(&state, |v| v.version_note(&id, &version))
}

#[tauri::command]
fn restore_version(state: State<AppState>, id: String, version: String) -> CmdResult<Note> {
    with_vault(&state, |v| v.restore_version(&id, &version))
}

/// Zip the vault into the snapshot folder now.
#[tauri::command]
async fn snapshot_now(app: AppHandle) -> CmdResult<Snapshot> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.vault.lock().map_err(err)?;
        let vault = guard.as_ref().ok_or_else(|| "no vault open".to_string())?;
        vault.snapshot(None).map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn snapshots(state: State<AppState>) -> CmdResult<Vec<Snapshot>> {
    with_vault(&state, |v| v.snapshots(None))
}

/// Show the snapshot folder in the file manager.
#[tauri::command]
fn reveal_snapshots(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let dir = with_vault(&state, |v| {
        let d = v.snapshot_dir();
        std::fs::create_dir_all(&d)?;
        Ok(d)
    })?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(err)
}

/// Run a sync pass now (turning sync on for this vault if it was off).
#[tauri::command]
async fn sync_now(app: AppHandle) -> CmdResult<SyncReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        {
            let mut guard = state.vault.lock().map_err(err)?;
            let vault = guard.as_mut().ok_or_else(|| "no vault open".to_string())?;
            if !vault.sync_enabled() {
                // First pass: create the sync folder so later passes run.
                std::fs::create_dir_all(vault.sync_dir()).map_err(err)?;
            }
        }
        // Wait for a pass already running rather than skipping.
        loop {
            if let Some(report) = run_sync(&app) {
                return Ok(report);
            }
            if !state.syncing.load(Ordering::SeqCst) {
                return Err("sync did not run".to_string());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn sync_status(state: State<AppState>) -> CmdResult<SyncStatus> {
    with_vault(&state, |v| v.sync_status())
}

/// Connect to a relay with `config`, run a first pass to prove it works
/// (dropping the settings again if it does not), and report the new status.
fn connect_relay(app: &AppHandle, config: RelayConfig) -> CmdResult<SyncStatus> {
    let state = app.state::<AppState>();
    let mut guard = state.vault.lock().map_err(err)?;
    let vault = guard.as_mut().ok_or_else(|| "no vault open".to_string())?;
    vault.set_relay(Some(config)).map_err(err)?;
    if let Err(e) = vault.relay_sync() {
        let _ = vault.set_relay(None);
        return Err(err(e));
    }
    state.relay_generation.fetch_add(1, Ordering::SeqCst);
    let status = vault.sync_status().map_err(err)?;
    drop(guard);
    let _ = app.emit(
        "vault://changed",
        ChangedPayload {
            ids: Vec::new(),
            assets: Vec::new(),
        },
    );
    Ok(status)
}

/// Make a new key for a relay and connect with it; returns the invite.
#[tauri::command]
async fn relay_create(app: AppHandle, url: String) -> CmdResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = RelayConfig::new(&url).map_err(err)?;
        let invite = config.invite();
        connect_relay(&app, config)?;
        Ok(invite)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn relay_connect(app: AppHandle, invite: String) -> CmdResult<SyncStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = RelayConfig::from_invite(&invite).map_err(err)?;
        connect_relay(&app, config)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn relay_invite(state: State<AppState>) -> CmdResult<Option<String>> {
    with_vault(&state, |v| Ok(v.relay_config().map(|c| c.invite())))
}

#[tauri::command]
fn relay_disconnect(app: AppHandle, state: State<AppState>) -> CmdResult<SyncStatus> {
    let status = with_vault(&state, |v| {
        v.set_relay(None)?;
        v.sync_status()
    })?;
    app.state::<AppState>().relay_generation.fetch_add(1, Ordering::SeqCst);
    Ok(status)
}

#[tauri::command]
fn get_config(state: State<AppState>) -> CmdResult<VaultConfig> {
    with_vault(&state, |v| Ok(v.config()))
}

#[tauri::command]
fn set_config(state: State<AppState>, config: VaultConfig) -> CmdResult<VaultConfig> {
    with_vault(&state, |v| {
        v.save_config(&config)?;
        Ok(v.config())
    })
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

/// Vault-relative path of a rendered PDF page image (1-based).
#[tauri::command]
fn asset_page(state: State<AppState>, path: String, page: u32) -> CmdResult<String> {
    with_vault(&state, |v| v.asset_page(&path, page))
}

/// Open a file in the OS default application.
#[tauri::command]
fn open_asset(app: AppHandle, state: State<AppState>, path: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let abs = with_vault(&state, |v| v.asset_abs_path(&path))?;
    app.opener().open_path(abs.to_string_lossy(), None::<&str>).map_err(err)
}

/// Show a file in the OS file manager.
#[tauri::command]
fn reveal_asset(app: AppHandle, state: State<AppState>, path: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let abs = with_vault(&state, |v| v.asset_abs_path(&path))?;
    app.opener().reveal_item_in_dir(abs).map_err(err)
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

// ----- sources (importers) -----
// These hit the network, so they are async: Tauri runs them off the main
// thread and the window stays responsive while a clone or download runs.

/// Import a URL (GitHub repo, web page, video, PDF or file) as a source note.
#[tauri::command]
async fn import_url(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    kind: Option<String>,
    project: Option<String>,
    title: Option<String>,
) -> CmdResult<ImportReport> {
    let kind = match kind.as_deref() {
        Some(k) => Some(SourceKind::parse(k).ok_or_else(|| format!("unknown importer: {k}"))?),
        None => None,
    };
    let opts = ImportOptions {
        kind,
        project: project.filter(|p| !p.trim().is_empty()),
        title: title.filter(|t| !t.trim().is_empty()),
    };
    let report = with_vault(&state, |v| v.import_url(&url, &opts))?;
    start_deriver(&app);
    Ok(report)
}

/// Fetch a source again, replacing its files and note body.
#[tauri::command]
async fn reimport_source(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<ImportReport> {
    let report = with_vault(&state, |v| v.reimport_source(&id))?;
    start_deriver(&app);
    Ok(report)
}

/// Every source note with its record (url, kind, last check, update flag).
#[tauri::command]
fn sources(state: State<AppState>) -> CmdResult<Vec<SourceRow>> {
    with_vault(&state, |v| v.sources())
}

/// Compare one source with upstream and record the outcome on its note.
#[tauri::command]
async fn check_source(state: State<'_, AppState>, id: String) -> CmdResult<UpdateCheck> {
    with_vault(&state, |v| v.check_source(&id))
}

/// Check every source not checked within `max_age_hours` (all of them when omitted).
#[tauri::command]
async fn check_all_sources(state: State<'_, AppState>, max_age_hours: Option<u64>) -> CmdResult<Vec<UpdateCheck>> {
    let max_age = max_age_hours.map(|h| std::time::Duration::from_secs(h * 3600));
    with_vault(&state, |v| v.check_all_sources(max_age))
}

/// Which importer a URL would get.
#[tauri::command]
fn classify_url(url: String) -> String {
    graphite_core::classify_url(&url).as_str().to_string()
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

/// `android`, `ios`, `linux`, `macos` or `windows`: the UI picks the
/// phone layout from this.
#[tauri::command]
fn platform() -> &'static str {
    std::env::consts::OS
}

/// The vault a phone opens without asking: `vault/` in the app's private
/// storage, created on first use. Desktops pick a folder instead.
#[tauri::command]
fn default_vault(app: AppHandle) -> CmdResult<String> {
    let dir = app.path().app_data_dir().map_err(err)?.join("vault");
    std::fs::create_dir_all(&dir).map_err(err)?;
    Ok(dir.to_string_lossy().into_owned())
}

/// Something shared into the app from another app (Android share sheet).
#[derive(Serialize, serde::Deserialize, Clone, Default)]
struct Shared {
    #[serde(default)]
    text: String,
    #[serde(default)]
    subject: String,
    /// Absolute paths of files the share handed over, already copied into
    /// the app's storage.
    #[serde(default)]
    files: Vec<String>,
}

/// Captures waiting from the share sheet, oldest first; emptied by the
/// call. The native side appends to `shared/pending.json` in the app's
/// data directory, one object per share.
#[tauri::command]
fn take_shared(app: AppHandle) -> CmdResult<Vec<Shared>> {
    let dir = app.path().app_data_dir().map_err(err)?.join("shared");
    let file = dir.join("pending.json");
    let Ok(text) = std::fs::read_to_string(&file) else {
        return Ok(Vec::new());
    };
    let _ = std::fs::remove_file(&file);
    serde_json::from_str(&text).map_err(err)
}

/// Hide the quick-capture window (Esc / after saving).
#[cfg(desktop)]
#[tauri::command]
fn hide_capture_window(app: AppHandle) -> CmdResult<()> {
    if let Some(w) = app.get_webview_window(CAPTURE_WINDOW) {
        w.hide().map_err(err)?;
    }
    Ok(())
}

/// Show or hide the quick-capture window (same as the global shortcut).
#[cfg(desktop)]
#[tauri::command]
fn toggle_capture_window(app: AppHandle) {
    toggle_capture_window_impl(&app);
}

/// Bring the main window forward and open a note there (from the capture window).
#[cfg(desktop)]
#[tauri::command]
fn open_in_main(app: AppHandle, id: String) -> CmdResult<()> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    app.emit_to("main", "note://open", id).map_err(err)
}

#[cfg(desktop)]
const CAPTURE_WINDOW: &str = "capture";
#[cfg(desktop)]
pub const CAPTURE_SHORTCUT: &str = "CmdOrCtrl+Shift+Space";

/// Show the capture window (or hide it if it is already in front).
#[cfg(desktop)]
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let builder = tauri::Builder::default();
    // The global shortcut registers at plugin setup; on platforms that refuse
    // (some Wayland sessions) the app still runs, only without the hotkey.
    #[cfg(desktop)]
    let builder = builder.plugin(
        tauri_plugin_global_shortcut::Builder::new()
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
            .build(),
    );

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            // Phones have no home directory: keep the device id and relay
            // settings in the app's private storage instead.
            #[cfg(mobile)]
            if let Ok(dir) = app.path().app_data_dir() {
                std::env::set_var("GRAPHITE_CONFIG_DIR", dir.join("config"));
            }
            let _ = app;
            Ok(())
        })
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
            asset_page,
            open_asset,
            reveal_asset,
            derive_asset,
            derive_assets,
            tools,
            import_url,
            reimport_source,
            sources,
            check_source,
            check_all_sources,
            classify_url,
            sync_now,
            sync_status,
            relay_create,
            relay_connect,
            relay_invite,
            relay_disconnect,
            get_config,
            set_config,
            versions,
            version_note,
            restore_version,
            snapshot_now,
            snapshots,
            reveal_snapshots,
            attach_file,
            attach_data,
            capture,
            platform,
            default_vault,
            take_shared,
            daily_note,
            move_note,
            set_note_type,
            folders,
            #[cfg(desktop)]
            hide_capture_window,
            #[cfg(desktop)]
            toggle_capture_window,
            #[cfg(desktop)]
            open_in_main,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Graphite");
}
