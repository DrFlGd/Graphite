import { useMemo, useState } from 'react'
import { CalendarDays, FilePlus2, FolderOpen, Inbox, Search, Waypoints } from 'lucide-react'
import type { NoteSummary, VaultInfo } from '../api'

interface Props {
  vault: VaultInfo
  notes: NoteSummary[]
  currentId: string | null
  onOpen: (id: string) => void
  onCreate: (title: string) => void
  onPickVault: () => void
  onSearch: () => void
  inboxCount: number
  inboxOpen: boolean
  graphOpen: boolean
  onInbox: () => void
  onToday: () => void
  onGraph: () => void
}

export function Sidebar({ vault, notes, currentId, onOpen, onCreate, onPickVault, onSearch, inboxCount, inboxOpen, graphOpen, onInbox, onToday, onGraph }: Props) {
  const [filter, setFilter] = useState('')
  const [newTitle, setNewTitle] = useState('')

  const groups = useMemo(() => {
    const q = filter.trim().toLowerCase()
    const visible = q ? notes.filter((n) => n.title.toLowerCase().includes(q) || n.path.toLowerCase().includes(q)) : notes
    const byFolder = new Map<string, NoteSummary[]>()
    for (const n of visible) {
      const folder = n.path.includes('/') ? n.path.slice(0, n.path.lastIndexOf('/')) : ''
      byFolder.set(folder, [...(byFolder.get(folder) ?? []), n])
    }
    return [...byFolder.entries()].sort(([a], [b]) => a.localeCompare(b))
  }, [notes, filter])

  const submitNew = () => {
    const t = newTitle.trim()
    if (!t) return
    onCreate(t)
    setNewTitle('')
  }

  return (
    <aside className="sidebar">
      <div className="sidebar__vault">
        <button type="button" className="sidebar__vault-name" title={vault.root} onClick={onPickVault}>
          <FolderOpen size={14} /> {vault.name}
        </button>
        <button type="button" className="icon-btn" title="Search (Ctrl+K)" onClick={onSearch}>
          <Search size={16} />
        </button>
      </div>
      <div className="sidebar__nav">
        <button type="button" className={`sidebar__nav-btn ${inboxOpen ? 'is-current' : ''}`} onClick={onInbox} title="Captured notes waiting to be filed">
          <Inbox size={14} /> Inbox
          {inboxCount > 0 && <span className="sidebar__count">{inboxCount}</span>}
        </button>
        <button type="button" className="sidebar__nav-btn" onClick={onToday} title="Today's daily note (Ctrl+D)">
          <CalendarDays size={14} /> Today
        </button>
        <button type="button" className={`sidebar__nav-btn ${graphOpen ? 'is-current' : ''}`} onClick={onGraph} title="Link graph (Ctrl+Shift+G)">
          <Waypoints size={14} /> Graph
        </button>
      </div>
      <form
        className="sidebar__new"
        onSubmit={(e) => {
          e.preventDefault()
          submitNew()
        }}
      >
        <input value={newTitle} onChange={(e) => setNewTitle(e.target.value)} placeholder="New note title…" aria-label="New note title" />
        <button type="submit" className="icon-btn" title="Create note (Ctrl+N)" disabled={!newTitle.trim()}>
          <FilePlus2 size={16} />
        </button>
      </form>
      <input className="sidebar__filter" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Filter notes" aria-label="Filter notes" />
      <nav className="sidebar__list" aria-label="Notes">
        {groups.length === 0 && <p className="sidebar__empty">No notes yet.</p>}
        {groups.map(([folder, items]) => (
          <section key={folder || '(root)'}>
            <h3 className="sidebar__folder">{folder || 'Vault root'}</h3>
            <ul>
              {items.map((n) => (
                <li key={n.id}>
                  <button type="button" className={`sidebar__note ${n.id === currentId ? 'is-current' : ''}`} onClick={() => onOpen(n.id)}>
                    <span className="sidebar__note-title">{n.title}</span>
                    {n.type !== 'note' && <span className="sidebar__note-type">{n.type}</span>}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </nav>
      <div className="sidebar__footer">{vault.note_count === notes.length ? `${notes.length} notes` : `${notes.length} of ${vault.note_count} notes`}</div>
    </aside>
  )
}
