import { useCallback, useEffect, useState } from 'react'
import { Download, FilePlus2, SquarePen } from 'lucide-react'
import { NoteEditor } from '@graphite/ui'
import { api, assetUrl, type ItemRow, type Note, type ProjectHub as Hub, type SourceRow } from '../api'
import { SourceKindIcon, shortUrl } from './sources'
import { ItemText, QueryView, type ViewKind } from './QueryView'
import { FilesView } from './FilesView'
import { TypeChip } from '../noteTypes'

interface Props {
  noteId: string
  version: number
  vaultRoot: string
  /** Bumped when files change. */
  assetVersion: number
  onOpenFile: (path: string, newTab?: boolean) => void
  onFilesChanged: () => void
  /** Open the import dialog with this project preselected. */
  onImport: (project: string) => void
  onOpen: (id: string, newTab?: boolean) => void
  onFollowLink: (target: string) => void
  onOpenUrl: (href: string) => void
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
export function ProjectHub({ noteId, version, vaultRoot, assetVersion, onOpenFile, onFilesChanged, onImport, onOpen, onFollowLink, onOpenUrl, onChanged, onError, onEdit }: Props) {
  const [hub, setHub] = useState<Hub | null>(null)
  const [note, setNote] = useState<Note | null>(null)
  const [sources, setSources] = useState<Map<string, SourceRow>>(() => new Map())
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
    api
      .readNote(noteId)
      .then((n) => {
        if (!cancelled) setNote(n)
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [noteId, version])

  // Source records (url, update flag) for the Sources section.
  useEffect(() => {
    let cancelled = false
    api
      .sources()
      .then((rows) => {
        if (!cancelled) setSources(new Map(rows.map((r) => [r.id, r])))
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [version])

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
          <TypeChip type="project" icon />
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

      {note && hasText(note) && (
        <details className="hub__about" open={note.body.length < 1500}>
          <summary>About this project</summary>
          <NoteEditor
            docKey={`${note.id}:${note.updated ?? ''}`}
            markdown={note.body}
            title={note.title}
            editable={false}
            toolbar={false}
            onChange={() => {}}
            onFollowLink={(t) => onFollowLink(t)}
            onOpenFile={(src) => onOpenFile(src)}
            onOpenUrl={(href) => onOpenUrl(href)}
            resolveSrc={(src) => assetUrl(vaultRoot, src)}
          />
        </details>
      )}

      <section className="hub__section hub__section--tasks">
        <h3>Tasks</h3>
        <QueryView query={query} onQueryChange={setQuery} view={view} onViewChange={setView} version={version} onOpen={onOpen} onFollowLink={onFollowLink} onChanged={onChanged} onError={onError} />
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
            <GapLine key={`${g.note_id}:${g.line}`} gap={g} onOpen={onOpen} onFollow={onFollowLink} act={act} />
          ))}
          {answered.length > 0 && (
            <details className="hub__answered">
              <summary>{answered.length} answered</summary>
              {answered.map((g) => (
                <GapLine key={`${g.note_id}:${g.line}`} gap={g} onOpen={onOpen} onFollow={onFollowLink} act={act} />
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
          <div className="hub__section-head">
            <h3>Sources {hub.sources.length > 0 && <span className="qv__muted">{hub.sources.length}</span>}</h3>
            <button type="button" className="icon-btn" title="Import a repo, page, video, PDF or file from a URL into this project" onClick={() => onImport(hub.project.title)}>
              <Download size={14} />
            </button>
          </div>
          {hub.sources.length === 0 && <p className="hub__empty">Nothing imported yet. Paste a GitHub, article, video or PDF link with the import button, and it lands here.</p>}
          {hub.sources.map((s) => {
            const r = sources.get(s.id)
            return (
              <button key={s.id} type="button" className={`context__item ${r?.update_available ? 'has-update' : ''}`} onClick={(e) => onOpen(s.id, e.ctrlKey || e.metaKey)} title={r ? r.url : s.path}>
                <span className="hub__source">
                  {r && <SourceKindIcon kind={r.kind} size={13} />}
                  <span>{s.title}</span>
                </span>
                {r?.update_available ? <span className="qv__badge source-row__badge">changed</span> : r ? <span className="context__rel">{shortUrl(r.url).split('/')[0]}</span> : null}
              </button>
            )
          })}
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
                {l.type !== 'note' && <TypeChip type={l.type} />}
              </span>
              <span className="context__rel">{l.relation ?? (l.incoming ? '←' : '→')}</span>
            </button>
          ))}
        </section>
      </div>
    </div>
  )
}

/** Whether the project note has text beyond a heading repeating its title. */
function hasText(note: Note): boolean {
  const lines = note.body.trim().split('\n')
  if (lines[0]?.replace(/^#\s+/, '').trim().toLowerCase() === note.title.trim().toLowerCase()) lines.shift()
  return lines.join('\n').trim().length > 0
}

function GapLine({ gap, onOpen, onFollow, act }: { gap: ItemRow; onOpen: (id: string) => void; onFollow: (target: string) => void; act: (f: () => Promise<unknown>) => Promise<void> }) {
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
        <ItemText text={gap.text} onFollow={onFollow} />
      </button>
      <span className="qv__badges">
        {gap.fields.blocks && <span className="qv__badge">blocks {gap.fields.blocks}</span>}
        {answered && gap.answer && <span className="qv__badge">→ {gap.answer}</span>}
        <span className="qv__badge qv__muted">{gap.note_title}</span>
      </span>
    </div>
  )
}
