//! Background jobs: imports, re-imports and video tool downloads. Each runs
//! on a worker thread and the window lists them with their progress, so the
//! user keeps working while they run. Only the first and last steps of an
//! import hold the vault (see `Vault::begin_import`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use graphite_core::{ImportControl, ImportOptions, PendingImport, SourceKind, CANCELLED};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{err, start_deriver, with_vault, AppState, CmdResult};

/// What the import window asked for; kept on the job so it can be retried.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub url: String,
    pub kind: Option<String>,
    pub project: Option<String>,
    pub title: Option<String>,
    /// Download the video tools first (missing ones, and the newest yt-dlp).
    #[serde(default)]
    pub with_tools: bool,
    /// An inbox capture this import replaces, deleted once it succeeded.
    pub capture_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: u64,
    /// `import`, `reimport` or `tools`.
    pub kind: &'static str,
    /// What the list shows: the URL, the note title, or "Video tools".
    pub label: String,
    /// Which importer runs, for an import or re-import.
    pub source: Option<SourceKind>,
    pub stage: String,
    /// Bytes so far and in all, when the current stage counts them.
    pub done: u64,
    pub total: Option<u64>,
    pub state: JobState,
    pub error: Option<String>,
    /// The source note, once there is one.
    pub note_id: Option<String>,
    pub summary: Option<String>,
    /// For an import: what to send again on retry.
    pub request: Option<ImportRequest>,
    /// For a re-import: the note to fetch again on retry.
    pub source_id: Option<String>,
}

struct Entry {
    job: Job,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct Jobs {
    next: AtomicU64,
    list: Mutex<Vec<Entry>>,
}

impl Jobs {
    fn snapshot(&self) -> Vec<Job> {
        self.list
            .lock()
            .map(|l| l.iter().map(|e| e.job.clone()).collect())
            .unwrap_or_default()
    }

    fn add(&self, kind: &'static str, label: String) -> (u64, Arc<AtomicBool>) {
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let job = Job {
            id,
            kind,
            label,
            source: None,
            stage: "Starting".into(),
            done: 0,
            total: None,
            state: JobState::Running,
            error: None,
            note_id: None,
            summary: None,
            request: None,
            source_id: None,
        };
        if let Ok(mut l) = self.list.lock() {
            l.push(Entry {
                job,
                cancel: cancel.clone(),
            });
        }
        (id, cancel)
    }

