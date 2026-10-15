import { useEffect, useMemo, useState } from 'react'
import { X } from 'lucide-react'
import { api, type Note, type Version } from '../api'
import { timeAgo } from './sources'
import { diffLines, diffSummary } from './history'

interface Props {
  note: Note
  /** The note as restored: adopt it without marking the tab dirty. */
  onRestore: (note: Note) => void
  onClose: () => void
  onError: (e: string) => void
}

function fmtTime(iso: string): string {
  const t = new Date(iso)
  return `${t.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' })} ${t.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}`
}

/**
 * Side panel on a note: its saved versions (every change, from every
 * device), what each one changed against the current text, and Restore.
 */
export function HistoryPanel({ note, onRestore, onClose, onError }: Props) {
  const [versions, setVersions] = useState<Version[] | null>(null)
  const [selected, setSelected] = useState<string | null>(null)
  const [old, setOld] = useState<Note | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let cancelled = false
    void api
      .versions(note.id)
      .then((v) => {
        if (cancelled) return
        setVersions(v)
        setSelected((s) => s ?? v[0]?.id ?? null)
      })
      .catch((e) => onError(String(e)))
    return () => {
      cancelled = true
    }
  }, [note.id, note.updated, onError])

  useEffect(() => {
    if (!selected) {
      setOld(null)
      return
    }
    let cancelled = false
    void api
      .versionNote(note.id, selected)
      .then((n) => {
        if (!cancelled) setOld(n)
      })
      .catch((e) => onError(String(e)))
    return () => {
      cancelled = true
    }
  }, [note.id, selected, onError])

  const diff = useMemo(() => (old ? diffLines(old.body, note.body) : []), [old, note.body])
  const titleChanged = old && old.title !== note.title

  const restore = async () => {
    if (!selected) return
    setBusy(true)
    try {
      const n = await api.restoreVersion(note.id, selected)
      onRestore(n)
    } catch (e) {
      onError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <aside className="history" aria-label="Note history">
      <div className="history__head">
        <strong>History</strong>
        <button type="button" className="history__close" onClick={onClose} aria-label="Close history">
          <X size={14} />
        </button>
      </div>
      {versions === null && <p className="history__empty">Loading…</p>}
      {versions && versions.length === 0 && <p className="history__empty">No saved versions yet. One is kept each time the note changes.</p>}
      {versions && versions.length > 0 && (
        <>
          <ul className="history__list">
            {versions.map((v) => (
              <li key={v.id}>
                <button type="button" className={`history__item ${v.id === selected ? 'is-current' : ''}`} onClick={() => setSelected(v.id)} title={fmtTime(v.time)}>
                  <span>{timeAgo(v.time)}</span>
                  <span className="history__device">{v.device}</span>
                </button>
              </li>
            ))}
          </ul>
          {old && (
            <div className="history__detail">
              <div className="history__detail-head">
                <span>
                  {diffSummary(diff)}
                  {titleChanged ? ` · title was “${old.title}”` : ''}
                </span>
                <button type="button" className="history__restore" onClick={() => void restore()} disabled={busy || (diff.every((l) => l.kind === 'same') && !titleChanged)}>
                  {busy ? 'Restoring…' : 'Restore'}
                </button>
              </div>
              <pre className="history__diff">
                {diff.map((l, i) => (
                  <div key={i} className={`history__line history__line--${l.kind}`}>
                    {l.kind === 'add' ? '+ ' : l.kind === 'del' ? '− ' : '  '}
                    {l.text}
                  </div>
                ))}
              </pre>
            </div>
          )}
        </>
      )}
    </aside>
  )
}
