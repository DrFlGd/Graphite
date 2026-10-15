import type { MouseEvent, ReactNode } from 'react'
import { Columns2, FileText, Inbox, ListChecks, Paperclip, Plus, Waypoints, X } from 'lucide-react'
import type { NoteSummary } from '../api'
import type { Pane as PaneModel, Tab } from '../layout'

interface Props {
  pane: PaneModel
  active: boolean
  notes: NoteSummary[]
  canSplit: boolean
  onFocus: () => void
  onSelect: (ti: number) => void
  onClose: (ti: number) => void
  onNewTab: () => void
  onSplit: () => void
  children: ReactNode
}

export function tabTitle(tab: Tab, notes: NoteSummary[]): string {
  switch (tab.kind) {
    case 'note':
      return notes.find((n) => n.id === tab.noteId)?.title ?? 'Note'
    case 'inbox':
      return 'Inbox'
    case 'graph':
      return 'Graph'
    case 'tasks':
      return 'Tasks'
    case 'files':
      return 'Files'
    case 'asset':
      return tab.path.split('/').pop() ?? 'File'
    default:
      return 'New tab'
  }
}

/** A column of tabs with one body: notes, the inbox or the graph. */
export function Pane({ pane, active, notes, canSplit, onFocus, onSelect, onClose, onNewTab, onSplit, children }: Props) {
  const icon = (t: Tab) => (t.kind === 'inbox' ? <Inbox size={13} /> : t.kind === 'graph' ? <Waypoints size={13} /> : t.kind === 'tasks' ? <ListChecks size={13} /> : t.kind === 'files' || t.kind === 'asset' ? <Paperclip size={13} /> : t.kind === 'note' ? <FileText size={13} /> : null)
  const middleClick = (e: MouseEvent, ti: number) => {
    if (e.button === 1) {
      e.preventDefault()
      onClose(ti)
    }
  }
  return (
    <section className={`pane ${active ? 'is-active' : ''}`} onMouseDownCapture={onFocus} aria-label={active ? 'Active pane' : 'Pane'}>
      <div className="pane__tabs" role="tablist">
        {pane.tabs.map((t, i) => (
          <div key={t.id} role="tab" aria-selected={i === pane.active} className={`pane__tab ${i === pane.active ? 'is-current' : ''}`} onMouseDown={(e) => e.button === 0 && onSelect(i)} onAuxClick={(e) => middleClick(e, i)} title={tabTitle(t, notes)}>
            {icon(t)}
            <span className="pane__tab-title">{tabTitle(t, notes)}</span>
            <button
              type="button"
              className="pane__tab-close"
              aria-label={`Close ${tabTitle(t, notes)}`}
              onMouseDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation()
                onClose(i)
              }}
            >
              <X size={12} />
            </button>
          </div>
        ))}
        <button type="button" className="icon-btn pane__action" title="New tab (Ctrl+T)" onClick={onNewTab}>
          <Plus size={14} />
        </button>
        <button type="button" className="icon-btn pane__action" title="Split right (Ctrl+\\)" onClick={onSplit} disabled={!canSplit}>
          <Columns2 size={14} />
        </button>
      </div>
      <div className="pane__body">{children}</div>
    </section>
  )
}
