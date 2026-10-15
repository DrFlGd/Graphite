// Typed wrappers over the Tauri commands in `src-tauri/src/lib.rs`.
// The UI never touches the file system; everything goes through here.

import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface VaultInfo {
  root: string
  name: string
  note_count: number
  asset_count: number
}

export interface NoteSummary {
  id: string
  path: string
  title: string
  type: string
  tags: string[]
  updated: string | null
}

export interface Note extends NoteSummary {
  aliases: string[]
  created: string | null
  properties: Record<string, unknown>
  body: string
}

export interface SearchHit {
  id: string
  path: string
  title: string
  snippet: string
}

export interface Backlink {
  from: NoteSummary
  relation: string | null
  kind: 'wiki' | 'embed' | 'file'
  position: number
}

export type AssetKind = 'image' | 'vector' | 'video' | 'audio' | 'pdf' | 'model' | 'code' | 'data' | 'document' | 'archive' | 'other'
export const ASSET_KINDS: AssetKind[] = ['image', 'vector', 'video', 'audio', 'pdf', 'model', 'code', 'data', 'document', 'archive', 'other']

/** A file in the vault as the index knows it (see core `assets.rs`). */
export interface Asset {
  /** Vault-relative path; the asset's identity. */
  path: string
  name: string
  ext: string
  kind: AssetKind
  size: number
  modified: string
  hash: string | null
  width: number | null
  height: number | null
  duration: number | null
  pages: number | null
  meta: Record<string, unknown>
  /** Vault-relative path of extracted text under .graphite/derived. */
  text: string | null
  /** Vault-relative path of a JPEG thumbnail under .graphite/derived. */
  thumbnail: string | null
  derived: 'pending' | 'done'
}

export interface AssetHit {
  path: string
  name: string
  kind: AssetKind
  snippet: string
}

export type SourceKind = 'github' | 'git' | 'video' | 'page' | 'pdf' | 'file'

export const SOURCE_KIND_LABEL: Record<SourceKind, string> = {
  github: 'GitHub repository',
  git: 'Git repository',
  video: 'Video',
  page: 'Web page',
  pdf: 'PDF',
  file: 'File',
}

/** A `type: source` note with its record fields (flattened NoteSummary + record). */
export interface SourceRow extends NoteSummary {
  url: string
  kind: SourceKind
  fetched: string | null
  last_checked: string | null
  update_available: boolean
  update_summary: string | null
  folder: string | null
  project: string | null
}

export interface ImportReport {
  note: Note
  kind: SourceKind
  folder: string
  files: string[]
  summary: string
}

export interface UpdateCheck {
  note_id: string
  title: string
  kind: SourceKind
  changed: boolean
  summary: string
  checked: string
}

export interface Tools {
  ffmpeg: boolean
  ffprobe: boolean
  pdfinfo: boolean
  pdftotext: boolean
  pdftoppm: boolean
  git: boolean
  yt_dlp: boolean
  browser: boolean
}

export interface Outgoing {
  target: string
  id: string | null
}

export interface GraphNode {
  id: string
  title: string
  type: string
  path: string
  /** A link target no note resolves to; opening it creates the note. */
  missing: boolean
}

export interface GraphEdge {
  from: string
  to: string
  relation: string | null
  kind: 'wiki' | 'embed'
  count: number
}

export interface Graph {
  nodes: GraphNode[]
  edges: GraphEdge[]
}

export type ItemKind = 'task' | 'gap'
export type ItemStatus = 'open' | 'doing' | 'done' | 'cancelled' | 'answered'

/** A `- [ ]` task or `- [?]` gap line in a note. */
export interface Item {
  note_id: string
  /** 0-based line in the note body. */
  line: number
  kind: ItemKind
  status: ItemStatus
  text: string
  raw: string
  depth: number
  parent: number | null
  heading: string | null
  due: string | null
  priority: number | null
  tags: string[]
  /** Raw link target of `project:: [[X]]` on the item or an ancestor. */
  project: string | null
  answer: string | null
  fields: Record<string, string>
}

export interface ItemRow extends Item {
  note_title: string
  note_path: string
  project_id: string | null
  project_title: string | null
}

export interface ItemFilter {
  kind?: ItemKind | null
  open_only?: boolean
  note?: string | null
  project?: string | null
  due_before?: string | null
  text?: string | null
}

export interface NoteRow extends NoteSummary {
  created: string | null
  properties: Record<string, unknown>
}

export interface QueryGroup {
  key: string
  count: number
}

/** Result of a query; rows are sorted and, when grouped, ordered group by group. */
export interface QueryResult {
  domain: 'notes' | 'items'
  notes: NoteRow[]
  items: ItemRow[]
  groups: QueryGroup[]
  total: number
}

