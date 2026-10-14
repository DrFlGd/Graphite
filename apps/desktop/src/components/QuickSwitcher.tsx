import { useEffect, useRef, useState } from 'react'
import { FileText, Plus } from 'lucide-react'
import { api, type SearchHit } from '../api'

interface Props {
  onClose: () => void
  onOpen: (id: string) => void
  onCreate: (title: string) => void
}

/** Ctrl+K: full-text search and quick switcher; Enter on no match creates the note. */
export function QuickSwitcher({ onClose, onOpen, onCreate }: Props) {
  const [query, setQuery] = useState('')
  const [hits, setHits] = useState<SearchHit[]>([])
  const [selected, setSelected] = useState(0)
  const input = useRef<HTMLInputElement>(null)

  useEffect(() => {
    input.current?.focus()
  }, [])

  useEffect(() => {
    const q = query.trim()
    if (!q) {
      setHits([])
      return
    }
    let cancelled = false
    const t = window.setTimeout(() => {
      void api.search(q, 20).then((h) => {
        if (!cancelled) {
          setHits(h)
          setSelected(0)
        }
      })
    }, 80)
    return () => {
      cancelled = true
      window.clearTimeout(t)
    }
  }, [query])

  const rows = [...hits.map((h) => ({ kind: 'hit' as const, h })), ...(query.trim() && !hits.some((h) => h.title.toLowerCase() === query.trim().toLowerCase()) ? [{ kind: 'new' as const }] : [])]

  const choose = (i: number) => {
    const row = rows[i]
    if (!row) return
    if (row.kind === 'hit') onOpen(row.h.id)
    else onCreate(query.trim())
    onClose()
  }

  return (
    <div className="modal" onMouseDown={onClose}>
      <div className="modal__card" onMouseDown={(e) => e.stopPropagation()} role="dialog" aria-label="Search notes">
        <input
          ref={input}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') onClose()
            else if (e.key === 'ArrowDown') setSelected((s) => Math.min(s + 1, rows.length - 1))
            else if (e.key === 'ArrowUp') setSelected((s) => Math.max(s - 1, 0))
            else if (e.key === 'Enter') choose(selected)
          }}
          placeholder="Search notes, or type a title to create one"
          aria-label="Search"
          className="modal__input"
        />
        <ul className="modal__list">
          {rows.map((row, i) =>
            row.kind === 'hit' ? (
              <li key={row.h.id}>
                <button type="button" className={`modal__row ${i === selected ? 'is-selected' : ''}`} onMouseEnter={() => setSelected(i)} onClick={() => choose(i)}>
                  <FileText size={16} />
                  <span className="modal__row-text">
                    <span className="modal__row-title">{row.h.title}</span>
                    <span className="modal__row-snippet">{renderSnippet(row.h.snippet)}</span>
                  </span>
                </button>
              </li>
            ) : (
              <li key="new">
                <button type="button" className={`modal__row ${i === selected ? 'is-selected' : ''}`} onMouseEnter={() => setSelected(i)} onClick={() => choose(i)}>
                  <Plus size={16} />
                  <span className="modal__row-text">
                    <span className="modal__row-title">Create “{query.trim()}”</span>
                  </span>
                </button>
              </li>
            ),
          )}
        </ul>
      </div>
    </div>
  )
}

/** The index wraps matches in `[` `]`; show them emphasised. */
function renderSnippet(s: string) {
  const parts = s.split(/(\[[^\]]*\])/g)
  return parts.map((p, i) => (p.startsWith('[') && p.endsWith(']') ? <mark key={i}>{p.slice(1, -1)}</mark> : <span key={i}>{p}</span>))
}
