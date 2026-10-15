import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { List, ListTree, Play, Save, SquareKanban, Table2 } from 'lucide-react'
import { api, type ItemRow, type ItemStatus, type NoteRow, type QueryResult } from '../api'
import { buildTree, cellText, cellValue, domainOf, propertyKeys, sliceGroups, termValue, withTerm, type TreeNode } from '../query'

export type ViewKind = 'list' | 'table' | 'board' | 'tree'
export const VIEW_KINDS: ViewKind[] = ['list', 'table', 'board', 'tree']
export const isViewKind = (v: unknown): v is ViewKind => typeof v === 'string' && (VIEW_KINDS as string[]).includes(v)

interface Props {
  query: string
  onQueryChange: (q: string) => void
  view: ViewKind
  onViewChange: (v: ViewKind) => void
  /** Bumped by the app when notes change, so results refetch. */
  version: number
  onOpen: (id: string, newTab?: boolean) => void
  /** Called after this view wrote to a note. */
  onChanged: () => void
  onError: (e: string) => void
  /** Offer a "Save as query note" button. */
  onSave?: (query: string, view: ViewKind) => void
  /** Hide the query line (the Tasks view has its own controls). */
  hideQuery?: boolean
  /** Extra controls on the toolbar. */
  extra?: ReactNode
}

const ITEM_GROUPS = ['status', 'project', 'priority', 'due', 'note', 'heading', 'tag']
const NOTE_GROUPS = ['type', 'tag', 'status', 'project', 'folder']
const STATUSES: ItemStatus[] = ['open', 'doing', 'done', 'cancelled']

/**
 * One query, four ways to look at it: list, table (inline editing), board
 * (columns from `group:`) and task tree. Every view writes back through the
 * core, so a ticked box or an edited cell changes one line of a file.
 */
