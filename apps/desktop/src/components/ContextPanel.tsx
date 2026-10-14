import { Fragment, useEffect, useState, type ReactNode } from 'react'
import { api, type Backlink, type Note, type NoteSummary, type Outgoing } from '../api'

interface Props {
  note: Note
  onOpen: (id: string) => void
  onFollowLink: (target: string) => void
}

/** Right-hand panel: backlinks, outgoing links, unlinked mentions, properties. */
export function ContextPanel({ note, onOpen, onFollowLink }: Props) {
  const [backlinks, setBacklinks] = useState<Backlink[]>([])
  const [outgoing, setOutgoing] = useState<Outgoing[]>([])
  const [mentions, setMentions] = useState<NoteSummary[]>([])

  useEffect(() => {
    let cancelled = false
    const load = async () => {
      const [b, o, m] = await Promise.all([api.backlinks(note.id), api.outgoingLinks(note.id), api.unlinkedMentions(note.id)])
      if (cancelled) return
      setBacklinks(b)
      setOutgoing(o)
      setMentions(m)
    }
    void load().catch(() => {})
    return () => {
      cancelled = true
    }
    // Reload when the body or title changes (after save) so links stay current.
  }, [note.id, note.updated, note.title])

  const props = Object.entries(note.properties)

  return (
    <aside className="context">
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
        <dl className="context__props">
          <dt>type</dt>
          <dd>{note.type}</dd>
          <dt>id</dt>
          <dd className="mono">{note.id}</dd>
          {note.created && (
            <>
              <dt>created</dt>
              <dd>{new Date(note.created).toLocaleString()}</dd>
            </>
          )}
          {note.updated && (
            <>
              <dt>updated</dt>
              <dd>{new Date(note.updated).toLocaleString()}</dd>
            </>
          )}
          {note.aliases.length > 0 && (
            <>
              <dt>aliases</dt>
              <dd>{note.aliases.join(', ')}</dd>
            </>
          )}
          {props.map(([k, v]) => (
            <Fragment key={k}>
              <dt>{k}</dt>
              <dd>{typeof v === 'string' ? v : JSON.stringify(v)}</dd>
            </Fragment>
          ))}
        </dl>
      </Section>
    </aside>
  )
}


function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="context__section">
      <h3>{title}</h3>
      {children}
    </section>
  )
}
