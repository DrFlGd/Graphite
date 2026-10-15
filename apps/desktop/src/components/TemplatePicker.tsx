import { useEffect, useRef, useState } from 'react'
import { api, type Template } from '../api'

interface Props {
  /** `create` asks for a title; `promote` turns the named note into a project. */
  mode: { kind: 'create' } | { kind: 'promote'; noteId: string; title: string }
  onClose: () => void
  onDone: (noteId: string) => void
  onError: (e: string) => void
}

/** Pick a template (built-in or from the vault's templates/ folder). */
export function TemplatePicker({ mode, onClose, onDone, onError }: Props) {
  const [templates, setTemplates] = useState<Template[]>([])
  const [selected, setSelected] = useState<string>('')
  const [title, setTitle] = useState('')
  const [busy, setBusy] = useState(false)
  const input = useRef<HTMLInputElement>(null)

  useEffect(() => {
    api
      .templates()
      .then((list) => {
        const usable = mode.kind === 'promote' ? list.filter((t) => t.note_type === 'project') : list
        setTemplates(usable)
        setSelected(usable.find((t) => t.name === 'project')?.name ?? usable[0]?.name ?? '')
      })
      .catch((e) => onError(String(e)))
    input.current?.focus()
  }, [mode.kind, onError])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const go = async () => {
    if (!selected || busy) return
    setBusy(true)
    try {
      const note = mode.kind === 'create' ? await api.createFromTemplate(title.trim() || 'Untitled', selected) : await api.promoteToProject(mode.noteId, selected)
      onDone(note.id)
    } catch (e) {
      onError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const current = templates.find((t) => t.name === selected)
  return (
    <div className="modal" onMouseDown={onClose}>
      <div className="modal__card picker" onMouseDown={(e) => e.stopPropagation()} role="dialog" aria-label={mode.kind === 'create' ? 'New note from template' : 'Promote to project'}>
        <form
          onSubmit={(e) => {
            e.preventDefault()
            void go()
          }}
        >
          {mode.kind === 'create' ? (
            <input ref={input} className="modal__input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Title of the new note" aria-label="Title" />
          ) : (
            <p className="picker__intro">
              Turn <strong>{mode.title}</strong> into a project. Its text is kept and the template's sections are added around it.
            </p>
          )}
          <ul className="picker__list" role="listbox">
            {templates.map((t) => (
              <li key={t.name}>
                <button type="button" role="option" aria-selected={t.name === selected} className={`picker__item ${t.name === selected ? 'is-selected' : ''}`} onClick={() => setSelected(t.name)} onDoubleClick={() => void go()}>
                  <span className="picker__name">
                    {t.name}
                    <span className="sidebar__note-type">{t.note_type}</span>
                    {!t.builtin && <span className="sidebar__note-type">vault</span>}
                  </span>
                  <span className="picker__desc">{t.description}</span>
                </button>
              </li>
            ))}
          </ul>
          <div className="picker__footer">
            <span className="qv__muted">{current ? `Creates in ${current.folder}/` : ''}</span>
            <button type="button" className="btn" onClick={onClose}>
              Cancel
            </button>
            <button type="submit" className="btn btn--primary" disabled={!selected || busy}>
              {mode.kind === 'create' ? 'Create' : 'Promote'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
