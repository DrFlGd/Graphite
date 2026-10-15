import { useCallback, useEffect, useState } from 'react'
import { FilePlus2, SquarePen } from 'lucide-react'
import { api, type ItemRow, type ProjectHub as Hub } from '../api'
import { QueryView, type ViewKind } from './QueryView'
import { FilesView } from './FilesView'

interface Props {
  noteId: string
  version: number
  vaultRoot: string
  /** Bumped when files change. */
  assetVersion: number
  onOpenFile: (path: string, newTab?: boolean) => void
  onFilesChanged: () => void
  onOpen: (id: string, newTab?: boolean) => void
  onChanged: () => void
  onError: (e: string) => void
  /** Switch this tab to the note's editor. */
  onEdit: () => void
}

/**
 * A `type: project` note shown as its hub: progress, tasks (any view),
 * open questions, decisions, sources and linked notes, all gathered from
 * the index. The note's own text is one click away.
 */
export function ProjectHub({ noteId, version, vaultRoot, assetVersion, onOpenFile, onFilesChanged, onOpen, onChanged, onError, onEdit }: Props) {
  const [hub, setHub] = useState<Hub | null>(null)
  const [query, setQuery] = useState<string | null>(null)
  const [view, setView] = useState<ViewKind>('tree')

  useEffect(() => {
    let cancelled = false
    api
      .projectHub(noteId)
      .then((h) => {
        if (cancelled) return
        setHub(h)
        setQuery((q) => q ?? `type:task project:[[${h.project.title}]] status:open`)
      })
      .catch((e) => onError(String(e)))
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [noteId, version])

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

  const addGap = async () => {
    const text = window.prompt('What needs finding out?')
    if (text?.trim()) await act(() => api.addItem(noteId, 'gap', text.trim()))
  }

  const newDecision = async () => {
    if (!hub) return
    const title = window.prompt('Decision title')
    if (!title?.trim()) return
    try {
      const note = await api.createFromTemplate(title.trim(), 'decision')
      await api.saveNote({ ...note, properties: { ...note.properties, project: `[[${hub.project.title}]]` } })
      onChanged()
      onOpen(note.id)
    } catch (e) {
      onError(String(e))
    }
  }

  if (!hub || query === null) return <div className="app__empty">Loading…</div>
  const p = hub.progress
  const openGaps = hub.gaps.filter((g) => g.status !== 'answered')
  const answered = hub.gaps.filter((g) => g.status === 'answered')

  return (
    <div className="hub">
      <header className="hub__header">
        <div className="hub__title">
          <h2>{hub.project.title}</h2>
          <span className="hub__type">project</span>
        </div>
        <div className="hub__progress" title={`${p.done} done, ${p.doing} in progress, ${p.open} open, ${p.cancelled} cancelled`}>
          <div className="hub__bar">
            <div className="hub__bar-fill" style={{ width: `${p.percent}%` }} />
          </div>
          <span>
            {p.percent}% · {p.done} of {p.tasks - p.cancelled} tasks done{p.doing > 0 ? `, ${p.doing} in progress` : ''}
            {p.gaps_open > 0 ? ` · ${p.gaps_open} open question${p.gaps_open === 1 ? '' : 's'}` : ''}
          </span>
        </div>
        <button type="button" className="btn" onClick={onEdit} title="Edit the project note">
          <SquarePen size={14} /> Edit note
        </button>
      </header>

      <section className="hub__section hub__section--tasks">
        <h3>Tasks</h3>
        <QueryView query={query} onQueryChange={setQuery} view={view} onViewChange={setView} version={version} onOpen={onOpen} onChanged={onChanged} onError={onError} />
      </section>

      <div className="hub__columns">
        <section className="hub__section">
          <div className="hub__section-head">
            <h3>Open questions {openGaps.length > 0 && <span className="qv__muted">{openGaps.length}</span>}</h3>
            <button type="button" className="icon-btn" title="Add an open question" onClick={() => void addGap()}>
              <FilePlus2 size={14} />
            </button>
          </div>
          {openGaps.length === 0 && <p className="hub__empty">Nothing waiting on an answer.</p>}
          {openGaps.map((g) => (
            <GapLine key={`${g.note_id}:${g.line}`} gap={g} onOpen={onOpen} act={act} />
          ))}
          {answered.length > 0 && (
            <details className="hub__answered">
              <summary>{answered.length} answered</summary>
              {answered.map((g) => (
                <GapLine key={`${g.note_id}:${g.line}`} gap={g} onOpen={onOpen} act={act} />
              ))}
            </details>
          )}
        </section>

        <section className="hub__section">
          <div className="hub__section-head">
            <h3>Decisions {hub.decisions.length > 0 && <span className="qv__muted">{hub.decisions.length}</span>}</h3>
            <button type="button" className="icon-btn" title="Record a decision" onClick={() => void newDecision()}>
              <FilePlus2 size={14} />
            </button>
          </div>
          {hub.decisions.length === 0 && <p className="hub__empty">No decisions recorded yet.</p>}
          {hub.decisions.map((d) => (
            <button key={d.id} type="button" className="context__item" onClick={(e) => onOpen(d.id, e.ctrlKey || e.metaKey)}>
              <span>{d.title}</span>
            </button>
          ))}
        </section>

        <section className="hub__section">
          <h3>Sources {hub.sources.length > 0 && <span className="qv__muted">{hub.sources.length}</span>}</h3>
          {hub.sources.length === 0 && <p className="hub__empty">Sources (imported pages, repos, videos) arrive in M3. Notes of type source linked here will show up.</p>}
          {hub.sources.map((s) => (
            <button key={s.id} type="button" className="context__item" onClick={(e) => onOpen(s.id, e.ctrlKey || e.metaKey)}>
              <span>{s.title}</span>
            </button>
          ))}
        </section>

        <section className="hub__section hub__section--files">
          <h3>Files</h3>
          <p className="hub__hint">
            The project's own folder, <code>attachments/{hub.project.title}/</code>. Files anywhere else can still be linked from its notes.
          </p>
          <FilesView vaultRoot={vaultRoot} version={assetVersion} folder={`attachments/${hub.project.title}`} compact onOpen={onOpenFile} onChanged={onFilesChanged} onError={onError} />
        </section>

        <section className="hub__section">
          <h3>Linked notes {hub.linked.length > 0 && <span className="qv__muted">{hub.linked.length}</span>}</h3>
          {hub.linked.length === 0 && <p className="hub__empty">Nothing links here yet. Write [[{hub.project.title}]] in any note.</p>}
          {hub.linked.map((l) => (
            <button key={l.id} type="button" className="context__item" onClick={(e) => onOpen(l.id, e.ctrlKey || e.metaKey)} title={l.incoming ? 'Links to this project' : 'Linked from this project'}>
              <span>
                {l.title}
                {l.type !== 'note' && <span className="sidebar__note-type"> {l.type}</span>}
              </span>
              <span className="context__rel">{l.relation ?? (l.incoming ? '←' : '→')}</span>
            </button>
          ))}
        </section>
      </div>
    </div>
  )
}

function GapLine({ gap, onOpen, act }: { gap: ItemRow; onOpen: (id: string) => void; act: (f: () => Promise<unknown>) => Promise<void> }) {
  const answered = gap.status === 'answered'
  return (
    <div className={`qv__item qv__item--${gap.status} qv__item--gap`}>
      <button
        type="button"
        className={`qv__gap ${answered ? 'is-answered' : ''}`}
        title={answered ? `Answered in ${gap.answer ?? ''}` : 'Record which note answers this'}
        onClick={() => {
          if (answered) return
          const target = window.prompt('Which note answers this? (title)')
          if (target?.trim()) void act(() => api.answerGap(gap.note_id, gap.line, target.trim()))
        }}
      >
        {answered ? '✓' : '?'}
      </button>
      <button type="button" className="qv__item-text" onClick={() => onOpen(gap.note_id)} title={`In ${gap.note_title}`}>
        {gap.text}
      </button>
      <span className="qv__badges">
        {gap.fields.blocks && <span className="qv__badge">blocks {gap.fields.blocks}</span>}
        {answered && gap.answer && <span className="qv__badge">→ {gap.answer}</span>}
        <span className="qv__badge qv__muted">{gap.note_title}</span>
      </span>
    </div>
  )
}
