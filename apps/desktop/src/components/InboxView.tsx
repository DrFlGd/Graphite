import { useEffect, useState } from 'react'
import { Download, ExternalLink, FolderInput, Trash2 } from 'lucide-react'
import { api, INBOX_DIR, type Note, type NoteSummary } from '../api'
import { firstUrl } from './sources'

interface Props {
  notes: NoteSummary[]
  onOpen: (id: string) => void
  /** A capture that is a link: import it as a source (the capture goes once that succeeds). */
  onImport: (url: string, captureId: string) => void
  onChanged: () => void
  onError: (e: string) => void
}

const TYPES = ['note', 'idea', 'project', 'area', 'source', 'capture']

/** Inbox triage: read each capture, then file it (move + type) or drop it. */
export function InboxView({ notes, onOpen, onImport, onChanged, onError }: Props) {
  const inbox = notes.filter((n) => n.path.startsWith(INBOX_DIR + '/'))
  const [folders, setFolders] = useState<string[]>([])
  const [previews, setPreviews] = useState<Record<string, string>>({})
  const [newFolder, setNewFolder] = useState<Record<string, string>>({})

  useEffect(() => {
    void api.folders().then(setFolders).catch(() => {})
  }, [notes])

  useEffect(() => {
    let cancelled = false
    void (async () => {
      const out: Record<string, string> = {}
      for (const n of inbox.slice(0, 50)) {
        if (previews[n.id] !== undefined) {
          out[n.id] = previews[n.id]
          continue
        }
        try {
          const note: Note = await api.readNote(n.id)
          out[n.id] = note.body.replace(/\s+/g, ' ').trim().slice(0, 240)
        } catch {
          out[n.id] = ''
        }
      }
      if (!cancelled) setPreviews(out)
    })()
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [notes])

  const act = async (f: () => Promise<unknown>) => {
    try {
      await f()
      onChanged()
    } catch (e) {
      onError(String(e))
    }
  }

  return (
    <div className="inbox">
      <header className="inbox__header">
        <h2>Inbox</h2>
        <p>{inbox.length === 0 ? 'Nothing to triage. Press Ctrl+Shift+Space anywhere to capture.' : `${inbox.length} to file`}</p>
      </header>
      <ul className="inbox__list">
        {inbox.map((n) => {
          const target = newFolder[n.id] ?? ''
          const url = previews[n.id] ? firstUrl(previews[n.id]) : null
          return (
            <li key={n.id} className="inbox__item">
              <div className="inbox__main">
                <button type="button" className="inbox__title" onClick={() => onOpen(n.id)}>
                  {n.title}
                </button>
                <p className="inbox__preview">{previews[n.id] || <em>empty</em>}</p>
              </div>
              <div className="inbox__actions">
                <label>
                  Type
                  <select value={n.type} onChange={(e) => void act(() => api.setNoteType(n.id, e.target.value))} aria-label="Note type">
                    {(TYPES.includes(n.type) ? TYPES : [n.type, ...TYPES]).map((t) => (
                      <option key={t} value={t}>
                        {t}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Move to
                  <input
                    list={`folders-${n.id}`}
                    value={target}
                    onChange={(e) => setNewFolder((m) => ({ ...m, [n.id]: e.target.value }))}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && target.trim()) void act(() => api.moveNote(n.id, target.trim()))
                    }}
                    placeholder="folder"
                    aria-label="Destination folder"
                  />
                  <datalist id={`folders-${n.id}`}>
                    {folders
                      .filter((f) => f !== INBOX_DIR)
                      .map((f) => (
                        <option key={f} value={f} />
                      ))}
                  </datalist>
                </label>
                <button type="button" className="icon-btn" title="Move" disabled={!target.trim()} onClick={() => void act(() => api.moveNote(n.id, target.trim()))}>
                  <FolderInput size={16} />
                </button>
                {url && (
                  <button type="button" className="btn inbox__import" title={`Import ${url} as a source`} onClick={() => onImport(url, n.id)}>
                    <Download size={14} /> Import
                  </button>
                )}
                <button type="button" className="icon-btn" title="Open" onClick={() => onOpen(n.id)}>
                  <ExternalLink size={16} />
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  title="Delete"
                  onClick={() => {
                    if (window.confirm(`Delete "${n.title}"?`)) void act(() => api.deleteNote(n.id))
                  }}
                >
                  <Trash2 size={16} />
                </button>
              </div>
            </li>
          )
        })}
      </ul>
    </div>
  )
}
