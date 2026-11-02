import { useMemo, useState, type ReactNode } from 'react'
import { CalendarDays, ChevronRight, Download, FilePlus2, FolderOpen, Hash, Inbox, LayoutTemplate, ListChecks, Paperclip, RefreshCw, Search, Settings2, Star, Waypoints, X } from 'lucide-react'
import type { NoteSummary, SyncStatus, VaultInfo } from '../api'
import { syncSummary } from './sync'
import { TypeChip, TypeIcon, typeColour } from '../noteTypes'
import { hasTag, tagTree, type TagNode } from '../tags'

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

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

export function Sidebar({ vault, notes, currentId, onOpen, onCreate, onCreateFromTemplate, onPickVault, onSearch, inboxCount, inboxOpen, graphOpen, tasksOpen, filesOpen, sourcesOpen, openTasks, sourceUpdates, sync, syncing, onSync, onSyncSettings, onInbox, onToday, onGraph, onTasks, onFiles, onSources }: Props) {
  const [filter, setFilter] = useState('')
  const [newTitle, setNewTitle] = useState('')
  const [view, setView] = useState<'notes' | 'tags'>('notes')
  /** The tag picked in the tag pane; its notes and those of the tags under it are listed. */
  const [tag, setTag] = useState<string | null>(null)

  /** Notes passing the tag pick and the filter box (`#garden` filters by tag). */
  const visible = useMemo(() => {
    const q = filter.trim().toLowerCase()
    const tagged = tag ? notes.filter((n) => hasTag(n, tag)) : notes
    if (!q || view === 'tags') return tagged
    if (q.startsWith('#')) {
      const t = q.slice(1)
      return tagged.filter((n) => n.tags.some((x) => x.toLowerCase().startsWith(t)))
    }
    return tagged.filter((n) => n.title.toLowerCase().includes(q) || n.path.toLowerCase().includes(q))
  }, [notes, filter, tag, view])

  const bookmarks = useMemo(() => visible.filter((n) => n.bookmarked).sort((a, b) => collator.compare(a.title, b.title)), [visible])

  const tags = useMemo(() => {
    const tree = tagTree(notes)
    const q = filter.trim().toLowerCase().replace(/^#/, '')
    if (!q) return tree
    const keep = (nodes: TagNode[]): TagNode[] =>
      nodes.flatMap((n) => {
        if (n.path.toLowerCase().includes(q)) return [n]
        const children = keep(n.children)
        return children.length ? [{ ...n, children }] : []
      })
    return keep(tree)
  }, [notes, filter])

  const groups = useMemo(() => {
    const byFolder = new Map<string, NoteSummary[]>()
    for (const n of visible) {
      const folder = n.path.includes('/') ? n.path.slice(0, n.path.lastIndexOf('/')) : ''
      byFolder.set(folder, [...(byFolder.get(folder) ?? []), n])
    }
    // Folders and notes in name order (numbers compared as numbers), so the
    // list does not reshuffle as notes are edited.
    for (const items of byFolder.values()) items.sort((a, b) => collator.compare(a.title, b.title))
    const folders = [...byFolder.keys()]
    // Every note usually sits under one top folder (`notes/`); leave it out of the headings.
    const top = folders.length > 0 && folders.every((f) => f !== '') ? folders[0].split('/')[0] : null
    const shared = top !== null && folders.every((f) => f === top || f.startsWith(`${top}/`)) ? top : null
    const label = (f: string) => {
      const rel = shared && f.startsWith(`${shared}/`) ? f.slice(shared.length + 1) : f
      return rel ? rel.split('/').join(' / ') : shared ?? 'Vault root'
    }
    return [...byFolder.entries()].sort(([a], [b]) => collator.compare(a, b)).map(([folder, items]) => ({ folder, label: label(folder), items }))
  }, [visible])

  const noteRow = (n: NoteSummary) => (
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
        {(n.icon || n.color) && <TypeIcon type={n.type} icon={n.icon} color={n.color} size={14} />}
        <span className="sidebar__note-title">{n.title}</span>
        {n.type !== 'note' && <TypeChip type={n.type} />}
      </button>
    </li>
  )

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
          <Inbox size={14} color={typeColour('capture')} /> Inbox
          {inboxCount > 0 && <span className="sidebar__count">{inboxCount}</span>}
        </button>
        <button type="button" className="sidebar__nav-btn" onClick={onToday} title="Today's daily note (Ctrl+D)">
          <CalendarDays size={14} color={typeColour('daily')} /> Today
        </button>
        <button type="button" className={`sidebar__nav-btn ${graphOpen ? 'is-current' : ''}`} onClick={onGraph} title="Link graph (Ctrl+Shift+G)">
          <Waypoints size={14} /> Graph
        </button>
        <button type="button" className={`sidebar__nav-btn ${tasksOpen ? 'is-current' : ''}`} onClick={onTasks} title="Every open task in the vault (Ctrl+Shift+T)">
          <ListChecks size={14} color={typeColour('task')} /> Tasks
          {openTasks > 0 && <span className="sidebar__count sidebar__count--quiet">{openTasks}</span>}
        </button>
        <button type="button" className={`sidebar__nav-btn ${filesOpen ? 'is-current' : ''}`} onClick={onFiles} title="Every file in the vault: images, video, PDFs, 3D models (Ctrl+Shift+F)">
          <Paperclip size={14} /> Files
          {vault.asset_count > 0 && <span className="sidebar__count sidebar__count--quiet">{vault.asset_count}</span>}
        </button>
        <button type="button" className={`sidebar__nav-btn ${sourcesOpen ? 'is-current' : ''}`} onClick={onSources} title="Imported repos, pages, videos and files, and whether they changed upstream">
          <Download size={14} color={typeColour('source')} /> Sources
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
      <div className="sidebar__tabs" role="tablist" aria-label="Sidebar list">
        <button type="button" role="tab" aria-selected={view === 'notes'} className={view === 'notes' ? 'is-current' : ''} onClick={() => setView('notes')}>
          Notes
        </button>
        <button type="button" role="tab" aria-selected={view === 'tags'} className={view === 'tags' ? 'is-current' : ''} onClick={() => setView('tags')} title="Every tag, nested on /">
          Tags
        </button>
      </div>
      <input className="sidebar__filter" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder={view === 'tags' ? 'Filter tags' : 'Filter notes, or #tag'} aria-label={view === 'tags' ? 'Filter tags' : 'Filter notes'} />
      {view === 'tags' ? (
        <nav className="sidebar__list" aria-label="Tags">
          {tags.length === 0 && <p className="sidebar__empty">{filter.trim() ? 'No matching tags.' : 'No tags yet. Add them above a note or write #tag in the text.'}</p>}
          <TagList
            nodes={tags}
            open={Boolean(filter.trim())}
            current={tag}
            onPick={(t) => {
              setTag(t)
              setFilter('')
              setView('notes')
            }}
          />
        </nav>
      ) : (
        <nav className="sidebar__list" aria-label="Notes">
          {tag && (
            <div className="sidebar__picked">
              <span className="tag">#{tag}</span>
              <span className="sidebar__picked-count">{visible.length}</span>
              <button type="button" className="icon-btn" title="Show every note" aria-label="Clear tag" onClick={() => setTag(null)}>
                <X size={13} />
              </button>
            </div>
          )}
          {bookmarks.length > 0 && (
            <section className="sidebar__bookmarks">
              <h3 className="sidebar__folder">
                <Star size={11} fill="currentColor" /> Bookmarks
              </h3>
              <ul>{bookmarks.map(noteRow)}</ul>
            </section>
          )}
          {groups.length === 0 && <p className="sidebar__empty">{notes.length === 0 ? 'No notes yet.' : 'No matching notes.'}</p>}
          {groups.map(({ folder, label, items }) => (
            <section key={folder || '(root)'}>
              <h3 className="sidebar__folder" title={folder || undefined}>
                {label}
              </h3>
              <ul>{items.map(noteRow)}</ul>
            </section>
          ))}
        </nav>
      )}
      <div className="sidebar__footer">
        <span>{view === 'notes' && visible.length !== notes.length ? `${visible.length} of ${notes.length} notes` : `${notes.length} notes`}</span>
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

/** The tag tree; a tag with tags under it folds open with its chevron. */
function TagList({ nodes, open, current, onPick, depth = 0 }: { nodes: TagNode[]; open: boolean; current: string | null; onPick: (tag: string) => void; depth?: number }): ReactNode {
  return (
    <ul className="sidebar__tags">
      {nodes.map((n) => (
        <TagRow key={n.path} node={n} open={open} current={current} onPick={onPick} depth={depth} />
      ))}
    </ul>
  )
}

function TagRow({ node, open, current, onPick, depth }: { node: TagNode; open: boolean; current: string | null; onPick: (tag: string) => void; depth: number }) {
  const [expanded, setExpanded] = useState(false)
  const show = open || expanded
  return (
    <li>
      <div className={`sidebar__tag ${current?.toLowerCase() === node.path.toLowerCase() ? 'is-current' : ''}`} style={{ paddingLeft: 4 + depth * 14 }}>
        {node.children.length > 0 ? (
          <button type="button" className="sidebar__tag-fold" aria-label={show ? `Fold ${node.path}` : `Unfold ${node.path}`} aria-expanded={show} onClick={() => setExpanded((e) => !e)}>
            <ChevronRight size={12} className={show ? 'is-open' : ''} />
          </button>
        ) : (
          <span className="sidebar__tag-fold" />
        )}
        <button type="button" className="sidebar__tag-name" onClick={() => onPick(node.path)} title={`Notes tagged #${node.path} or a tag under it`}>
          <Hash size={12} />
          <span>{node.name}</span>
        </button>
        <span className="sidebar__count sidebar__count--quiet">{node.count}</span>
      </div>
      {show && node.children.length > 0 && <TagList nodes={node.children} open={open} current={current} onPick={onPick} depth={depth + 1} />}
    </li>
  )
}