export interface Template {
  name: string
  description: string
  note_type: string
  folder: string
  properties: Record<string, unknown>
  body: string
  builtin: boolean
}

export interface LinkedNote extends NoteSummary {
  relation: string | null
  incoming: boolean
}

/** Result of one sync pass over the shared folder. */
export interface SyncReport {
  pushed: number
  pulled: string[]
  deleted: string[]
  conflicts: number
  pending: number
  devices: number
}

export interface SyncDevice {
  id: string
  name: string
  last_sync: string | null
  notes: number
  this_device: boolean
}

export interface SyncStatus {
  enabled: boolean
  device: { id: string; name: string }
  devices: SyncDevice[]
  last_sync: string | null
}

export interface ProjectHub {
  project: NoteSummary
  progress: Progress
  tasks: ItemRow[]
  gaps: ItemRow[]
  decisions: NoteSummary[]
  sources: NoteSummary[]
  linked: LinkedNote[]
}

export interface Progress {
  tasks: number
  done: number
  doing: number
  open: number
  cancelled: number
  gaps: number
  gaps_open: number
  percent: number
}

export const api = {
  openVault: (path: string) => invoke<VaultInfo>('open_vault', { path }),
  vaultInfo: () => invoke<VaultInfo | null>('vault_info'),
  listNotes: () => invoke<NoteSummary[]>('list_notes'),
  readNote: (id: string) => invoke<Note>('read_note', { id }),
  saveNote: (note: Note) => invoke<Note>('save_note', { note }),
  createNote: (title: string, noteType?: string, folder?: string) => invoke<Note>('create_note', { title, noteType, folder }),
  deleteNote: (id: string) => invoke<void>('delete_note', { id }),
  search: (query: string, limit = 30) => invoke<SearchHit[]>('search', { query, limit }),
  backlinks: (id: string) => invoke<Backlink[]>('backlinks', { id }),
  unlinkedMentions: (id: string) => invoke<NoteSummary[]>('unlinked_mentions', { id }),
  outgoingLinks: (id: string) => invoke<Outgoing[]>('outgoing_links', { id }),
  followLink: (target: string) => invoke<string>('follow_link', { target }),
  graph: () => invoke<Graph>('graph'),
  localGraph: (id: string, hops = 1) => invoke<Graph>('local_graph', { id, hops }),
  items: (filter: ItemFilter) => invoke<ItemRow[]>('items', { filter }),
  progress: (projectId: string) => invoke<Progress>('progress', { projectId }),
  setItemStatus: (noteId: string, line: number, status: ItemStatus) => invoke<Item>('set_item_status', { noteId, line, status }),
  setItemField: (noteId: string, line: number, key: string, value: string) => invoke<Item>('set_item_field', { noteId, line, key, value }),
  answerGap: (noteId: string, line: number, answer: string) => invoke<Item>('answer_gap', { noteId, line, answer }),
  addItem: (noteId: string, kind: ItemKind, text: string) => invoke<Item>('add_item', { noteId, kind, text }),
  query: (query: string) => invoke<QueryResult>('query', { query }),
  runSavedQuery: (id: string) => invoke<QueryResult>('run_saved_query', { id }),
  templates: () => invoke<Template[]>('templates'),
  createFromTemplate: (title: string, template: string, folder?: string) =>
    invoke<Note>('create_from_template', { title, template, folder }),
  promoteToProject: (id: string, template?: string) => invoke<Note>('promote_to_project', { id, template }),
  projectHub: (id: string) => invoke<ProjectHub>('project_hub', { id }),
  completeTitle: (prefix: string) => invoke<NoteSummary[]>('complete_title', { prefix }),
  rebuildIndex: () => invoke<number>('rebuild_index'),
  assets: (kind?: AssetKind, filter?: string) => invoke<Asset[]>('assets', { kind, filter }),
  asset: (path: string) => invoke<Asset>('asset', { path }),
  assetReferences: (path: string) => invoke<Backlink[]>('asset_references', { path }),
  assetText: (path: string) => invoke<string | null>('asset_text', { path }),
  assetCounts: () => invoke<Record<string, number>>('asset_counts'),
  searchAssets: (query: string, limit = 20) => invoke<AssetHit[]>('search_assets', { query, limit }),
  /** Absolute path on disk, for opening in another app or revealing in the file manager. */
  assetPath: (path: string) => invoke<string>('asset_path', { path }),
  /** Vault-relative path of a rendered PDF page image (1-based); needs poppler. */
  assetPage: (path: string, page: number) => invoke<string>('asset_page', { path, page }),
  openAsset: (path: string) => invoke<void>('open_asset', { path }),
  revealAsset: (path: string) => invoke<void>('reveal_asset', { path }),
  deriveAsset: (path: string) => invoke<Asset>('derive_asset', { path }),
  deriveAssets: () => invoke<void>('derive_assets'),
  tools: () => invoke<Tools>('tools'),
  /** Import a URL as a source note; `kind` overrides the importer guessed from the URL. */
  importUrl: (url: string, opts: { kind?: SourceKind; project?: string; title?: string } = {}) =>
    invoke<ImportReport>('import_url', { url, kind: opts.kind ?? null, project: opts.project ?? null, title: opts.title ?? null }),
  reimportSource: (id: string) => invoke<ImportReport>('reimport_source', { id }),
  sources: () => invoke<SourceRow[]>('sources'),
  checkSource: (id: string) => invoke<UpdateCheck>('check_source', { id }),
  /** Check sources not checked in the last `maxAgeHours` (all when omitted). */
  checkAllSources: (maxAgeHours?: number) => invoke<UpdateCheck[]>('check_all_sources', { maxAgeHours: maxAgeHours ?? null }),
  classifyUrl: (url: string) => invoke<SourceKind>('classify_url', { url }),
  /** Merge with the other devices now; turns sync on for the vault the first time. */
  syncNow: () => invoke<SyncReport>('sync_now'),
  syncStatus: () => invoke<SyncStatus>('sync_status'),
  /** Fires after every automatic sync pass. */
  onSyncDone: (cb: (report: SyncReport, status: SyncStatus) => void): Promise<UnlistenFn> =>
    listen<{ report: SyncReport; status: SyncStatus }>('sync://done', (e) => cb(e.payload.report, e.payload.status)),
  /** Fires as previews and text are derived in the background; `pending` false when the queue is empty. */
  onAssetsDerived: (cb: (paths: string[], pending: boolean) => void): Promise<UnlistenFn> =>
    listen<{ paths: string[]; pending: boolean }>('assets://derived', (e) => cb(e.payload.paths, e.payload.pending)),
  attachFile: (src: string, folder?: string) => invoke<string>('attach_file', { src, folder }),
  attachData: (name: string, dataBase64: string, folder?: string) => invoke<string>('attach_data', { name, dataBase64, folder }),
  capture: (text: string, attachments: string[]) => invoke<Note>('capture', { text, attachments }),
  dailyNote: () => invoke<Note>('daily_note'),
  moveNote: (id: string, folder: string) => invoke<Note>('move_note', { id, folder }),
  setNoteType: (id: string, noteType: string) => invoke<Note>('set_note_type', { id, noteType }),
  folders: () => invoke<string[]>('folders'),
  hideCaptureWindow: () => invoke<void>('hide_capture_window'),
  toggleCaptureWindow: () => invoke<void>('toggle_capture_window'),
  openInMain: (id: string) => invoke<void>('open_in_main', { id }),
  /** The capture window was shown by the global shortcut. */
  onCaptureShown: (cb: () => void): Promise<UnlistenFn> => listen('capture://shown', () => cb()),
  /** Another window asked the main window to open a note. */
  onOpenNote: (cb: (id: string) => void): Promise<UnlistenFn> => listen<string>('note://open', (e) => cb(e.payload)),
  /** Subscribe to notes (ids) and files (vault-relative paths) changed on disk by something other than this app. */
  onVaultChanged: (cb: (ids: string[], assets: string[]) => void): Promise<UnlistenFn> =>
    listen<{ ids: string[]; assets?: string[] }>('vault://changed', (e) => cb(e.payload.ids, e.payload.assets ?? [])),
}

/** Read a File (clipboard or drop) as base64 for `attachData`. */
export function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader()
    r.onerror = () => reject(r.error)
    r.onload = () => resolve(String(r.result).split(',')[1] ?? '')
    r.readAsDataURL(file)
  })
}

export const INBOX_DIR = 'notes/Inbox'
export const IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'avif']
export const isImagePath = (p: string) => IMAGE_EXTS.includes(p.split('.').pop()?.toLowerCase() ?? '')

/** Undo the percent-encoding Markdown links need for spaces and parentheses. */
export function decodePath(src: string): string {
  try {
    return decodeURIComponent(src)
  } catch {
    return src
  }
}

/** Turn a vault-relative src into a URL the webview may load. */
export function assetUrl(vaultRoot: string, src: string): string {
  if (/^(https?:|data:|blob:|asset:)/i.test(src)) return src
  const sep = vaultRoot.includes('\\') ? '\\' : '/'
  const rel = decodePath(src.split(/[?#]/)[0]).replace(/^\.?\//, '').split('/').join(sep)
  return convertFileSrc(`${vaultRoot}${sep}${rel}`)
}