export function QueryView({ query, onQueryChange, view, onViewChange, version, onOpen, onChanged, onError, onSave, hideQuery, extra }: Props) {
  const [draft, setDraft] = useState(query)
  const [result, setResult] = useState<QueryResult | null>(null)
  const [busy, setBusy] = useState(false)
  useEffect(() => setDraft(query), [query])

  const domain = domainOf(query)
  const effectiveView: ViewKind = view === 'tree' && domain === 'notes' ? 'list' : view
  const group = termValue(query, 'group')
  // The board needs columns; pick a default grouping when the query has none.
  const effective = effectiveView === 'board' && !group ? withTerm(query, 'group', domain === 'items' ? 'status' : 'type') : query

  useEffect(() => {
    let cancelled = false
    setBusy(true)
    api
      .query(effective)
      .then((r) => {
        if (!cancelled) setResult(r)
      })
      .catch((e) => {
        if (!cancelled) onError(String(e))
      })
      .finally(() => {
        if (!cancelled) setBusy(false)
      })
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [effective, version])

  const act = useCallback(
    async (f: () => Promise<unknown>) => {
      try {
        await f()
        onChanged()
      } catch (e) {
        onError(String(e))
      }
    },
    [onChanged, onError],
  )

  const commit = () => {
    const q = draft.trim()
    if (q !== query) onQueryChange(q)
  }

  const groupOptions = domain === 'items' ? ITEM_GROUPS : NOTE_GROUPS
  const itemGroups = useMemo(() => (result ? sliceGroups(result.items, result) : []), [result])
  const noteGroups = useMemo(() => (result ? sliceGroups(result.notes, result) : []), [result])

  return (
    <div className="qv">
      <div className="qv__toolbar">
        {!hideQuery && (
          <form
            className="qv__query"
            onSubmit={(e) => {
              e.preventDefault()
              commit()
            }}
          >
            <input value={draft} onChange={(e) => setDraft(e.target.value)} onBlur={commit} placeholder="type:task status:open sort:due" aria-label="Query" spellCheck={false} />
            <button type="submit" className="icon-btn" title="Run">
              <Play size={14} />
            </button>
          </form>
        )}
        {extra}
        <div className="qv__views" role="radiogroup" aria-label="View">
          <ViewButton kind="list" current={effectiveView} onPick={onViewChange} icon={<List size={14} />} />
          <ViewButton kind="table" current={effectiveView} onPick={onViewChange} icon={<Table2 size={14} />} />
          <ViewButton kind="board" current={effectiveView} onPick={onViewChange} icon={<SquareKanban size={14} />} />
          {domain === 'items' && <ViewButton kind="tree" current={effectiveView} onPick={onViewChange} icon={<ListTree size={14} />} />}
        </div>
        {(effectiveView === 'board' || group) && (
          <label className="qv__group">
            Group by
            <select value={group ?? ''} onChange={(e) => onQueryChange(withTerm(query, 'group', e.target.value || null))} aria-label="Group by">
              {effectiveView !== 'board' && <option value="">none</option>}
              {(group && !groupOptions.includes(group) ? [group, ...groupOptions] : groupOptions).map((g) => (
                <option key={g} value={g}>
                  {g}
                </option>
              ))}
            </select>
          </label>
        )}
        <span className="qv__count">{busy ? 'Running…' : result ? `${result.total} ${domain === 'items' ? 'items' : 'notes'}` : ''}</span>
        {onSave && (
          <button type="button" className="icon-btn" title="Save as a query note" onClick={() => onSave(query, view)}>
            <Save size={14} />
          </button>
        )}
      </div>
      <div className="qv__body">
        {result && result.total === 0 && <p className="qv__empty">Nothing matches.</p>}
        {result && result.total > 0 && domain === 'items' && effectiveView === 'tree' && <ItemTree rows={result.items} groups={itemGroups} onOpen={onOpen} act={act} />}
        {result && result.total > 0 && domain === 'items' && effectiveView === 'list' && <ItemList groups={itemGroups} grouped={!!group} onOpen={onOpen} act={act} />}
        {result && result.total > 0 && domain === 'items' && effectiveView === 'table' && <ItemTable rows={result.items} onOpen={onOpen} act={act} />}
        {result && result.total > 0 && domain === 'items' && effectiveView === 'board' && (
          <Board groups={itemGroups} render={(r) => <ItemLine key={`${r.note_id}:${r.line}`} row={r} onOpen={onOpen} act={act} showNote />} />
        )}
        {result && result.total > 0 && domain === 'notes' && effectiveView === 'list' && <NoteList groups={noteGroups} onOpen={onOpen} />}
        {result && result.total > 0 && domain === 'notes' && effectiveView === 'table' && <NoteTable rows={result.notes} onOpen={onOpen} act={act} />}
        {result && result.total > 0 && domain === 'notes' && effectiveView === 'board' && (
          <Board groups={noteGroups} render={(n) => <NoteCard key={n.id} row={n} onOpen={onOpen} />} />
        )}
      </div>
    </div>
  )
}

function ViewButton({ kind, current, onPick, icon }: { kind: ViewKind; current: ViewKind; onPick: (v: ViewKind) => void; icon: ReactNode }) {
  return (
    <button type="button" role="radio" aria-checked={kind === current} className={`qv__view ${kind === current ? 'is-current' : ''}`} title={`${kind[0].toUpperCase()}${kind.slice(1)} view`} onClick={() => onPick(kind)}>
      {icon}
      <span>{kind}</span>
    </button>
  )
}

type Act = (f: () => Promise<unknown>) => Promise<void>

// ---------- items ----------

const toggleStatus = (s: ItemStatus): ItemStatus => (s === 'done' ? 'open' : 'done')

/** The checkbox for a task, or the `?` of a gap (click to record where the answer is). */
function ItemBox({ row, act }: { row: ItemRow; act: Act }) {
  if (row.kind === 'gap') {
    const answered = row.status === 'answered'
    return (
      <button
        type="button"
        className={`qv__gap ${answered ? 'is-answered' : ''}`}
        title={answered ? `Answered in ${row.answer ?? ''}` : 'Record where the answer is'}
        onClick={() => {
          if (answered) return
          const target = window.prompt('Which note answers this? (title)')
          if (target?.trim()) void act(() => api.answerGap(row.note_id, row.line, target.trim()))
        }}
      >
        {answered ? '✓' : '?'}
      </button>
    )
  }
  return (
    <input
      type="checkbox"
      className={`qv__check qv__check--${row.status}`}
      checked={row.status === 'done'}
      aria-label={row.status === 'done' ? 'Mark open' : 'Mark done'}
      title={row.status === 'doing' ? 'In progress (click to finish)' : row.status === 'cancelled' ? 'Cancelled' : undefined}
      onChange={() => void act(() => api.setItemStatus(row.note_id, row.line, toggleStatus(row.status)))}
    />
  )
}

function ItemBadges({ row, showNote, onOpen }: { row: ItemRow; showNote?: boolean; onOpen: (id: string) => void }) {
  const overdue = row.due && row.status !== 'done' && row.status !== 'cancelled' && row.due < new Date().toISOString().slice(0, 10)
  return (
    <span className="qv__badges">
      {row.due && <span className={`qv__badge ${overdue ? 'is-overdue' : ''}`}>{row.due}</span>}
      {row.priority && <span className={`qv__badge qv__badge--p${row.priority}`}>p{row.priority}</span>}
      {row.tags.filter((t) => !/^p[1-4]$/.test(t)).map((t) => (
        <span key={t} className="qv__badge">
          #{t}
        </span>
      ))}
      {showNote && (
        <button type="button" className="qv__badge qv__badge--note" onClick={() => onOpen(row.note_id)} title={row.note_path}>
          {row.note_title}
        </button>
      )}
      {row.project_title && !showNote && row.project_id && (
        <button type="button" className="qv__badge qv__badge--note" onClick={() => onOpen(row.project_id!)}>
          {row.project_title}
        </button>
      )}
    </span>
  )
}

function ItemLine({ row, onOpen, act, showNote }: { row: ItemRow; onOpen: (id: string) => void; act: Act; showNote?: boolean }) {
  return (
    <div className={`qv__item qv__item--${row.status} qv__item--${row.kind}`}>
      <ItemBox row={row} act={act} />
      <button type="button" className="qv__item-text" onClick={() => onOpen(row.note_id)} title={`Open ${row.note_title}`}>
        {row.text}
      </button>
      <ItemBadges row={row} showNote={showNote} onOpen={onOpen} />
    </div>
  )
}

function ItemTree({ rows, groups, onOpen, act }: { rows: ItemRow[]; groups: { key: string; rows: ItemRow[] }[]; onOpen: (id: string) => void; act: Act }) {
  const render = (nodes: TreeNode[]) => (
    <ul className="qv__tree">
      {nodes.map((n) => (
        <li key={`${n.row.note_id}:${n.row.line}`}>
          <ItemLine row={n.row} onOpen={onOpen} act={act} />
          {n.children.length > 0 && render(n.children)}
        </li>
      ))}
    </ul>
  )
  if (groups.length > 1 || groups[0]?.key) {
    return (
      <div className="qv__sections">
        {groups.map((g) => (
          <section key={g.key || '(none)'} className="qv__section">
            <h3>{g.key || 'No value'}</h3>
            {render(buildTree(g.rows))}
          </section>
        ))}
      </div>
    )
  }
  // Ungrouped: one section per note, like the CLI prints it.
  const byNote = new Map<string, ItemRow[]>()
  for (const r of rows) byNote.set(r.note_id, [...(byNote.get(r.note_id) ?? []), r])
  return (
    <div className="qv__sections">
      {[...byNote.entries()].map(([id, list]) => (
        <section key={id} className="qv__section">
          <h3>
            <button type="button" className="qv__link" onClick={() => onOpen(id)}>
              {list[0].note_title}
            </button>
          </h3>
          {render(buildTree(list))}
        </section>
      ))}
    </div>
  )
}

function ItemList({ groups, grouped, onOpen, act }: { groups: { key: string; rows: ItemRow[] }[]; grouped: boolean; onOpen: (id: string) => void; act: Act }) {
  return (
    <div className="qv__sections">
      {groups.map((g) => (
        <section key={g.key || '(none)'} className="qv__section">
          {grouped && <h3>{g.key || 'No value'}</h3>}
          {g.rows.map((r) => (
            <ItemLine key={`${r.note_id}:${r.line}`} row={r} onOpen={onOpen} act={act} showNote />
          ))}
        </section>
      ))}
    </div>
  )
}

function ItemTable({ rows, onOpen, act }: { rows: ItemRow[]; onOpen: (id: string) => void; act: Act }) {
  const fieldKeys = useMemo(() => {
    const keys: string[] = []
    for (const r of rows) for (const k of Object.keys(r.fields)) if (!['due', 'priority', 'project', 'answered'].includes(k) && !keys.includes(k)) keys.push(k)
    return keys
  }, [rows])
  return (
    <table className="qv__table">
      <thead>
        <tr>
          <th />
          <th>Item</th>
          <th>Status</th>
          <th>Due</th>
          <th>Priority</th>
          <th>Project</th>
          {fieldKeys.map((k) => (
            <th key={k}>{k}</th>
          ))}
          <th>Note</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={`${r.note_id}:${r.line}`} className={`qv__row--${r.status}`}>
            <td>
              <ItemBox row={r} act={act} />
            </td>
            <td>
              <button type="button" className="qv__link" onClick={() => onOpen(r.note_id)}>
                {r.text}
              </button>
            </td>
            <td>
              {r.kind === 'task' ? (
                <select value={r.status} onChange={(e) => void act(() => api.setItemStatus(r.note_id, r.line, e.target.value as ItemStatus))} aria-label="Status">
                  {STATUSES.map((s) => (
                    <option key={s} value={s}>
                      {s}
                    </option>
                  ))}
                </select>
              ) : (
                r.status
              )}
            </td>
            <td>
              <input type="date" value={r.due ?? ''} onChange={(e) => void act(() => api.setItemField(r.note_id, r.line, 'due', e.target.value))} aria-label="Due" />
            </td>
            <td>
              <select value={r.priority ? `p${r.priority}` : ''} onChange={(e) => void act(() => api.setItemField(r.note_id, r.line, 'priority', e.target.value))} aria-label="Priority">
                <option value="">—</option>
                <option value="p1">p1</option>
                <option value="p2">p2</option>
                <option value="p3">p3</option>
                <option value="p4">p4</option>
              </select>
            </td>
            <td>
              <FieldCell value={r.project ?? ''} placeholder="[[Project]]" onCommit={(v) => void act(() => api.setItemField(r.note_id, r.line, 'project', v && !v.startsWith('[[') ? `[[${v}]]` : v))} />
            </td>
            {fieldKeys.map((k) => (
              <td key={k}>
                <FieldCell value={r.fields[k] ?? ''} onCommit={(v) => void act(() => api.setItemField(r.note_id, r.line, k, v))} />
              </td>
            ))}
            <td>
              <button type="button" className="qv__link qv__muted" onClick={() => onOpen(r.note_id)}>
                {r.note_title}
              </button>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

/** A text cell that writes on Enter or blur, only when the text changed. */
function FieldCell({ value, placeholder, onCommit }: { value: string; placeholder?: string; onCommit: (v: string) => void }) {
  const [draft, setDraft] = useState(value)
  useEffect(() => setDraft(value), [value])
  const commit = () => {
    if (draft.trim() !== value.trim()) onCommit(draft.trim())
  }
  return (
    <input
      className="qv__cell"
      value={draft}
      placeholder={placeholder}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault()
          commit()
          ;(e.target as HTMLInputElement).blur()
        } else if (e.key === 'Escape') setDraft(value)
      }}
    />
  )
}

