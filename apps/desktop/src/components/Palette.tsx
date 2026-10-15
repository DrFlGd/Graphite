import { useEffect, useRef, useState, type ReactNode } from 'react'
import { ChevronRight, Download, FileText, Plus } from 'lucide-react'
import { api, type AssetHit, type SearchHit } from '../api'
import { KindIcon } from './FilesView'
import { looksLikeUrl, shortUrl } from './sources'

export interface Command {
  id: string
  label: string
  /** Shown right-aligned, e.g. "Ctrl+D". */
  shortcut?: string
  icon?: ReactNode
  /** Extra words that should match this command. */
  keywords?: string
  run: () => void
}

export type PaletteMode = 'notes' | 'commands'

interface Props {
  mode: PaletteMode
  commands: Command[]
  onClose: () => void
  /** Enter opens in the current tab; Ctrl/Cmd+Enter in a new one. */
  onOpen: (id: string, newTab: boolean) => void
  onOpenFile: (path: string, newTab: boolean) => void
  onCreate: (title: string) => void
  /** A pasted URL: import it as a source. */
  onImport: (url: string) => void
}

type Row = { kind: 'hit'; h: SearchHit } | { kind: 'file'; a: AssetHit } | { kind: 'new' } | { kind: 'import' } | { kind: 'cmd'; c: Command }

/**
 * One box for everything reachable by keyboard: Ctrl+K searches notes (Enter
 * on no match creates one); Ctrl+Shift+P, or a leading `>`, lists commands.
 */
export function Palette({ mode, commands, onClose, onOpen, onOpenFile, onCreate, onImport }: Props) {
  const [query, setQuery] = useState(mode === 'commands' ? '>' : '')
  const [hits, setHits] = useState<SearchHit[]>([])
  const [files, setFiles] = useState<AssetHit[]>([])
  const [selected, setSelected] = useState(0)
  const input = useRef<HTMLInputElement>(null)
  const commandMode = query.startsWith('>')
  const text = (commandMode ? query.slice(1) : query).trim()
  const urlMode = !commandMode && looksLikeUrl(text)

  useEffect(() => {
    input.current?.focus()
  }, [])

  useEffect(() => {
    if (commandMode || urlMode || !text) {
      setHits([])
      setFiles([])
      setSelected(0)
      return
    }
    let cancelled = false
    const t = window.setTimeout(() => {
      void Promise.all([api.search(text, 20), api.searchAssets(text, 8).catch(() => [] as AssetHit[])])
        .then(([h, f]) => {
          if (!cancelled) {
            setHits(h)
            setFiles(f)
            setSelected(0)
          }
        })
        .catch(() => {})
    }, 80)
    return () => {
      cancelled = true
      window.clearTimeout(t)
    }
  }, [text, commandMode, urlMode])

  const rows: Row[] = commandMode
    ? filterCommands(commands, text).map((c) => ({ kind: 'cmd', c }))
    : urlMode
      ? [{ kind: 'import' }]
      : [...hits.map((h) => ({ kind: 'hit' as const, h })), ...files.map((a) => ({ kind: 'file' as const, a })), ...(text && !hits.some((h) => h.title.toLowerCase() === text.toLowerCase()) ? [{ kind: 'new' as const }] : [])]

  const choose = (i: number, newTab = false) => {
    const row = rows[i]
    if (!row) return
    onClose()
    if (row.kind === 'hit') onOpen(row.h.id, newTab)
    else if (row.kind === 'file') onOpenFile(row.a.path, newTab)
    else if (row.kind === 'new') onCreate(text)
    // After the keydown that chose this row, the same key's keypress would submit the
    // import dialog's form the moment it mounts; open it on the next tick instead.
    else if (row.kind === 'import') window.setTimeout(() => onImport(text), 0)
    else row.c.run()
  }

  return (
    <div className="modal" onMouseDown={onClose}>
      <div className="modal__card" onMouseDown={(e) => e.stopPropagation()} role="dialog" aria-label={commandMode ? 'Commands' : 'Search notes'}>
        <input
          ref={input}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value)
            setSelected(0)
          }}
          onKeyDown={(e) => {
            if (e.key === 'Escape') onClose()
            else if (e.key === 'ArrowDown') {
              e.preventDefault()
              setSelected((s) => Math.min(s + 1, rows.length - 1))
            } else if (e.key === 'ArrowUp') {
              e.preventDefault()
              setSelected((s) => Math.max(s - 1, 0))
            } else if (e.key === 'Enter') choose(selected, e.ctrlKey || e.metaKey)
          }}
          placeholder={commandMode ? 'Type a command' : 'Search notes, type a title to create one, paste a link to import it, or > for commands'}
          aria-label={commandMode ? 'Command' : 'Search'}
          className="modal__input"
        />
        <ul className="modal__list">
          {rows.map((row, i) => (
            <li key={row.kind === 'hit' ? row.h.id : row.kind === 'file' ? `file:${row.a.path}` : row.kind === 'cmd' ? row.c.id : row.kind}>
              <button type="button" className={`modal__row ${i === selected ? 'is-selected' : ''}`} onMouseMove={() => setSelected(i)} onClick={(e) => choose(i, e.ctrlKey || e.metaKey)}>
                {row.kind === 'hit' ? <FileText size={16} /> : row.kind === 'file' ? <KindIcon kind={row.a.kind} size={16} /> : row.kind === 'new' ? <Plus size={16} /> : row.kind === 'import' ? <Download size={16} /> : (row.c.icon ?? <ChevronRight size={16} />)}
                <span className="modal__row-text">
                  <span className="modal__row-title">{row.kind === 'hit' ? row.h.title : row.kind === 'file' ? row.a.name : row.kind === 'new' ? `Create “${text}”` : row.kind === 'import' ? `Import ${shortUrl(text)}` : row.c.label}</span>
                  {row.kind === 'hit' && <span className="modal__row-snippet">{renderSnippet(row.h.snippet)}</span>}
                  {row.kind === 'file' && <span className="modal__row-snippet">{row.a.snippet.trim() ? renderSnippet(row.a.snippet) : row.a.path}</span>}
                  {row.kind === 'import' && <span className="modal__row-snippet">Fetch it into the vault as a source: repo, page, video, PDF or file</span>}
                </span>
                {row.kind === 'cmd' && row.c.shortcut && <kbd className="modal__row-key">{row.c.shortcut}</kbd>}
              </button>
            </li>
          ))}
          {commandMode && rows.length === 0 && <li className="modal__none">No command matches “{text}”.</li>}
        </ul>
      </div>
    </div>
  )
}

/** Prefix matches first, then word-start matches, then anything containing the text. */
function filterCommands(commands: Command[], text: string): Command[] {
  const q = text.toLowerCase()
  if (!q) return commands
  const score = (c: Command) => {
    const l = c.label.toLowerCase()
    const k = (c.keywords ?? '').toLowerCase()
    if (l.startsWith(q)) return 0
    if (l.split(/\s+/).some((w) => w.startsWith(q))) return 1
    if (l.includes(q) || k.includes(q)) return 2
    return 9
  }
  return commands
    .map((c) => ({ c, s: score(c) }))
    .filter((x) => x.s < 9)
    .sort((a, b) => a.s - b.s)
    .map((x) => x.c)
}

/** The index wraps matches in `[` `]`; show them emphasised. */
function renderSnippet(s: string) {
  const parts = s.split(/(\[[^\]]*\])/g)
  return parts.map((p, i) => (p.startsWith('[') && p.endsWith(']') ? <mark key={i}>{p.slice(1, -1)}</mark> : <span key={i}>{p}</span>))
}
