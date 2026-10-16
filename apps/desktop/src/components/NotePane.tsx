import { useCallback, useLayoutEffect, useRef, useState } from 'react'
import { NoteEditor, ToolButton, type Editor, type NoteCandidate } from '@graphite/ui'
import { FileCode2, History, Trash2 } from 'lucide-react'
import { api, type Note } from '../api'
import { SourceBar } from './SourceBar'
import { HistoryPanel } from './HistoryPanel'

interface Props {
  note: Note
  saveState: 'saved' | 'dirty' | 'saving'
  onChange: (note: Note) => void
  onFollowLink: (target: string) => void
  onOpenFile: (src: string) => void
  onOpenUrl: (href: string) => void
  onDelete: () => void
  resolveSrc?: (src: string) => string
  pickImage: () => Promise<string | null>
  pickFile: () => Promise<{ src: string; name: string } | null>
  onEditorReady?: (editor: Editor) => void
  onFiles?: (files: File[], pos?: number) => void
  /** Adopt a note rewritten by an import without marking it dirty. */
  onReplace?: (note: Note) => void
  onError?: (e: string) => void
}

export function NotePane({ note, saveState, onChange, onFollowLink, onOpenFile, onOpenUrl, onDelete, resolveSrc, pickImage, pickFile, onEditorReady, onFiles, onReplace, onError }: Props) {
  const [raw, setRaw] = useState(false)
  const [history, setHistory] = useState(false)

  const searchNotes = useCallback(async (q: string): Promise<NoteCandidate[]> => {
    const hits = await api.completeTitle(q)
    return hits.map((h) => ({ id: h.id, title: h.title, subtitle: h.path }))
  }, [])

  return (
    <div className="note">
      <header className="note__header">
        <TitleField value={note.title} onChange={(title) => onChange({ ...note, title })} />
        <div className="note__meta">
          <span className="note__path" title={note.path}>
            {note.path}
          </span>
          <span className={`note__save note__save--${saveState}`}>{saveState === 'saved' ? 'Saved' : saveState === 'saving' ? 'Saving…' : 'Unsaved'}</span>
          {onReplace && (
            <ToolButton label="History" active={history} onClick={() => setHistory((h) => !h)}>
              <History size={16} />
            </ToolButton>
          )}
          <ToolButton label="Delete note" onClick={onDelete}>
            <Trash2 size={16} />
          </ToolButton>
        </div>
      </header>
      <TagRow tags={note.tags} onChange={(tags) => onChange({ ...note, tags })} />
      {note.type === 'source' && onReplace && <SourceBar note={note} onReplace={onReplace} onOpenUrl={onOpenUrl} onError={onError ?? (() => {})} />}
      <div className="note__body">
      {raw ? (
        <div className="note__raw">
          <div className="gr-toolbar">
            <div className="gr-toolbar__group gr-toolbar__trailing">
              <ToolButton label="Rich editor" active onClick={() => setRaw(false)}>
                <FileCode2 size={16} />
              </ToolButton>
            </div>
          </div>
          <textarea className="note__raw-text" value={note.body} onChange={(e) => onChange({ ...note, body: e.target.value })} spellCheck aria-label="Markdown source" />
        </div>
      ) : (
        <NoteEditor
          docKey={note.id}
          markdown={note.body}
          title={note.title}
          onChange={(body) => onChange({ ...note, body })}
          onFollowLink={(target) => onFollowLink(target)}
          onOpenFile={onOpenFile}
          onOpenUrl={onOpenUrl}
          searchNotes={searchNotes}
          resolveSrc={resolveSrc}
          blockContext={{ pickImage, pickFile }}
          onReady={onEditorReady}
          onFiles={onFiles}
          autofocus
          toolbarTrailing={
            <ToolButton label="Markdown source" onClick={() => setRaw(true)}>
              <FileCode2 size={16} />
            </ToolButton>
          }
        />
      )}
      {history && onReplace && (
        <HistoryPanel
          note={note}
          onRestore={(n) => onReplace(n)}
          onClose={() => setHistory(false)}
          onError={onError ?? (() => {})}
        />
      )}
      </div>
    </div>
  )
}

function TagRow({ tags, onChange }: { tags: string[]; onChange: (tags: string[]) => void }) {
  const [draft, setDraft] = useState('')
  const add = () => {
    const t = draft.trim().replace(/^#/, '')
    if (t && !tags.includes(t)) onChange([...tags, t])
    setDraft('')
  }
  return (
    <div className="note__tags">
      {tags.map((t) => (
        <button key={t} type="button" className="tag" title="Remove tag" onClick={() => onChange(tags.filter((x) => x !== t))}>
          #{t}
        </button>
      ))}
      <input
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ',') {
            e.preventDefault()
            add()
          } else if (e.key === 'Backspace' && !draft && tags.length) {
            onChange(tags.slice(0, -1))
          }
        }}
        onBlur={add}
        placeholder={tags.length ? '' : 'Add tag'}
        aria-label="Add tag"
        className="note__tag-input"
      />
    </div>
  )
}

/** The note title: one logical line that wraps instead of clipping. */
function TitleField({ value, onChange }: { value: string; onChange: (title: string) => void }) {
  const ref = useRef<HTMLTextAreaElement>(null)
  const fit = () => {
    const el = ref.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${el.scrollHeight}px`
  }
  useLayoutEffect(fit, [value])
  useLayoutEffect(() => {
    const el = ref.current
    if (!el || typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(fit)
    ro.observe(el)
    return () => ro.disconnect()
  }, [])
  return (
    <textarea
      ref={ref}
      className="note__title"
      rows={1}
      value={value}
      onChange={(e) => onChange(e.target.value.replace(/[\r\n]+/g, ' '))}
      onKeyDown={(e) => {
        // Enter moves on to the text rather than adding a line to the title.
        if (e.key === 'Enter') {
          e.preventDefault()
          ;(e.currentTarget.closest('.note')?.querySelector('.ProseMirror') as HTMLElement | null)?.focus()
        }
      }}
      aria-label="Title"
      placeholder="Untitled"
      spellCheck
    />
  )
}
