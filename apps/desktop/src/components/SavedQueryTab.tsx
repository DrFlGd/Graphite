import { useCallback, useEffect, useState } from 'react'
import { SquarePen } from 'lucide-react'
import { api, type Note } from '../api'
import { QueryView, isViewKind, type ViewKind } from './QueryView'

interface Props {
  noteId: string
  version: number
  onOpen: (id: string, newTab?: boolean) => void
  onChanged: () => void
  onError: (e: string) => void
  /** Switch this tab to the plain editor. */
  onEdit: () => void
}

/**
 * A `type: query` note shown as its results. The query and the view are the
 * note's `query` and `view` properties, so editing them here writes the
 * note, and the note's own editor shows the same text.
 */
export function SavedQueryTab({ noteId, version, onOpen, onChanged, onError, onEdit }: Props) {
  const [note, setNote] = useState<Note | null>(null)

  useEffect(() => {
    let cancelled = false
    api
      .readNote(noteId)
      .then((n) => {
        if (!cancelled) setNote(n)
      })
      .catch((e) => onError(String(e)))
    return () => {
      cancelled = true
    }
    // Reload when anything changed (the note may have been edited elsewhere).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [noteId, version])

  const save = useCallback(
    async (patch: Record<string, unknown>) => {
      if (!note) return
      const properties = { ...note.properties, ...patch }
      try {
        const saved = await api.saveNote({ ...note, properties })
        setNote(saved)
        onChanged()
      } catch (e) {
        onError(String(e))
      }
    },
    [note, onChanged, onError],
  )

  if (!note) return <div className="app__empty">Loading…</div>
  const query = typeof note.properties.query === 'string' ? note.properties.query : ''
  const view: ViewKind = isViewKind(note.properties.view) ? note.properties.view : 'list'

  return (
    <div className="tasks">
      <header className="tasks__header">
        <h2>{note.title}</h2>
        <button type="button" className="btn" onClick={onEdit} title="Edit the note behind this view">
          <SquarePen size={14} /> Edit note
        </button>
      </header>
      <QueryView query={query} onQueryChange={(q) => void save({ query: q })} view={view} onViewChange={(v) => void save({ view: v })} version={version} onOpen={onOpen} onChanged={onChanged} onError={onError} />
    </div>
  )
}
