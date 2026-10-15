import { useState } from 'react'
import { ExternalLink, FolderOpen, RefreshCw, RotateCw } from 'lucide-react'
import { api, SOURCE_KIND_LABEL, type Note, type SourceKind } from '../api'
import { SourceKindIcon, shortUrl, timeAgo } from './sources'

interface Props {
  note: Note
  /** The note as re-imported or re-checked: adopt it without marking the tab dirty. */
  onReplace: (note: Note) => void
  onOpenUrl: (href: string) => void
  onError: (e: string) => void
}

const str = (v: unknown): string | null => (typeof v === 'string' && v ? v : null)

/**
 * Header strip on a `type: source` note: where it came from, when it was
 * fetched, whether upstream changed, and the check / re-import / folder actions.
 */
export function SourceBar({ note, onReplace, onOpenUrl, onError }: Props) {
  const [busy, setBusy] = useState<'check' | 'reimport' | null>(null)
  const p = note.properties
  const url = str(p.url)
  const kind = (str(p.source_type) ?? 'file') as SourceKind
  const folder = str(p.folder)
  const changed = p.update_available === true
  const summary = str(p.update_summary)
  const commit = str(p.commit)

  const check = async () => {
    setBusy('check')
    try {
      await api.checkSource(note.id)
      onReplace(await api.readNote(note.id))
    } catch (e) {
      onError(String(e))
    } finally {
      setBusy(null)
    }
  }

  const reimport = async () => {
    if (!window.confirm(`Fetch "${note.title}" again? Its files and the note text are replaced; your front-matter properties and tags are kept.`)) return
    setBusy('reimport')
    try {
      const report = await api.reimportSource(note.id)
      onReplace(report.note)
    } catch (e) {
      onError(String(e))
    } finally {
      setBusy(null)
    }
  }

  if (!url) return null
  return (
    <div className={`source-bar ${changed ? 'has-update' : ''}`} data-testid="source-bar">
      <span className="source-bar__kind" title={SOURCE_KIND_LABEL[kind] ?? kind}>
        <SourceKindIcon kind={kind} size={14} />
      </span>
      <button type="button" className="source-bar__url" onClick={() => onOpenUrl(url)} title={`Open ${url} in the browser`}>
        {shortUrl(url)} <ExternalLink size={11} />
      </button>
      <span className="source-bar__meta">
        fetched {timeAgo(str(p.fetched))}
        {commit && <span title={commit}> · {commit.slice(0, 7)}</span>}
        {changed ? <span className="source-bar__changed" title={summary ?? ''}> · changed upstream{summary ? `: ${summary}` : ''}</span> : summary ? <span className="qv__muted" title={`Checked ${timeAgo(str(p.last_checked))}`}> · {summary}</span> : null}
      </span>
      <span className="source-bar__actions">
        <button type="button" className="btn" onClick={() => void check()} disabled={busy !== null} title="Compare with upstream">
          <RefreshCw size={13} className={busy === 'check' ? 'is-spinning' : ''} /> {busy === 'check' ? 'Checking…' : 'Check for updates'}
        </button>
        <button type="button" className={`btn ${changed ? 'btn--primary' : ''}`} onClick={() => void reimport()} disabled={busy !== null} title="Fetch again and replace the files and note text">
          <RotateCw size={13} className={busy === 'reimport' ? 'is-spinning' : ''} /> {busy === 'reimport' ? 'Importing…' : 'Re-import'}
        </button>
        {folder && (
          <button type="button" className="icon-btn" onClick={() => void api.openAsset(folder).catch((e) => onError(String(e)))} title={`Open ${folder}/ in the file manager`}>
            <FolderOpen size={15} />
          </button>
        )}
      </span>
    </div>
  )
}
