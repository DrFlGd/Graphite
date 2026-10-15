import { Fragment, useEffect, useState, type ReactNode } from 'react'
import { Maximize2, Plus, X } from 'lucide-react'
import { api, type Backlink, type Graph, type Note, type NoteSummary, type Outgoing } from '../api'
import { GraphCanvas } from './GraphCanvas'

interface Props {
  note: Note
  onOpen: (id: string) => void
  onFollowLink: (target: string) => void
  /** Show the full graph view centred on this note. */
  onShowGraph: () => void
  /** Change front-matter (properties, aliases, type); the note tab saves it. */
  onEdit: (patch: Partial<Pick<Note, 'properties' | 'aliases' | 'type'>>) => void
}

/** Right-hand panel: backlinks, outgoing links, unlinked mentions, properties. */
export function ContextPanel({ note, onOpen, onFollowLink, onShowGraph, onEdit }: Props) {
  const [backlinks, setBacklinks] = useState<Backlink[]>([])
  const [outgoing, setOutgoing] = useState<Outgoing[]>([])
  const [mentions, setMentions] = useState<NoteSummary[]>([])
  const [graph, setGraph] = useState<Graph>({ nodes: [], edges: [] })

  useEffect(() => {
    let cancelled = false
    const load = async () => {
      const [b, o, m, g] = await Promise.all([api.backlinks(note.id), api.outgoingLinks(note.id), api.unlinkedMentions(note.id), api.localGraph(note.id, 1)])
      if (cancelled) return
      setBacklinks(b)
      setOutgoing(o)
      setMentions(m)
      setGraph(g)
    }
    void load().catch(() => {})
    return () => {
      cancelled = true
    }
    // Reload when the body or title changes (after save) so links stay current.
  }, [note.id, note.updated, note.title])


  return (
    <aside className="context">
      <Section
        title="Graph"
        action={
          <button type="button" className="icon-btn" title="Open the graph view" onClick={onShowGraph}>
            <Maximize2 size={14} />
          </button>
        }
      >
        {graph.nodes.length > 1 ? (
          <GraphCanvas graph={graph} focusId={note.id} compact className="context__graph" onOpen={(n) => (n.missing ? onFollowLink(n.title) : onOpen(n.id))} />
        ) : (
          <p className="context__empty">Links in and out will be drawn here.</p>
        )}
      </Section>
      <Section title={`Backlinks (${backlinks.length})`}>
        {backlinks.length === 0 && <p className="context__empty">Nothing links here yet.</p>}
        {backlinks.map((b, i) => (
          <button key={`${b.from.id}-${i}`} type="button" className="context__item" onClick={() => onOpen(b.from.id)}>
            <span>{b.from.title}</span>
            {b.relation && <span className="context__rel">{b.relation}</span>}
          </button>
        ))}
      </Section>
      <Section title={`Links out (${outgoing.length})`}>
        {outgoing.map((o) => (
          <button key={o.target} type="button" className={`context__item ${o.id ? '' : 'is-missing'}`} onClick={() => (o.id ? onOpen(o.id) : onFollowLink(o.target))}>
            <span>{o.target}</span>
            {!o.id && <span className="context__rel">create</span>}
          </button>
        ))}
      </Section>
      {mentions.length > 0 && (
        <Section title={`Unlinked mentions (${mentions.length})`}>
          {mentions.map((m) => (
            <button key={m.id} type="button" className="context__item" onClick={() => onOpen(m.id)}>
              {m.title}
            </button>
          ))}
        </Section>
      )}
      <Section title="Properties">
        <PropertyEditor note={note} onEdit={onEdit} />
      </Section>
    </aside>
  )
}


/** Front-matter as an editable table: type, aliases and every user property. */
function PropertyEditor({ note, onEdit }: { note: Note; onEdit: Props['onEdit'] }) {
  const [newKey, setNewKey] = useState('')
  const props = Object.entries(note.properties)
  const setProp = (key: string, text: string) => {
    const properties = { ...note.properties }
    const prev = properties[key]
    const t = text.trim()
    if (t === '') delete properties[key]
    else if (typeof prev === 'number' && /^-?\d+(\.\d+)?$/.test(t)) properties[key] = Number(t)
    else if (typeof prev === 'boolean' && (t === 'true' || t === 'false')) properties[key] = t === 'true'
    else if (Array.isArray(prev)) properties[key] = t.split(',').map((x) => x.trim()).filter(Boolean)
    else properties[key] = t
    onEdit({ properties })
  }
  const addKey = () => {
    const k = newKey.trim().toLowerCase().replace(/\s+/g, '-')
    if (!k || k in note.properties) return
    onEdit({ properties: { ...note.properties, [k]: '' } })
    setNewKey('')
  }
  const when = (iso: string) => new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
  return (
    <>
      <dl className="context__props">
        <dt>type</dt>
        <dd>
          <Cell value={note.type} onCommit={(v) => onEdit({ type: v || 'note' })} />
        </dd>
        <dt>aliases</dt>
        <dd>
          <Cell value={note.aliases.join(', ')} placeholder="comma separated" onCommit={(v) => onEdit({ aliases: v.split(',').map((x) => x.trim()).filter(Boolean) })} />
        </dd>
        {props.map(([k, v]) => (
          <Fragment key={k}>
            <dt>
              <span className="context__key" title={k}>
                {k}
              </span>
              <button type="button" className="context__remove" title={`Remove ${k}`} onClick={() => setProp(k, '')}>
                <X size={11} />
              </button>
            </dt>
            <dd>
              <Cell value={typeof v === 'string' ? v : Array.isArray(v) ? v.map(String).join(', ') : JSON.stringify(v)} onCommit={(t) => setProp(k, t)} />
            </dd>
          </Fragment>
        ))}
        <dt>
          <input className="context__cell context__newkey" value={newKey} onChange={(e) => setNewKey(e.target.value)} placeholder="new property" aria-label="New property name" onKeyDown={(e) => e.key === 'Enter' && (e.preventDefault(), addKey())} />
        </dt>
        <dd>
          <button type="button" className="icon-btn" title="Add property" disabled={!newKey.trim()} onClick={addKey}>
            <Plus size={13} />
          </button>
        </dd>
      </dl>
      <div className="context__meta">
        {note.created && <span>Created {when(note.created)}</span>}
        {note.updated && <span>Updated {when(note.updated)}</span>}
        <span className="mono" title="Note id">{note.id}</span>
      </div>
    </>
  )
}

/** A value cell that commits on Enter or blur, only when changed. */
function Cell({ value, placeholder, onCommit }: { value: string; placeholder?: string; onCommit: (v: string) => void }) {
  const [draft, setDraft] = useState(value)
  useEffect(() => setDraft(value), [value])
  const commit = () => {
    if (draft.trim() !== value.trim()) onCommit(draft.trim())
  }
  return (
    <input
      className="context__cell"
      value={draft}
      title={value || undefined}
      placeholder={placeholder}
      aria-label="Value"
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

function Section({ title, action, children }: { title: string; action?: ReactNode; children: ReactNode }) {
  return (
    <section className="context__section">
      <div className="context__section-head">
        <h3>{title}</h3>
        {action}
      </div>
      {children}
    </section>
  )
}