// ---------- notes ----------

function NoteCard({ row, onOpen }: { row: NoteRow; onOpen: (id: string, newTab?: boolean) => void }) {
  return (
    <button type="button" className="qv__note" onClick={(e) => onOpen(row.id, e.ctrlKey || e.metaKey)}>
      <span className="qv__note-title">{row.title}</span>
      <span className="qv__badges">
        {row.type !== 'note' && <span className="qv__badge">{row.type}</span>}
        {row.tags.map((t) => (
          <span key={t} className="qv__badge">
            #{t}
          </span>
        ))}
      </span>
    </button>
  )
}

function NoteList({ groups, onOpen }: { groups: { key: string; rows: NoteRow[] }[]; onOpen: (id: string, newTab?: boolean) => void }) {
  return (
    <div className="qv__sections">
      {groups.map((g) => (
        <section key={g.key || '(none)'} className="qv__section">
          {(groups.length > 1 || g.key) && <h3>{g.key || 'No value'}</h3>}
          {g.rows.map((n) => (
            <NoteCard key={n.id} row={n} onOpen={onOpen} />
          ))}
        </section>
      ))}
    </div>
  )
}

function NoteTable({ rows, onOpen, act }: { rows: NoteRow[]; onOpen: (id: string, newTab?: boolean) => void; act: Act }) {
  const keys = useMemo(() => propertyKeys(rows), [rows])
  const setProperty = (id: string, key: string, text: string, previous: unknown) =>
    act(async () => {
      const note = await api.readNote(id)
      const properties = { ...note.properties }
      const v = cellValue(text, previous)
      if (v === null) delete properties[key]
      else properties[key] = v
      await api.saveNote({ ...note, properties })
    })
  return (
    <table className="qv__table">
      <thead>
        <tr>
          <th>Title</th>
          <th>Type</th>
          <th>Tags</th>
          {keys.map((k) => (
            <th key={k}>{k}</th>
          ))}
          <th>Updated</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((n) => (
          <tr key={n.id}>
            <td>
              <button type="button" className="qv__link" onClick={(e) => onOpen(n.id, e.ctrlKey || e.metaKey)}>
                {n.title}
              </button>
            </td>
            <td>
              <FieldCell value={n.type} onCommit={(v) => void act(() => api.setNoteType(n.id, v || 'note'))} />
            </td>
            <td className="qv__muted">{n.tags.join(', ')}</td>
            {keys.map((k) => (
              <td key={k}>
                <FieldCell value={cellText(n.properties[k])} onCommit={(v) => void setProperty(n.id, k, v, n.properties[k])} />
              </td>
            ))}
            <td className="qv__muted">{n.updated ? new Date(n.updated).toLocaleDateString() : ''}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

// ---------- board ----------

function Board<T>({ groups, render }: { groups: { key: string; rows: T[] }[]; render: (row: T) => ReactNode }) {
  return (
    <div className="qv__board">
      {groups.map((g) => (
        <section key={g.key || '(none)'} className="qv__column">
          <h3>
            {g.key || 'No value'} <span className="qv__muted">{g.rows.length}</span>
          </h3>
          <div className="qv__cards">{g.rows.map(render)}</div>
        </section>
      ))}
    </div>
  )
}
