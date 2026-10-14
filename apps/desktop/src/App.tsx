import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { api, assetUrl, type Note, type NoteSummary, type VaultInfo } from './api'
import { Sidebar } from './components/Sidebar'
import { NotePane } from './components/NotePane'
import { ContextPanel } from './components/ContextPanel'
import { QuickSwitcher } from './components/QuickSwitcher'
import { Welcome } from './components/Welcome'

const LAST_VAULT_KEY = 'graphite.lastVault'
const SAVE_DELAY_MS = 400

export function App() {
  const [vault, setVault] = useState<VaultInfo | null>(null)
  const [notes, setNotes] = useState<NoteSummary[]>([])
  const [current, setCurrent] = useState<Note | null>(null)
  const [switcherOpen, setSwitcherOpen] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saveState, setSaveState] = useState<'saved' | 'dirty' | 'saving'>('saved')
  const pending = useRef<{ timer: number | null; note: Note | null }>({ timer: null, note: null })

  const refreshList = useCallback(async () => {
    try {
      setNotes(await api.listNotes())
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
      } catch (e) {
        setError(String(e))
      }
    },
    [flushSave],
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

  // Keyboard: Ctrl/Cmd+K quick switcher, Ctrl/Cmd+N new note, Ctrl/Cmd+S flush.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey
      if (!mod) return
      if (e.key === 'k' || e.key === 'p') {
        e.preventDefault()
        setSwitcherOpen(true)
      } else if (e.key === 'n') {
        e.preventDefault()
        void createNote(`Untitled ${new Date().toLocaleString()}`)
      } else if (e.key === 's') {
        e.preventDefault()
        void flushSave()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [createNote, flushSave])

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

  if (!vault) return <Welcome onPick={pickVault} error={error} />

  return (
    <div className="app">
      <Sidebar vault={vault} notes={notes} currentId={current?.id ?? null} onOpen={openNote} onCreate={createNote} onPickVault={pickVault} onSearch={() => setSwitcherOpen(true)} />
      <main className="app__main">
        {current ? (
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
          />
        ) : (
          <div className="app__empty">
            <p>Pick a note on the left, press <kbd>Ctrl</kbd>+<kbd>N</kbd> for a new one, or <kbd>Ctrl</kbd>+<kbd>K</kbd> to search.</p>
          </div>
        )}
      </main>
      {current && <ContextPanel note={current} onOpen={openNote} onFollowLink={followLink} />}
      {switcherOpen && <QuickSwitcher onClose={() => setSwitcherOpen(false)} onOpen={(id) => void openNote(id)} onCreate={(t) => void createNote(t)} />}
      {error && (
        <div className="app__error" role="alert" onClick={() => setError(null)}>
          {error}
        </div>
      )}
    </div>
  )
}
