import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { ArrowUpRight, CalendarDays, Columns2, Download, FilePlus2, FolderOpen, Inbox, LayoutTemplate, ListChecks, Locate, Paperclip, RefreshCw, Search, SquareKanban, Trash2, Waypoints, X, Zap } from 'lucide-react'
import { insertAttachments, type AttachmentItem, type Editor } from '@graphite/ui'
import { api, assetUrl, decodePath, INBOX_DIR, type Note, type NoteSummary, type SyncStatus, type VaultInfo } from './api'
import { Sidebar } from './components/Sidebar'
import { NoteTab } from './components/NoteTab'
import { Pane } from './components/Pane'
import { InboxView } from './components/InboxView'
import { TasksView } from './components/TasksView'
import { SavedQueryTab } from './components/SavedQueryTab'
import { ProjectHub } from './components/ProjectHub'
import { TemplatePicker } from './components/TemplatePicker'
import type { ViewKind } from './components/QueryView'
import { ContextPanel } from './components/ContextPanel'
import { AssetView } from './components/AssetView'
import { FilesView } from './components/FilesView'
import { SourcesView } from './components/SourcesView'
import { ImportDialog, type ImportRequest } from './components/ImportDialog'
import { GraphView, type GraphMode } from './components/GraphView'
import { Palette, type Command, type PaletteMode } from './components/Palette'
import { Welcome } from './components/Welcome'
import * as L from './layout'

const LAST_VAULT_KEY = 'graphite.lastVault'
const layoutKey = (root: string) => `graphite.layout:${root}`

