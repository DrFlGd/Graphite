import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ArrowLeft, CheckSquare, Inbox, ListChecks, Pencil, RefreshCw, Search, Settings2, SquareKanban } from 'lucide-react'
import { openUrl } from '@tauri-apps/plugin-opener'
import { NoteEditor, type NoteCandidate } from '@graphite/ui'
import { api, assetUrl, type Note, type NoteSummary, type SearchHit, type SyncStatus, type VaultInfo } from './api'
import { InboxView } from './components/InboxView'
import { TasksView } from './components/TasksView'
import { ProjectHub } from './components/ProjectHub'
import { SyncDialog } from './components/SyncDialog'
import { syncSummary } from './components/sync'
import { snippetText } from './snippet'

type Tab = 'inbox' | 'notes' | 'tasks' | 'projects'

/**
 * The phone layout: one screen at a time, a bar of tabs at the bottom.
 * Capture (typed, or shared in from another app), inbox triage, reading
 * and light editing, search, ticking tasks, and a project's hub. The vault
 * lives in the app's private storage and reaches other devices through
 * a relay (see the sync panel).
 */
export function MobileApp() {
  const [vault, setVault] = useState<VaultInfo | null>(null)
  const [notes, setNotes] = useState<NoteSummary[]>([])
  const [version, setVersion] = useState(0)
  const [tab, setTab] = useState<Tab>('inbox')
  const [open, setOpen] = useState<Note | null>(null)
  const [editing, setEditing] = useState(false)
  const [hub, setHub] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [syncStatus, setSyncStatus] = useState<SyncStatus | null>(null)
  const [syncOpen, setSyncOpen] = useState(false)
  const [syncing, setSyncing] = useState(false)
  const [capture, setCapture] = useState('')
  const [query, setQuery] = useState('')
  const [hits, setHits] = useState<SearchHit[] | null>(null)
  const saveTimer = useRef<number | null>(null)
  const pendingBody = useRef<string | null>(null)

  const refreshList = useCallback(async () => {
    try {
      setNotes(await api.listNotes())
      setVersion((v) => v + 1)
    } catch (e) {
      setError(String(e))
    }
  }, [])

  /** File what other apps shared into Graphite since the last look. */
  const takeShared = useCallback(async () => {
    try {
      const shared = await api.takeShared()
      if (shared.length === 0) return
      for (const s of shared) {
        const attachments: string[] = []
        for (const f of s.files) {
          try {
            attachments.push(await api.attachFile(f, 'attachments/Inbox'))
          } catch (e) {
            setError(String(e))
          }
        }
        const text = [s.subject, s.text].filter((t) => t.trim()).join('\n')
        if (!text.trim() && attachments.length === 0) continue
        await api.capture(text || s.files.map((f) => f.split('/').pop()).join(', '), attachments)
      }
      setNotice(shared.length === 1 ? 'Saved to the inbox' : `${shared.length} items saved to the inbox`)
      setTab('inbox')
      await refreshList()
    } catch (e) {
      setError(String(e))
    }
  }, [refreshList])

  useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        const path = await api.defaultVault()
        const info = await api.openVault(path)
        if (cancelled) return
        setVault(info)
        setNotes(await api.listNotes())
        setSyncStatus(await api.syncStatus())
        await takeShared()
      } catch (e) {
        if (!cancelled) setError(String(e))
      }
    })()
    return () => {
      cancelled = true
    }
  }, [takeShared])

  useEffect(() => {
    if (!vault) return
    const onVisible = () => {
      if (document.visibilityState === 'visible') void takeShared()
    }
    document.addEventListener('visibilitychange', onVisible)
    const un1 = api.onVaultChanged((ids) => {
      void refreshList()
      if (open && ids.includes(open.id) && !editing) {
        void api.readNote(open.id).then(setOpen).catch(() => {})
      }
    })
    const un2 = api.onSyncDone((_report, status) => setSyncStatus(status))
    return () => {
      document.removeEventListener('visibilitychange', onVisible)
      void un1.then((f) => f())
      void un2.then((f) => f())
    }
  }, [vault, takeShared, refreshList, open, editing])

  useEffect(() => {
    if (!notice) return
    const t = window.setTimeout(() => setNotice(null), 3000)
    return () => window.clearTimeout(t)
  }, [notice])

  // Search as the user types on the Notes tab.
  useEffect(() => {
    const q = query.trim()
    if (!q) {
      setHits(null)
      return
    }
    let cancelled = false
    const t = window.setTimeout(() => {
      void api
        .search(q, 40)
        .then((h) => {
          if (!cancelled) setHits(h)
        })
        .catch((e) => setError(String(e)))
    }, 150)
    return () => {
      cancelled = true
      window.clearTimeout(t)
    }
  }, [query, version])

  const openNote = useCallback(
    async (id: string) => {
      try {
        const n = await api.readNote(id)
        setHub(n.type === 'project' ? id : null)
        setOpen(n)
        setEditing(false)
      } catch (e) {
        setError(String(e))
      }
    },
    [],
  )

  const flushSave = useCallback(async () => {
    if (saveTimer.current) {
      window.clearTimeout(saveTimer.current)
      saveTimer.current = null
    }
    if (open && pendingBody.current !== null) {
      const body = pendingBody.current
      pendingBody.current = null
      try {
        const saved = await api.saveNote({ ...open, body })
        setOpen((o) => (o && o.id === saved.id ? { ...saved, body: pendingBody.current ?? saved.body } : o))
        setVersion((v) => v + 1)
      } catch (e) {
        setError(String(e))
      }
    }
  }, [open])

  const onBodyChange = useCallback(
    (markdown: string) => {
      if (!open || markdown === open.body) return
      pendingBody.current = markdown
      if (saveTimer.current) window.clearTimeout(saveTimer.current)
      saveTimer.current = window.setTimeout(() => void flushSave(), 800)
    },
    [open, flushSave],
  )

  const closeNote = useCallback(async () => {
    await flushSave()
    setOpen(null)
    setHub(null)
    setEditing(false)
    await refreshList()
  }, [flushSave, refreshList])

  const doCapture = useCallback(async () => {
    const text = capture.trim()
    if (!text) return
    try {
      await api.capture(text, [])
      setCapture('')
      setNotice('Saved to the inbox')
      await refreshList()
    } catch (e) {
      setError(String(e))
    }
  }, [capture, refreshList])

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

  const searchNotes = useCallback(async (q: string): Promise<NoteCandidate[]> => {
    const all = await api.completeTitle(q)
    return all.map((n) => ({ id: n.id, title: n.title }))
  }, [])

  const resolveSrc = useCallback((src: string) => (vault ? assetUrl(vault.root, src) : src), [vault])

  const followLink = useCallback(
    async (target: string) => {
      try {
        const id = await api.followLink(target)
        await refreshList()
        await openNote(id)
      } catch (e) {
        setError(String(e))
      }
    },
    [openNote, refreshList],
  )

  const recent = useMemo(() => [...notes].sort((a, b) => (b.updated ?? '').localeCompare(a.updated ?? '')).slice(0, 50), [notes])
  const projects = useMemo(() => notes.filter((n) => n.type === 'project').sort((a, b) => a.title.localeCompare(b.title)), [notes])

  if (!vault) {
    return (
      <div className="mobile mobile--empty">
        <p>{error ?? 'Opening your vault…'}</p>
      </div>
    )
  }

  let screen: React.ReactNode
  if (open) {
    screen = (
      <div className="mobile__note">
        <header className="mobile__bar">
          <button type="button" className="mobile__icon" onClick={() => void closeNote()} aria-label="Back">
            <ArrowLeft size={20} />
          </button>
          <span className="mobile__title">{open.title}</span>
          {hub && !editing ? (
            <button type="button" className="mobile__icon" onClick={() => setHub(null)} aria-label="Show the note text">
              <Pencil size={18} />
            </button>
          ) : (
            <button type="button" className={`mobile__icon ${editing ? 'is-active' : ''}`} onClick={() => (editing ? void flushSave().then(() => setEditing(false)) : setEditing(true))} aria-label={editing ? 'Done editing' : 'Edit'}>
              {editing ? <CheckSquare size={18} /> : <Pencil size={18} />}
            </button>
          )}
        </header>
        {hub && !editing ? (
          <div className="mobile__screen">
            <ProjectHub noteId={hub} version={version} vaultRoot={vault.root} assetVersion={version} onOpenFile={() => setNotice('Open files on a desktop')} onFilesChanged={() => void refreshList()} onImport={() => setNotice('Import on a desktop')} onOpen={(id) => void openNote(id)} onFollowLink={(t) => void followLink(t)} onOpenUrl={(href) => void openUrl(href).catch((e) => setError(String(e)))} onChanged={() => void refreshList()} onError={setError} onEdit={() => setHub(null)} />
          </div>
        ) : (
          <div className="mobile__screen mobile__editor">
            <NoteEditor markdown={open.body} docKey={open.id} title={open.title} onChange={onBodyChange} editable={editing} toolbar={editing} resolveSrc={resolveSrc} onFollowLink={(t) => void followLink(t)} onOpenUrl={(href) => void openUrl(href).catch((e) => setError(String(e)))} searchNotes={searchNotes} />
          </div>
        )}
      </div>
    )
  } else if (tab === 'inbox') {
    screen = (
      <div className="mobile__screen">
        <div className="mobile__capture">
          <textarea value={capture} onChange={(e) => setCapture(e.target.value)} placeholder="Capture a thought, a link, a task…" rows={3} aria-label="Quick capture" />
          <button type="button" className="btn btn--primary" onClick={() => void doCapture()} disabled={!capture.trim()}>
            Save to inbox
          </button>
        </div>
        <InboxView notes={notes} onOpen={(id) => void openNote(id)} onImport={(_url, captureId) => void openNote(captureId)} onChanged={() => void refreshList()} onError={setError} />
      </div>
    )
  } else if (tab === 'notes') {
    const rows: { id: string; title: string; sub: string }[] = hits ? hits.map((h) => ({ id: h.id, title: h.title, sub: snippetText(h.snippet) })) : recent.map((n) => ({ id: n.id, title: n.title, sub: n.path }))
    screen = (
      <div className="mobile__screen">
        <div className="mobile__search">
          <Search size={16} />
          <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search notes" aria-label="Search notes" />
        </div>
        <ul className="mobile__list">
          {rows.map((r) => (
            <li key={r.id}>
              <button type="button" onClick={() => void openNote(r.id)}>
                <span className="mobile__row-title">{r.title}</span>
                <span className="mobile__row-sub">{r.sub}</span>
              </button>
            </li>
          ))}
          {rows.length === 0 && <li className="mobile__none">{hits ? 'Nothing matches.' : 'No notes yet.'}</li>}
        </ul>
      </div>
    )
  } else if (tab === 'tasks') {
    screen = (
      <div className="mobile__screen">
        <TasksView notes={notes} version={version} onOpen={(id) => void openNote(id)} onFollowLink={(t) => void followLink(t)} onChanged={() => void refreshList()} onError={setError} onSaveQuery={() => setNotice('Save queries on a desktop')} compact />
      </div>
    )
  } else {
    screen = (
      <div className="mobile__screen">
        <ul className="mobile__list">
          {projects.map((p) => (
            <li key={p.id}>
              <button type="button" onClick={() => void openNote(p.id)}>
                <span className="mobile__row-title">{p.title}</span>
                <span className="mobile__row-sub">{p.tags.join(', ')}</span>
              </button>
            </li>
          ))}
          {projects.length === 0 && <li className="mobile__none">No projects yet. Promote a note to a project on a desktop.</li>}
        </ul>
      </div>
    )
  }

  return (
    <div className="mobile">
      {!open && (
        <header className="mobile__bar">
          <span className="mobile__title">{tab === 'inbox' ? 'Inbox' : tab === 'notes' ? 'Notes' : tab === 'tasks' ? 'Tasks' : 'Projects'}</span>
          <button type="button" className={`mobile__icon ${syncing ? 'is-spinning' : ''}`} onClick={() => void syncNow()} disabled={syncing} aria-label="Sync now" title={syncSummary(syncStatus)}>
            <RefreshCw size={18} />
          </button>
          <button type="button" className="mobile__icon" onClick={() => setSyncOpen(true)} aria-label="Sync settings">
            <Settings2 size={18} />
          </button>
        </header>
      )}
      {screen}
      {!open && (
        <nav className="mobile__tabs" aria-label="Sections">
          <button type="button" className={tab === 'inbox' ? 'is-active' : ''} onClick={() => setTab('inbox')}>
            <Inbox size={20} />
            <span>Inbox</span>
          </button>
          <button type="button" className={tab === 'notes' ? 'is-active' : ''} onClick={() => setTab('notes')}>
            <Search size={20} />
            <span>Notes</span>
          </button>
          <button type="button" className={tab === 'tasks' ? 'is-active' : ''} onClick={() => setTab('tasks')}>
            <ListChecks size={20} />
            <span>Tasks</span>
          </button>
          <button type="button" className={tab === 'projects' ? 'is-active' : ''} onClick={() => setTab('projects')}>
            <SquareKanban size={20} />
            <span>Projects</span>
          </button>
        </nav>
      )}
      {syncOpen && <SyncDialog status={syncStatus} onStatus={setSyncStatus} onClose={() => setSyncOpen(false)} onError={setError} />}
      {error && (
        <div className="app__error" role="alert" onClick={() => setError(null)}>
          {error}
        </div>
      )}
      {notice && !error && (
        <div className="app__notice" role="status" onClick={() => setNotice(null)}>
          {notice}
        </div>
      )}
    </div>
  )
}