    /// Cancel every running job and wait (up to `limit`) for them to clean up.
    pub fn stop_all(&self, limit: Duration) {
        let running = |l: &Vec<Entry>| l.iter().any(|e| e.job.state == JobState::Running);
        if let Ok(l) = self.list.lock() {
            for e in l.iter() {
                e.cancel.store(true, Ordering::SeqCst);
            }
        }
        let start = Instant::now();
        while start.elapsed() < limit && self.list.lock().map(|l| running(&l)).unwrap_or(false) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn update(&self, id: u64, f: impl FnOnce(&mut Job)) {
        if let Ok(mut l) = self.list.lock() {
            if let Some(e) = l.iter_mut().find(|e| e.job.id == id) {
                f(&mut e.job);
            }
        }
    }
}

fn emit(app: &AppHandle) {
    let jobs = app.state::<AppState>().jobs.snapshot();
    let _ = app.emit("jobs://changed", jobs);
}

/// Reports a job's progress to the window, at most a few times a second.
struct Control {
    app: AppHandle,
    id: u64,
    cancel: Arc<AtomicBool>,
    last: Mutex<(Instant, String)>,
}

impl ImportControl for Control {
    fn progress(&self, stage: &str, done: u64, total: Option<u64>) {
        let stage = if stage.is_empty() { "Downloading" } else { stage };
        self.app.state::<AppState>().jobs.update(self.id, |j| {
            j.stage = stage.to_string();
            j.done = done;
            j.total = total;
        });
        let Ok(mut last) = self.last.lock() else { return };
        // A new stage shows at once; byte counts only every quarter second.
        if last.1 != stage || last.0.elapsed() >= Duration::from_millis(250) {
            *last = (Instant::now(), stage.to_string());
            drop(last);
            emit(&self.app);
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// Record how a job ended and tell the window.
fn finish(app: &AppHandle, id: u64, outcome: Result<(Option<String>, String), String>) {
    app.state::<AppState>().jobs.update(id, |j| match outcome {
        Ok((note_id, summary)) => {
            j.state = JobState::Done;
            j.stage = summary.clone();
            j.summary = Some(summary);
            if note_id.is_some() {
                j.note_id = note_id;
            }
        }
        Err(e) if e == CANCELLED => {
            j.state = JobState::Cancelled;
            j.stage = "Cancelled".into();
        }
        Err(e) => {
            j.state = JobState::Failed;
            j.stage = "Failed".into();
            j.error = Some(e);
        }
    });
    emit(app);
}

/// Run a started import's fetch on a worker thread, then record it.
fn spawn_import(
    app: AppHandle,
    id: u64,
    cancel: Arc<AtomicBool>,
    mut pending: PendingImport,
    with_tools: bool,
    capture_id: Option<String>,
) {
    tauri::async_runtime::spawn_blocking(move || {
        let ctl = Control {
            app: app.clone(),
            id,
            cancel,
            last: Mutex::new((Instant::now(), String::new())),
        };
        let fetched = if with_tools {
            graphite_core::install_video_tools(&ctl).and_then(|_| pending.fetch_with(&ctl))
        } else {
            pending.fetch_with(&ctl)
        };
        ctl.progress("Saving the note", 0, None);
        let state = app.state::<AppState>();
        let outcome = with_vault(&state, |v| {
            let report = v.finish_import(pending, fetched)?;
            if let Some(capture) = &capture_id {
                // The capture held only the link; the source note replaces it.
                let _ = v.delete(capture);
            }
            Ok((Some(report.note.id), report.summary))
        });
        if outcome.is_ok() {
            start_deriver(&app);
        }
        finish(&app, id, outcome);
    });
}

fn parse_kind(kind: Option<&str>) -> CmdResult<Option<SourceKind>> {
    match kind {
        Some(k) => SourceKind::parse(k)
            .map(Some)
            .ok_or_else(|| format!("unknown importer: {k}")),
        None => Ok(None),
    }
}

/// Start importing a URL in the background. Errors in the request itself
/// (not a URL) come back at once; everything after shows on the job.
#[tauri::command]
pub fn start_import(app: AppHandle, state: State<AppState>, request: ImportRequest) -> CmdResult<u64> {
    let opts = ImportOptions {
        kind: parse_kind(request.kind.as_deref())?,
        project: request.project.clone().filter(|p| !p.trim().is_empty()),
        title: request.title.clone().filter(|t| !t.trim().is_empty()),
    };
    let pending = with_vault(&state, |v| v.begin_import(&request.url, &opts))?;
    let (id, cancel) = state.jobs.add("import", request.url.trim().to_string());
    let (with_tools, capture_id) = (request.with_tools, request.capture_id.clone());
    let source = pending.kind();
    state.jobs.update(id, |j| {
        j.source = Some(source);
        j.request = Some(request);
    });
    emit(&app);
    spawn_import(app, id, cancel, pending, with_tools, capture_id);
    Ok(id)
}

/// Fetch a source again in the background.
#[tauri::command]
pub fn start_reimport(app: AppHandle, state: State<AppState>, id: String, with_tools: Option<bool>) -> CmdResult<u64> {
    let pending = with_vault(&state, |v| v.begin_reimport(&id))?;
    let (job, cancel) = state.jobs.add("reimport", pending.title().to_string());
    let source = pending.kind();
    state.jobs.update(job, |j| {
        j.source = Some(source);
        j.note_id = Some(id.clone());
        j.source_id = Some(id);
    });
    emit(&app);
    spawn_import(app, job, cancel, pending, with_tools.unwrap_or(false), None);
    Ok(job)
}

/// Download the video tools in the background.
#[tauri::command]
pub fn start_install_video_tools(app: AppHandle, state: State<AppState>) -> u64 {
    let (id, cancel) = state.jobs.add("tools", "Video tools".into());
    emit(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let ctl = Control {
            app: app.clone(),
            id,
            cancel,
            last: Mutex::new((Instant::now(), String::new())),
        };
        let outcome = graphite_core::install_video_tools(&ctl)
            .map(|_| (None, "Video tools are ready".to_string()))
            .map_err(err);
        finish(&app, id, outcome);
    });
    id
}

#[tauri::command]
pub fn jobs(state: State<AppState>) -> Vec<Job> {
    state.jobs.snapshot()
}

/// Ask a running job to stop; it ends as cancelled and leaves nothing behind.
#[tauri::command]
pub fn cancel_job(state: State<AppState>, id: u64) {
    if let Ok(l) = state.jobs.list.lock() {
        if let Some(e) = l.iter().find(|e| e.job.id == id) {
            e.cancel.store(true, Ordering::SeqCst);
        }
    }
}

/// Remove a finished job from the list.
#[tauri::command]
pub fn dismiss_job(app: AppHandle, state: State<AppState>, id: u64) {
    if let Ok(mut l) = state.jobs.list.lock() {
        l.retain(|e| e.job.id != id || e.job.state == JobState::Running);
    }
    emit(&app);
}