export function App() {
  const [vault, setVault] = useState<VaultInfo | null>(null)
  const [notes, setNotes] = useState<NoteSummary[]>([])
  const [layout, setLayout] = useState<L.Layout>(() => L.emptyLayout())
  /** The note shown in the active pane's active tab, as its editor has it. */
  const [activeNote, setActiveNote] = useState<Note | null>(null)
  /** The last note that was active anywhere; the graph centres on it. */
  const [lastNote, setLastNote] = useState<Note | null>(null)
  const [graphMode, setGraphMode] = useState<GraphMode>('global')
  const [palette, setPalette] = useState<{ mode: PaletteMode; newTab: boolean } | null>(null)
  const [error, setError] = useState<string | null>(null)
  /** Bumped when notes change so views that derive from the index refetch. */
  const [version, setVersion] = useState(0)
  /** Bumped when files or their previews change on disk. */
  const [assetVersion, setAssetVersion] = useState(0)
  const [changed, setChanged] = useState<{ ids: string[]; seq: number }>({ ids: [], seq: 0 })
  const [saved, setSaved] = useState<Note | null>(null)
  /** Open tasks across the vault, for the sidebar badge. */
  const [openTasks, setOpenTasks] = useState(0)
  /** Query and project notes the user chose to see as text rather than as a view. */
  const [editingNotes, setEditingNotes] = useState<Set<string>>(() => new Set())
  const [picker, setPicker] = useState<{ kind: 'create' } | { kind: 'promote'; noteId: string; title: string } | null>(null)
  /** The import dialog, when open. */
  const [importing, setImporting] = useState<ImportRequest | null>(null)
  /** Sources flagged as changed upstream, for the sidebar badge. */
  const [sourceUpdates, setSourceUpdates] = useState(0)
  const [syncStatus, setSyncStatus] = useState<SyncStatus | null>(null)
  const [syncing, setSyncing] = useState(false)
  /** Front-matter edits from the context panel, for the note tab to save. */
  const [patch, setPatch] = useState<{ id: string; patch: Partial<Note>; seq: number } | null>(null)
  const activeEditor = useRef<Editor | null>(null)
  const layoutRef = useRef(layout)
  layoutRef.current = layout

  const refreshList = useCallback(async () => {
    try {
      const list = await api.listNotes()
      setNotes(list)
      setVersion((v) => v + 1)
      return list
    } catch (e) {
      setError(String(e))
      return null
    }
  }, [])

  const openVault = useCallback(
    async (path: string) => {
      try {
        const info = await api.openVault(path)
        setVault(info)
        try {
          localStorage.setItem(LAST_VAULT_KEY, path)
        } catch {
          /* private mode */
        }
        setActiveNote(null)
        setLastNote(null)
        const list = await refreshList()
        let restored: L.Layout | null = null
        try {
          restored = L.parseLayout(JSON.parse(localStorage.getItem(layoutKey(info.root)) ?? 'null'))
        } catch {
          /* ignore */
        }
        const ids = new Set((list ?? []).map((n) => n.id))
        setLayout(restored ? L.dropNotes(restored, (id) => ids.has(id)) : L.emptyLayout())
        setError(null)
      } catch (e) {
        setError(String(e))
      }
    },
    [refreshList],
  )

  // Reopen the last vault on launch.
  useEffect(() => {
    let last: string | null = null
    try {
      last = localStorage.getItem(LAST_VAULT_KEY)
    } catch {
      /* ignore */
    }
    if (last) void openVault(last)
  }, [openVault])

  // Remember the tabs per vault.
  useEffect(() => {
    if (!vault) return
    try {
      localStorage.setItem(layoutKey(vault.root), JSON.stringify(layout))
    } catch {
      /* ignore */
    }
  }, [layout, vault])

  // When the active tab is not a note, nothing is "the active note".
  const active = L.activeTab(layout)
  useEffect(() => {
    if (active.kind !== 'note') {
      setActiveNote(null)
      activeEditor.current = null
    }
  }, [active])

  const openNote = useCallback((id: string, newTab = false) => {
    setLayout((l) => L.open(l, { kind: 'note', noteId: id }, newTab))
  }, [])

  const openInbox = useCallback((newTab = false) => setLayout((l) => L.open(l, { kind: 'inbox' }, newTab)), [])
  const openFiles = useCallback((newTab = false) => setLayout((l) => L.open(l, { kind: 'files' }, newTab)), [])
  const openSources = useCallback((newTab = false) => setLayout((l) => L.open(l, { kind: 'sources' }, newTab)), [])
  /** Show a file's own view; `path` may carry `#page=N` from a PDF link. */
  const openAsset = useCallback((src: string, newTab = false) => {
    const [path, hash] = src.split('#')
    const page = hash ? Number(/page=(\d+)/.exec(hash)?.[1]) || undefined : undefined
    setLayout((l) => L.open(l, { kind: 'asset', path: decodePath(path.replace(/^\.\//, '')), page }, newTab))
  }, [])
  const openExternal = useCallback((href: string) => {
    openUrl(href).catch((e) => setError(String(e)))
  }, [])
  const openTasksView = useCallback((newTab = false) => setLayout((l) => L.open(l, { kind: 'tasks' }, newTab)), [])

  // Keep the open-task count fresh whenever the index changes.
  useEffect(() => {
    if (!vault) return
    let cancelled = false
    void api
      .items({ kind: 'task', open_only: true })
      .then((rows) => {
        if (!cancelled) setOpenTasks(rows.length)
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [vault, version])

  // How many sources changed upstream, whenever notes change.
  useEffect(() => {
    if (!vault) return
    let cancelled = false
    void api
      .sources()
      .then((rows) => {
        if (!cancelled) setSourceUpdates(rows.filter((r) => r.update_available).length)
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [vault, version])

  // Sync state: fetched on open and refreshed after every automatic pass.
  useEffect(() => {
    if (!vault) return
    let cancelled = false
    void api
      .syncStatus()
      .then((s) => {
        if (!cancelled) setSyncStatus(s)
      })
      .catch(() => {})
    const un = api.onSyncDone((_report, status) => {
      if (!cancelled) setSyncStatus(status)
    })
    return () => {
      cancelled = true
      void un.then((f) => f())
    }
  }, [vault])

  const syncNow = useCallback(async () => {
    setSyncing(true)
    try {
      await api.syncNow()
      setSyncStatus(await api.syncStatus())
      await refreshList()
    } catch (e) {
      setError(String(e))
    } finally {
      setSyncing(false)
    }
  }, [refreshList])

  // The scheduled update check: once per vault open, sources not checked in the last day.
  useEffect(() => {
    if (!vault) return
    let cancelled = false
    const t = window.setTimeout(() => {
      void api
        .checkAllSources(24)
        .then((checks) => {
          if (!cancelled && checks.length > 0) void refreshList()
        })
        .catch(() => {})
    }, 4000)
    return () => {
      cancelled = true
      window.clearTimeout(t)
    }
  }, [vault, refreshList])

  const showGraph = useCallback((mode: GraphMode, newTab = false) => {
    setGraphMode(mode)
    setLayout((l) => L.open(l, { kind: 'graph' }, newTab))
  }, [])

  const openToday = useCallback(async () => {
    try {
      const note = await api.dailyNote()
      await refreshList()
      openNote(note.id)
    } catch (e) {
      setError(String(e))
    }
  }, [openNote, refreshList])

  const createNote = useCallback(
    async (title: string, noteType?: string) => {
      try {
        const note = await api.createNote(title, noteType)
        await refreshList()
        openNote(note.id)
        return note
      } catch (e) {
        setError(String(e))
        return null
      }
    },
    [openNote, refreshList],
  )

  const editAsNote = useCallback((id: string) => setEditingNotes((set) => new Set(set).add(id)), [])
  const showAsView = useCallback(
    (id: string) =>
      setEditingNotes((set) => {
        const next = new Set(set)
        next.delete(id)
        return next
      }),
    [],
  )

  /** Turn a query into a `type: query` note and open it. */
  const saveQuery = useCallback(
    async (query: string, view: ViewKind) => {
      const title = window.prompt('Name for this saved query', 'Saved query')
      if (!title?.trim()) return
      try {
        const note = await api.createFromTemplate(title.trim(), 'query')
        await api.saveNote({ ...note, properties: { ...note.properties, query, view } })
        await refreshList()
        openNote(note.id)
      } catch (e) {
        setError(String(e))
      }
    },
    [openNote, refreshList],
  )

  const followLink = useCallback(
    async (target: string) => {
      try {
        const id = await api.followLink(target)
        await refreshList()
        openNote(id)
      } catch (e) {
        setError(String(e))
      }
    },
    [openNote, refreshList],
  )

  const deleteNote = useCallback(
    async (note: Note) => {
      if (!window.confirm(`Delete "${note.title}"? The file is removed from the vault.`)) return
      try {
        await api.deleteNote(note.id)
        setLayout((l) => L.dropNotes(l, (id) => id !== note.id))
        await refreshList()
      } catch (e) {
        setError(String(e))
      }
    },
    [refreshList],
  )

  // Notes changed on disk by something else (or the capture window).
  useEffect(() => {
    if (!vault) return
    let unlisten: (() => void) | undefined
    void api
      .onVaultChanged((ids, assets) => {
        if (assets.length > 0) setAssetVersion((v) => v + 1)
        if (ids.length === 0 && assets.length > 0) return
        setChanged((c) => ({ ids, seq: c.seq + 1 }))
        void refreshList().then((list) => {
          if (!list) return
          const have = new Set(list.map((n) => n.id))
          setLayout((l) => L.dropNotes(l, (id) => have.has(id)))
        })
      })
      .then((u) => {
        unlisten = u
      })
    return () => unlisten?.()
  }, [vault, refreshList])

  // Previews and extracted text arrive in the background after files change.
  useEffect(() => {
    if (!vault) return
    let unlisten: (() => void) | undefined
    void api
      .onAssetsDerived((paths, pending) => {
        if (paths.length > 0 || !pending) setAssetVersion((v) => v + 1)
        if (!pending) void api.vaultInfo().then((info) => info && setVault(info)).catch(() => {})
      })
      .then((u) => {
        unlisten = u
      })
    return () => unlisten?.()
  }, [vault])

  // The capture window asked us to show a note it just saved.
  useEffect(() => {
    if (!vault) return
    let unlisten: (() => void) | undefined
    void api.onOpenNote((id) => openNote(id)).then((u) => {
      unlisten = u
    })
    return () => unlisten?.()
  }, [vault, openNote])

  // Files dropped onto the window go into the active note's editor.
  useEffect(() => {
    if (!vault) return
    let unlisten: (() => void) | undefined
    void getCurrentWebview()
      .onDragDropEvent(async (e) => {
        if (e.payload.type !== 'drop') return
        const editor = activeEditor.current
        if (!editor || editor.isDestroyed) return
        const items: AttachmentItem[] = []
        for (const p of e.payload.paths) {
          try {
            const src = await api.attachFile(p)
            items.push({ src, name: p.split(/[\\/]/).pop() ?? src })
          } catch (err) {
            setError(String(err))
          }
        }
        const scale = window.devicePixelRatio || 1
        const pos = editor.view.posAtCoords({ left: e.payload.position.x / scale, top: e.payload.position.y / scale })?.pos
        insertAttachments(editor, items, pos)
        void api.deriveAssets()
        setAssetVersion((v) => v + 1)
      })
      .then((u) => {
        unlisten = u
      })
    return () => unlisten?.()
  }, [vault])

  const pickVault = useCallback(async () => {
    const dir = await openDialog({ directory: true, multiple: false, title: 'Open or create a vault folder' })
    if (typeof dir === 'string') await openVault(dir)
  }, [openVault])

  const pickImage = useCallback(async () => {
    const file = await openDialog({ multiple: false, directory: false, title: 'Insert image', filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'] }] })
    if (typeof file !== 'string') return null
    try {
      return await api.attachFile(file)
    } catch (e) {
      setError(String(e))
      return null
    }
  }, [])

  const pickFile = useCallback(async () => {
    const file = await openDialog({ multiple: false, directory: false, title: 'Attach a file' })
    if (typeof file !== 'string') return null
    try {
      const src = await api.attachFile(file)
      void api.deriveAssets()
      setAssetVersion((v) => v + 1)
      return { src, name: file.split(/[\\/]/).pop() ?? src }
    } catch (e) {
      setError(String(e))
      return null
    }
  }, [])

  const resolveSrc = useMemo(() => (vault ? (src: string) => assetUrl(vault.root, src) : undefined), [vault])

  // Keyboard shortcuts for the whole window.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey
      if (!mod || e.altKey) return
      const key = e.key.toLowerCase()
      const act = (f: () => void) => {
        e.preventDefault()
        f()
      }
      if (e.shiftKey) {
        if (key === 'p') act(() => setPalette({ mode: 'commands', newTab: false }))
        else if (key === 'g') act(() => showGraph(lastNote && active.kind !== 'graph' ? 'local' : 'global'))
        else if (key === 't') act(() => openTasksView())
        else if (key === 'f') act(() => openFiles())
        else if (key === 'i') act(() => setImporting({}))
        else if (key === 'n') act(() => setPicker({ kind: 'create' }))
        else if (key === 'tab') act(() => setLayout((l) => L.cycleTab(l, -1)))
        return
      }
      if (key === 'k' || key === 'p') act(() => setPalette({ mode: 'notes', newTab: false }))
      else if (key === 't') act(() => setPalette({ mode: 'notes', newTab: true }))
      else if (key === 'w') act(() => setLayout(L.closeActiveTab))
      else if (key === 'tab') act(() => setLayout((l) => L.cycleTab(l, 1)))
      else if (key === '\\') act(() => setLayout(L.splitRight))
      else if (key >= '1' && key <= '3') act(() => setLayout((l) => L.focusPane(l, Number(key) - 1)))
      else if (key === 'n') act(() => void createNote(`Untitled ${new Date().toLocaleString()}`))
      else if (key === 'd') act(() => void openToday())
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [createNote, openToday, showGraph, openTasksView, openFiles, lastNote, active.kind])

  const commands = useMemo<Command[]>(
    () => [
      { id: 'search', label: 'Search notes', shortcut: 'Ctrl+K', icon: <Search size={16} />, run: () => setPalette({ mode: 'notes', newTab: false }) },
      { id: 'new', label: 'New note', shortcut: 'Ctrl+N', icon: <FilePlus2 size={16} />, run: () => void createNote(`Untitled ${new Date().toLocaleString()}`) },
      { id: 'new-template', label: 'New note from template…', shortcut: 'Ctrl+Shift+N', icon: <LayoutTemplate size={16} />, keywords: 'project research build software writing decision', run: () => setPicker({ kind: 'create' }) },
      ...(activeNote && activeNote.type !== 'project' ? [{ id: 'promote', label: `Promote “${activeNote.title}” to a project…`, icon: <ArrowUpRight size={16} />, keywords: 'idea convert template', run: () => setPicker({ kind: 'promote', noteId: activeNote.id, title: activeNote.title }) }] : []),
      ...(activeNote && (activeNote.type === 'project' || activeNote.type === 'query') ? [{ id: 'as-view', label: activeNote.type === 'project' ? 'Show project hub' : 'Show query results', icon: <SquareKanban size={16} />, run: () => showAsView(activeNote.id) }] : []),
      { id: 'today', label: "Open today's daily note", shortcut: 'Ctrl+D', icon: <CalendarDays size={16} />, keywords: 'daily journal', run: () => void openToday() },
      { id: 'inbox', label: 'Show inbox', icon: <Inbox size={16} />, keywords: 'triage captures', run: () => openInbox() },
      { id: 'tasks', label: 'Show tasks', shortcut: 'Ctrl+Shift+T', icon: <ListChecks size={16} />, keywords: 'todo tracker gaps questions board', run: () => openTasksView() },
      { id: 'files', label: 'Show files', shortcut: 'Ctrl+Shift+F', icon: <Paperclip size={16} />, keywords: 'attachments images video pdf models gallery assets', run: () => openFiles() },
      { id: 'import', label: 'Import from URL…', shortcut: 'Ctrl+Shift+I', icon: <Download size={16} />, keywords: 'github repo clone web page article youtube video pdf source fetch', run: () => setImporting({}) },
      { id: 'sources', label: 'Show sources', icon: <Download size={16} />, keywords: 'imports repos pages videos updates', run: () => openSources() },
      { id: 'sync', label: syncStatus?.enabled ? 'Sync now' : 'Turn on sync', icon: <RefreshCw size={16} />, keywords: 'devices merge shared folder', run: () => void syncNow() },
      ...(activeNote ? [{ id: 'attach', label: 'Attach a file to this note…', icon: <Paperclip size={16} />, keywords: 'upload insert embed', run: () => void pickFile().then((f) => f && activeEditor.current && insertAttachments(activeEditor.current, [f])) }] : []),
      { id: 'query', label: 'New saved query', icon: <SquareKanban size={16} />, keywords: 'view list table board filter', run: () => void saveQuery('type:task status:open sort:due', 'list') },
      { id: 'graph', label: 'Show graph of the whole vault', shortcut: 'Ctrl+Shift+G', icon: <Waypoints size={16} />, keywords: 'links network', run: () => showGraph('global') },
      ...(lastNote ? [{ id: 'graph-local', label: `Show graph around “${lastNote.title}”`, icon: <Locate size={16} />, keywords: 'local links', run: () => showGraph('local') }] : []),
      { id: 'tab-new', label: 'Open a note in a new tab', shortcut: 'Ctrl+T', keywords: 'tab', run: () => setPalette({ mode: 'notes', newTab: true }) },
      { id: 'tab-close', label: 'Close tab', shortcut: 'Ctrl+W', icon: <X size={16} />, run: () => setLayout(L.closeActiveTab) },
      { id: 'split', label: 'Split pane to the right', shortcut: 'Ctrl+\\', icon: <Columns2 size={16} />, keywords: 'side by side columns', run: () => setLayout(L.splitRight) },
      { id: 'capture', label: 'Quick capture', shortcut: 'Ctrl+Shift+Space', icon: <Zap size={16} />, keywords: 'inbox thought', run: () => void api.toggleCaptureWindow() },
      ...(activeNote ? [{ id: 'delete', label: `Delete “${activeNote.title}”`, icon: <Trash2 size={16} />, run: () => void deleteNote(activeNote) }] : []),
      { id: 'vault', label: 'Open another vault folder', icon: <FolderOpen size={16} />, keywords: 'switch', run: () => void pickVault() },
      { id: 'reindex', label: 'Rebuild the search index', icon: <RefreshCw size={16} />, keywords: 'reindex repair', run: () => void api.rebuildIndex().then(refreshList).catch((e) => setError(String(e))) },
    ],
    [createNote, openToday, openInbox, openTasksView, openFiles, openSources, pickFile, saveQuery, showAsView, showGraph, lastNote, activeNote, deleteNote, pickVault, refreshList, syncNow, syncStatus],
  )

  if (!vault) return <Welcome onPick={pickVault} error={error} />

  const inboxCount = notes.filter((n) => n.path.startsWith(INBOX_DIR + '/')).length
  const showContext = active.kind === 'note' && activeNote !== null

  const body = (tab: L.Tab, isActive: boolean) => {
    switch (tab.kind) {
      case 'note': {
        const type = notes.find((n) => n.id === tab.noteId)?.type
        if (type === 'query' && !editingNotes.has(tab.noteId)) {
          return <SavedQueryTab key={tab.id} noteId={tab.noteId} version={version} onOpen={openNote} onChanged={() => void refreshList()} onError={setError} onEdit={() => editAsNote(tab.noteId)} />
        }
        if (type === 'project' && !editingNotes.has(tab.noteId)) {
          return (
            <ProjectHub
              key={tab.id}
              noteId={tab.noteId}
              version={version}
              vaultRoot={vault.root}
              assetVersion={assetVersion}
              onOpenFile={openAsset}
              onFilesChanged={() => setAssetVersion((v) => v + 1)}
              onImport={(project) => setImporting({ project })}
              onOpen={openNote}
              onChanged={() => void refreshList()}
              onError={setError}
              onEdit={() => editAsNote(tab.noteId)}
            />
          )
        }
        return (
          <NoteTab
            key={tab.id}
            noteId={tab.noteId}
            active={isActive}
            changed={changed}
            saved={saved}
            patch={patch}
            onActiveNote={(n) => {
              setActiveNote(n)
              if (n) setLastNote(n)
            }}
            onEditorReady={(ed) => {
              activeEditor.current = ed
            }}
            onSaved={(n) => {
              setSaved(n)
              void refreshList()
            }}
            onFollowLink={(t) => void followLink(t)}
            onOpenFile={(src) => openAsset(src)}
            onOpenUrl={openExternal}
            onDelete={(n) => void deleteNote(n)}
            onError={setError}
            resolveSrc={resolveSrc}
            pickImage={pickImage}
            pickFile={pickFile}
          />
        )
      }
      case 'inbox':
        return <InboxView key={tab.id} notes={notes} onOpen={(id) => openNote(id)} onImport={(url, captureId) => setImporting({ url, captureId })} onChanged={() => void refreshList()} onError={setError} />
      case 'tasks':
        return <TasksView key={tab.id} notes={notes} version={version} onOpen={openNote} onChanged={() => void refreshList()} onError={setError} onSaveQuery={(q, v) => void saveQuery(q, v)} />
      case 'files':
        return <FilesView key={tab.id} vaultRoot={vault.root} version={assetVersion} onOpen={openAsset} onChanged={() => setAssetVersion((v) => v + 1)} onError={setError} />
      case 'sources':
        return <SourcesView key={tab.id} version={version} onOpen={openNote} onImport={() => setImporting({})} onChanged={() => void refreshList()} onError={setError} />
      case 'asset':
        return <AssetView key={tab.id} path={tab.path} page={tab.page} vaultRoot={vault.root} version={assetVersion} onOpenNote={openNote} onNoteCreated={(id) => void refreshList().then(() => openNote(id))} onError={setError} />
      case 'graph':
        return <GraphView key={tab.id} current={lastNote} mode={graphMode} onModeChange={setGraphMode} version={version} onOpen={(id) => openNote(id)} onCreate={(t) => void followLink(t)} onError={setError} />
      default:
        return (
          <div key={tab.id} className="pane__empty">
            <p>
              Pick a note on the left, press <kbd>Ctrl</kbd>+<kbd>N</kbd> for a new one, <kbd>Ctrl</kbd>+<kbd>D</kbd> for today, <kbd>Ctrl</kbd>+<kbd>K</kbd> to search, or <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd> for every command.
              <br />
              <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Space</kbd> anywhere captures a thought to the inbox.
            </p>
          </div>
        )
    }
  }

  return (
    <div className={`app ${showContext ? '' : 'app--no-context'}`}>
      <Sidebar
        vault={vault}
        notes={notes}
        currentId={active.kind === 'note' ? active.noteId : null}
        onOpen={openNote}
        onCreate={(t) => void createNote(t)}
        onCreateFromTemplate={() => setPicker({ kind: 'create' })}
        onPickVault={pickVault}
        onSearch={() => setPalette({ mode: 'notes', newTab: false })}
        inboxCount={inboxCount}
        inboxOpen={active.kind === 'inbox'}
        graphOpen={active.kind === 'graph'}
        tasksOpen={active.kind === 'tasks'}
        filesOpen={active.kind === 'files' || active.kind === 'asset'}
        sourcesOpen={active.kind === 'sources'}
        openTasks={openTasks}
        sourceUpdates={sourceUpdates}
        sync={syncStatus}
        syncing={syncing}
        onSync={() => void syncNow()}
        onInbox={() => openInbox()}
        onToday={() => void openToday()}
        onGraph={() => showGraph(graphMode)}
        onTasks={() => openTasksView()}
        onFiles={() => openFiles()}
        onSources={() => openSources()}
      />
      <main className="panes">
        {layout.panes.map((pane, pi) => {
          const paneActive = pi === layout.active
          return (
            <Pane
              key={pane.id}
              pane={pane}
              active={paneActive}
              notes={notes}
              canSplit={layout.panes.length < L.MAX_PANES}
              onFocus={() => setLayout((l) => L.focusPane(l, pi))}
              onSelect={(ti) => setLayout((l) => L.selectTab(l, pi, ti))}
              onClose={(ti) => setLayout((l) => L.closeTab(l, pi, ti))}
              onNewTab={() => setPalette({ mode: 'notes', newTab: true })}
              onSplit={() => setLayout((l) => L.splitRight(L.focusPane(l, pi)))}
            >
              {body(pane.tabs[pane.active], paneActive)}
            </Pane>
          )
        })}
      </main>
      {showContext && (
        <ContextPanel
          note={activeNote}
          onOpen={(id) => openNote(id)}
          onFollowLink={(t) => void followLink(t)}
          onShowGraph={() => showGraph('local', true)}
          onEdit={(p) => setPatch((prev) => ({ id: activeNote.id, patch: p, seq: (prev?.seq ?? 0) + 1 }))}
        />
      )}
      {picker && (
        <TemplatePicker
          mode={picker}
          onClose={() => setPicker(null)}
          onError={setError}
          onDone={(id) => {
            setPicker(null)
            showAsView(id)
            void refreshList().then(() => openNote(id))
          }}
        />
      )}
      {palette && (
        <Palette
          mode={palette.mode}
          commands={commands}
          onClose={() => setPalette(null)}
          onOpen={(id, newTab) => openNote(id, newTab || palette.newTab)}
          onOpenFile={(path, newTab) => openAsset(path, newTab || palette.newTab)}
          onCreate={(t) => void createNote(t)}
          onImport={(url) => setImporting({ url })}
        />
      )}
      {importing && (
        <ImportDialog
          request={importing}
          notes={notes}
          onClose={() => setImporting(null)}
          onError={setError}
          onDone={(report) => {
            setImporting(null)
            setAssetVersion((v) => v + 1)
            void refreshList().then(() => openNote(report.note.id))
          }}
        />
      )}
      {error && (
        <div className="app__error" role="alert" onClick={() => setError(null)}>
          {error}
        </div>
      )}
    </div>
  )
}
