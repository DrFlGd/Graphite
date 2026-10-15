import { useCallback, useEffect, useMemo, useState } from 'react'
import { Download, RefreshCw } from 'lucide-react'
import { api, SOURCE_KIND_LABEL, type SourceKind, type SourceRow } from '../api'
import { SourceKindIcon, shortUrl, timeAgo } from './sources'

interface Props {
  /** Bumped when notes change. */
  version: number
  onOpen: (id: string, newTab?: boolean) => void
  onImport: () => void
  onChanged: () => void
  onError: (e: string) => void
}

/**
 * Every imported source: what it is, where it came from, when it was
 * fetched, and whether upstream has changed since. "Check all" asks every
 * source; each row's note has its own check and re-import.
 */
export function SourcesView({ version, onOpen, onImport, onChanged, onError }: Props) {
  const [rows, setRows] = useState<SourceRow[]>([])
  const [kind, setKind] = useState<SourceKind | null>(null)
  const [checking, setChecking] = useState(false)
  const [lastCheck, setLastCheck] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    api
      .sources()
      .then((r) => {
        if (!cancelled) setRows(r)
      })
      .catch((e) => onError(String(e)))
    return () => {
      cancelled = true
    }
  }, [version, onError])

  const checkAll = useCallback(async () => {
    setChecking(true)
    try {
      const checks = await api.checkAllSources()
      const changed = checks.filter((c) => c.changed).length
      setLastCheck(checks.length === 0 ? 'Nothing to check.' : changed === 0 ? `Checked ${checks.length}: all up to date.` : `Checked ${checks.length}: ${changed} changed upstream.`)
      onChanged()
    } catch (e) {
      onError(String(e))
    } finally {
      setChecking(false)
    }
  }, [onChanged, onError])

  const counts = useMemo(() => {
    const c = new Map<SourceKind, number>()
    for (const r of rows) c.set(r.kind, (c.get(r.kind) ?? 0) + 1)
    return c
  }, [rows])
  const shown = rows.filter((r) => !kind || r.kind === kind)
  const updates = rows.filter((r) => r.update_available).length

  return (
    <div className="sources">
      <div className="files__bar">
        <h2>Sources</h2>
        <div className="files__chips">
          <button type="button" className={`chip ${kind === null ? 'is-active' : ''}`} onClick={() => setKind(null)}>
            All <span className="chip__n">{rows.length}</span>
          </button>
          {(Object.keys(SOURCE_KIND_LABEL) as SourceKind[])
            .filter((k) => counts.has(k))
            .map((k) => (
              <button key={k} type="button" className={`chip ${kind === k ? 'is-active' : ''}`} onClick={() => setKind(kind === k ? null : k)}>
                <SourceKindIcon kind={k} size={12} /> {SOURCE_KIND_LABEL[k]} <span className="chip__n">{counts.get(k)}</span>
              </button>
            ))}
        </div>
        <span className="qv__muted sources__status">{lastCheck ?? (updates > 0 ? `${updates} source${updates === 1 ? ' has' : 's have'} changed upstream.` : '')}</span>
        <button type="button" className="btn" onClick={() => void checkAll()} disabled={checking || rows.length === 0} title="Ask every source whether it changed upstream">
          <RefreshCw size={14} className={checking ? 'is-spinning' : ''} /> {checking ? 'Checking…' : 'Check all'}
        </button>
        <button type="button" className="btn btn--primary" onClick={onImport} title="Import a GitHub repo, web page, video, PDF or file from its URL">
          <Download size={14} /> Import from URL
        </button>
      </div>
      {shown.length === 0 && <p className="hub__empty">{rows.length === 0 ? 'Nothing imported yet. Paste a link into the search box (Ctrl+K), or click Import from URL.' : 'Nothing of that kind.'}</p>}
      <ul className="sources__list">
        {shown.map((r) => (
          <li key={r.id}>
            <button type="button" className={`source-row ${r.update_available ? 'has-update' : ''}`} onClick={(e) => onOpen(r.id, e.ctrlKey || e.metaKey)} title={r.url}>
              <span className="source-row__icon">
                <SourceKindIcon kind={r.kind} size={18} />
              </span>
              <span className="source-row__main">
                <span className="source-row__title">{r.title}</span>
                <span className="source-row__url">{shortUrl(r.url)}</span>
              </span>
              <span className="source-row__meta">
                {r.project && <span className="qv__badge">{r.project.replace(/^\[\[|\]\]$/g, '')}</span>}
                {r.update_available ? <span className="qv__badge source-row__badge" title={r.update_summary ?? ''}>changed upstream</span> : <span className="qv__muted">fetched {timeAgo(r.fetched)}</span>}
                <span className="qv__muted">checked {timeAgo(r.last_checked)}</span>
              </span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  )
}
