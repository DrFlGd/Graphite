import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { CalendarDays, FilePlus2, FolderOpen, Inbox, Locate, RefreshCw, Search, Trash2, Waypoints, Zap } from 'lucide-react'
import { insertAttachments, type AttachmentItem, type Editor } from '@graphite/ui'
import { api, assetUrl, fileToBase64, INBOX_DIR, type Note, type NoteSummary, type VaultInfo } from './api'
import { Sidebar } from './components/Sidebar'
import { NotePane } from './components/NotePane'
import { InboxView } from './components/InboxView'
import { ContextPanel } from './components/ContextPanel'
import { GraphView, type GraphMode } from './components/GraphView'
import { Palette, type Command, type PaletteMode } from './components/Palette'
import { Welcome } from './components/Welcome'

const LAST_VAULT_KEY = 'graphite.lastVault'
const SAVE_DELAY_MS = 400

export function App() {
  const [vault, setVault] = useState<VaultInfo | null>(null)
  const [notes, setNotes] = useState<NoteSummary[]>([])
  const [current, setCurrent] = useState<Note | null>(null)
  const [palette, setPalette] = useState<PaletteMode | null>(null)
  const [view, setView] = useState<'note' | 'inbox' | 'graph'>('note')
  const [graphMode, setGraphMode] = useState<GraphMode>('global')
  /** Bumped when notes change so views that derive from the index refetch. */
  const [version, setVersion] = useState(0)
  const [error, setError] = useState<string | null>(null)
  const [saveState, setSaveState] = useState<'saved' | 'dirty' | 'saving'>('saved')
  const pending = useRef<{ timer: number | null; note: Note | null }>({ timer: null, note: null })
  /** The live editor of the open note, for inserting dropped/pasted files. */
  const editorRef = useRef<Editor | null>(null)

  const refreshList = useCallback(async () => {
    try {
      setNotes(await api.listNotes())
      setVersion((v) => v + 1)
    } catch (e) {
      setError(String(e))
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
        setCurrent(null)
        await refreshList()
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

  const flushSave = useCallback(async () => {
    const p = pending.current
    if (p.timer) {
      window.clearTimeout(p.timer)
      p.timer = null
    }
    const note = p.note
    p.note = null
    if (!note) return
    setSaveState('saving')
    try {
      const saved = await api.saveNote(note)
      setCurrent((c) => (c && c.id === saved.id && !pending.current.note ? { ...c, updated: saved.updated, created: saved.created } : c))
      setSaveState(pending.current.note ? 'dirty' : 'saved')
      void refreshList()
    } catch (e) {
      setError(String(e))
      setSaveState('dirty')
    }
  }, [refreshList])

  const scheduleSave = useCallback(
    (note: Note) => {
      pending.current.note = note
      setSaveState('dirty')
      if (pending.current.timer) window.clearTimeout(pending.current.timer)
      pending.current.timer = window.setTimeout(() => void flushSave(), SAVE_DELAY_MS)
    },
    [flushSave],
  )

  const openNote = useCallback(
    async (id: string) => {
      await flushSave()
      try {
        const note = await api.readNote(id)
        setCurrent(note)
        setView('note')
      } catch (e) {
        setError(String(e))
      }
    },
    [flushSave],
  )

  const openInbox = useCallback(async () => {
    await flushSave()
    setView('inbox')
  }, [flushSave])

  const showGraph = useCallback(
    async (mode: GraphMode) => {
      await flushSave()
      setGraphMode(mode)
      setView('graph')
    },
    [flushSave],
  )

  const openToday = useCallback(async () => {
    await flushSave()
    try {
      const note = await api.dailyNote()
      await refreshList()
      setCurrent(note)
      setView('note')
    } catch (e) {
      setError(String(e))
    }
  }, [flushSave, refreshList])

  // The capture window asked us to show a note it just saved.
  useEffect(() => {
    if (!vault) return
    let unlisten: (() => void) | undefined
    void api.onOpenNote((id) => void openNote(id)).then((u) => {
      unlisten = u
    })
    return () => unlisten?.()
  }, [vault, openNote])

  /** Copy files into the vault and put them into the open note. */
  const addFiles = useCallback(async (items: AttachmentItem[], pos?: number) => {
    const editor = editorRef.current
    if (!editor || editor.isDestroyed || items.length === 0) return
    insertAttachments(editor, items, pos)
  }, [])

  // Files dropped onto the window (Tauri hands us paths, not File objects).
  useEffect(() => {
    if (!vault) return
    let unlisten: (() => void) | undefined
    void getCurrentWebview()
      .onDragDropEvent(async (e) => {
        if (e.payload.type !== 'drop') return
        const editor = editorRef.current
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
        await addFiles(items, pos)
      })
      .then((u) => {
        unlisten = u
      })
    return () => unlisten?.()
  }, [vault, addFiles])

  // Files pasted into the editor arrive as File objects: store their bytes.
  const onPastedFiles = useCallback(
    async (files: File[], pos?: number) => {
      const items: AttachmentItem[] = []
      for (const f of files) {
        const ext = f.type.split('/')[1] || 'bin'
        const name = !f.name || f.name === 'image.png' ? `pasted-${new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19)}.${ext}` : f.name
        try {
          const src = await api.attachData(name, await fileToBase64(f))
          items.push({ src, name })
        } catch (err) {
          setError(String(err))
        }
      }
      await addFiles(items, pos)
    },
    [addFiles],
  )

  // Notes changed on disk by something else: refresh the list and reload the open note.
  useEffect(() => {
    if (!vault) return
    let unlisten: (() => void) | undefined
    void api.onVaultChanged((ids) => {
      void refreshList()
      const cur = current?.id
      if (cur && ids.includes(cur) && !pending.current.note) {
        void api.readNote(cur).then(setCurrent).catch(() => {})
      }
    }).then((u) => {
      unlisten = u
    })
    return () => unlisten?.()
  }, [vault, current?.id, refreshList])

  const pickVault = useCallback(async () => {
    const dir = await openDialog({ directory: true, multiple: false, title: 'Open or create a vault folder' })
    if (typeof dir === 'string') await openVault(dir)
  }, [openVault])

  const createNote = useCallback(
    async (title: string, noteType?: string) => {
      try {
        const note = await api.createNote(title, noteType)
        await refreshList()
        setCurrent(note)
        return note
      } catch (e) {
        setError(String(e))
        return null
      }
    },
    [refreshList],
  )

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

  const deleteCurrent = useCallback(async () => {
    if (!current) return
    if (!window.confirm(`Delete "${current.title}"? The file is removed from the vault.`)) return
    pending.current.note = null
    try {
      await api.deleteNote(current.id)
      setCurrent(null)
      await refreshList()
    } catch (e) {
      setError(String(e))
    }
  }, [current, refreshList])

  // Keyboard: Ctrl/Cmd+K search, Ctrl/Cmd+Shift+P commands, Ctrl/Cmd+N new note,
  // Ctrl/Cmd+D today, Ctrl/Cmd+Shift+G graph, Ctrl/Cmd+S flush.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey
      if (!mod || e.altKey) return
      const key = e.key.toLowerCase()
      if (e.shiftKey && key === 'p') {
        e.preventDefault()
        setPalette('commands')
      } else if (e.shiftKey && key === 'g') {
        e.preventDefault()
        void showGraph(current && view !== 'graph' ? 'local' : 'global')
      } else if (e.shiftKey) {
        return
      } else if (key === 'k' || key === 'p') {
        e.preventDefault()
        setPalette('notes')
      } else if (key === 'n') {
        e.preventDefault()
        void createNote(`Untitled ${new Date().toLocaleString()}`)
      } else if (key === 'd') {
        e.preventDefault()
        void openToday()
      } else if (key === 's') {
        e.preventDefault()
        void flushSave()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [createNote, flushSave, openToday, showGraph, current, view])

  const resolveSrc = useMemo(() => (vault ? (src: string) => assetUrl(vault.root, src) : undefined), [vault])

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

  const commands = useMemo<Command[]>(
    () => [
      { id: 'search', label: 'Search notes', shortcut: 'Ctrl+K', icon: <Search size={16} />, run: () => setPalette('notes') },
      { id: 'new', label: 'New note', shortcut: 'Ctrl+N', icon: <FilePlus2 size={16} />, run: () => void createNote(`Untitled ${new Date().toLocaleString()}`) },
      { id: 'today', label: "Open today's daily note", shortcut: 'Ctrl+D', icon: <CalendarDays size={16} />, keywords: 'daily journal', run: () => void openToday() },
      { id: 'inbox', label: 'Show inbox', icon: <Inbox size={16} />, keywords: 'triage captures', run: () => void openInbox() },
      { id: 'graph', label: 'Show graph of the whole vault', shortcut: 'Ctrl+Shift+G', icon: <Waypoints size={16} />, keywords: 'links network', run: () => void showGraph('global') },
      ...(current ? [{ id: 'graph-local', label: `Show graph around “${current.title}”`, icon: <Locate size={16} />, keywords: 'local links', run: () => void showGraph('local') }] : []),
      { id: 'capture', label: 'Quick capture', shortcut: 'Ctrl+Shift+Space', icon: <Zap size={16} />, keywords: 'inbox thought', run: () => void api.toggleCaptureWindow() },
      ...(current ? [{ id: 'delete', label: `Delete “${current.title}”`, icon: <Trash2 size={16} />, run: () => void deleteCurrent() }] : []),
      { id: 'vault', label: 'Open another vault folder', icon: <FolderOpen size={16} />, keywords: 'switch', run: () => void pickVault() },
      { id: 'reindex', label: 'Rebuild the search index', icon: <RefreshCw size={16} />, keywords: 'reindex repair', run: () => void api.rebuildIndex().then(refreshList).catch((e) => setError(String(e))) },
    ],
    [createNote, openToday, openInbox, showGraph, current, deleteCurrent, pickVault, refreshList],
  )

  if (!vault) return <Welcome onPick={pickVault} error={error} />

  const inboxCount = notes.filter((n) => n.path.startsWith(INBOX_DIR + '/')).length
  const showNote = view === 'note' && current !== null

  return (
    <div className={`app ${showNote ? '' : 'app--no-context'}`}>
      <Sidebar
        vault={vault}
        notes={notes}
        currentId={showNote ? current.id : null}
        onOpen={openNote}
        onCreate={createNote}
        onPickVault={pickVault}
        onSearch={() => setPalette('notes')}
        inboxCount={inboxCount}
        inboxOpen={view === 'inbox'}
        graphOpen={view === 'graph'}
        onInbox={() => void openInbox()}
        onToday={() => void openToday()}
        onGraph={() => void showGraph(graphMode)}
      />
      <main className="app__main">
        {view === 'inbox' ? (
          <InboxView notes={notes} onOpen={(id) => void openNote(id)} onChanged={() => void refreshList()} onError={setError} />
        ) : view === 'graph' ? (
          <GraphView current={current} mode={graphMode} onModeChange={setGraphMode} version={version} onOpen={(id) => void openNote(id)} onCreate={(t) => void followLink(t)} onError={setError} />
        ) : current ? (
          <NotePane
            key={current.id}
            note={current}
            saveState={saveState}
            onChange={(n) => {
              setCurrent(n)
              scheduleSave(n)
            }}
            onFollowLink={followLink}
            onDelete={deleteCurrent}
            resolveSrc={resolveSrc}
            pickImage={pickImage}
            onEditorReady={(ed) => {
              editorRef.current = ed
            }}
            onFiles={(files, pos) => void onPastedFiles(files, pos)}
          />
        ) : (
          <div className="app__empty">
            <p>
              Pick a note on the left, press <kbd>Ctrl</kbd>+<kbd>N</kbd> for a new one, <kbd>Ctrl</kbd>+<kbd>D</kbd> for today, <kbd>Ctrl</kbd>+<kbd>K</kbd> to search, or <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd> for every command.
            </p>
            <p>
              <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Space</kbd> anywhere captures a thought to the inbox.
            </p>
          </div>
        )}
      </main>
      {showNote && <ContextPanel note={current} onOpen={openNote} onFollowLink={followLink} onShowGraph={() => void showGraph('local')} />}
      {palette && <Palette mode={palette} commands={commands} onClose={() => setPalette(null)} onOpen={(id) => void openNote(id)} onCreate={(t) => void createNote(t)} />}
      {error && (
        <div className="app__error" role="alert" onClick={() => setError(null)}>
          {error}
        </div>
      )}
    </div>
  )
}
