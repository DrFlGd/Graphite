import { useMemo, useState } from 'react'
import { CalendarDays, Download, FilePlus2, FolderOpen, Inbox, LayoutTemplate, ListChecks, Paperclip, RefreshCw, Search, Settings2, Waypoints } from 'lucide-react'
import type { NoteSummary, SyncStatus, VaultInfo } from '../api'
import { syncSummary } from './sync'

interface Props {
  vault: VaultInfo
  notes: NoteSummary[]
  currentId: string | null
  /** `newTab` when the user Ctrl/Cmd-clicks or middle-clicks. */
  onOpen: (id: string, newTab?: boolean) => void
  onCreate: (title: string) => void
  onCreateFromTemplate: () => void
  onPickVault: () => void
  onSearch: () => void
  inboxCount: number
  inboxOpen: boolean
  graphOpen: boolean
  tasksOpen: boolean
  filesOpen: boolean
  sourcesOpen: boolean
  openTasks: number
  /** Sources that changed upstream since they were fetched. */
  sourceUpdates: number
  sync: SyncStatus | null
  syncing: boolean
  onSync: () => void
  onSyncSettings: () => void
  onInbox: () => void
  onToday: () => void
  onGraph: () => void
  onTasks: () => void
  onFiles: () => void
  onSources: () => void
}

export function Sidebar({ vault, notes, currentId, onOpen, onCreate, onCreateFromTemplate, onPickVault, onSearch, inboxCount, inboxOpen, graphOpen, tasksOpen, filesOpen, sourcesOpen, openTasks, sourceUpdates, sync, syncing, onSync, onSyncSettings, onInbox, onToday, onGraph, onTasks, onFiles, onSources }: Props) {
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
        <button type="button" className={`sidebar__nav-btn ${tasksOpen ? 'is-current' : ''}`} onClick={onTasks} title="Every open task in the vault (Ctrl+Shift+T)">
          <ListChecks size={14} /> Tasks
          {openTasks > 0 && <span className="sidebar__count sidebar__count--quiet">{openTasks}</span>}
        </button>
        <button type="button" className={`sidebar__nav-btn ${filesOpen ? 'is-current' : ''}`} onClick={onFiles} title="Every file in the vault: images, video, PDFs, 3D models (Ctrl+Shift+F)">
          <Paperclip size={14} /> Files
          {vault.asset_count > 0 && <span className="sidebar__count sidebar__count--quiet">{vault.asset_count}</span>}
        </button>
        <button type="button" className={`sidebar__nav-btn ${sourcesOpen ? 'is-current' : ''}`} onClick={onSources} title="Imported repos, pages, videos and files, and whether they changed upstream">
          <Download size={14} /> Sources
          {sourceUpdates > 0 && <span className="sidebar__count" title={`${sourceUpdates} changed upstream`}>{sourceUpdates}</span>}
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
        <button type="button" className="icon-btn" title="New from template: project, research, decision… (Ctrl+Shift+N)" onClick={onCreateFromTemplate}>
          <LayoutTemplate size={16} />
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
                  <button
                    type="button"
                    className={`sidebar__note ${n.id === currentId ? 'is-current' : ''}`}
                    onClick={(e) => onOpen(n.id, e.ctrlKey || e.metaKey)}
                    onAuxClick={(e) => {
                      if (e.button === 1) {
                        e.preventDefault()
                        onOpen(n.id, true)
                      }
                    }}
                    title="Click to open here, Ctrl+click for a new tab"
                  >
                    <span className="sidebar__note-title">{n.title}</span>
                    {n.type !== 'note' && <span className="sidebar__note-type">{n.type}</span>}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </nav>
      <div className="sidebar__footer">
        <span>{vault.note_count === notes.length ? `${notes.length} notes` : `${notes.length} of ${vault.note_count} notes`}</span>
        <button
          type="button"
          className={`sidebar__sync ${sync?.enabled ? 'is-on' : ''}`}
          onClick={onSync}
          disabled={syncing}
          title={sync?.enabled ? 'Merge with the other devices sharing this folder now' : 'Turn on sync: other devices opening this folder will merge their edits with yours'}
        >
          <RefreshCw size={12} className={syncing ? 'is-spinning' : ''} />
          <span>{syncing ? 'Syncing' : syncSummary(sync)}</span>
        </button>
        <button type="button" className="sidebar__sync sidebar__sync-settings" onClick={onSyncSettings} title="Sync settings: devices, relay, large files" aria-label="Sync settings">
          <Settings2 size={12} />
        </button>
      </div>
    </aside>
  )
}
