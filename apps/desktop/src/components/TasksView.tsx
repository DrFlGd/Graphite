import { useEffect, useMemo, useState } from 'react'
import { Plus } from 'lucide-react'
import { api, type NoteSummary } from '../api'
import { termValue, withTerm } from '../query'
import { QueryView, isViewKind, type ViewKind } from './QueryView'

interface Props {
  notes: NoteSummary[]
  version: number
  onOpen: (id: string, newTab?: boolean) => void
  onFollowLink: (target: string) => void
  onChanged: () => void
  onError: (e: string) => void
  onSaveQuery: (query: string, view: ViewKind) => void
}

const QUERY_KEY = 'graphite.tasks.query'
const VIEW_KEY = 'graphite.tasks.view'
const DEFAULT_QUERY = 'type:task status:open sort:due,priority'

const stored = (key: string, fallback: string) => {
  try {
    return localStorage.getItem(key) ?? fallback
  } catch {
    return fallback
  }
}
const store = (key: string, value: string) => {
  try {
    localStorage.setItem(key, value)
  } catch {
    /* ignore */
  }
}

/**
 * The task tracker: every `- [ ]` in the vault, filtered by project and
 * state, in any of the query views. The query line is editable for anything
 * the controls do not cover.
 */
export function TasksView({ notes, version, onOpen, onFollowLink, onChanged, onError, onSaveQuery }: Props) {
  const [query, setQuery] = useState(() => stored(QUERY_KEY, DEFAULT_QUERY))
  const [view, setView] = useState<ViewKind>(() => {
    const v = stored(VIEW_KEY, 'tree')
    return isViewKind(v) ? v : 'tree'
  })
  const [newText, setNewText] = useState('')
  useEffect(() => store(QUERY_KEY, query), [query])
  useEffect(() => store(VIEW_KEY, view), [view])

  const projects = useMemo(() => notes.filter((n) => n.type === 'project').sort((a, b) => a.title.localeCompare(b.title)), [notes])
  const projectTerm = termValue(query, 'project')
  const projectTitle = projectTerm?.replace(/^\[\[|\]\]$/g, '') ?? ''
  const kind = termValue(query, 'type') ?? 'task'
  const status = termValue(query, 'status') ?? ''

  const add = async () => {
    const text = newText.trim()
    if (!text) return
    try {
      const project = projects.find((p) => p.title === projectTitle)
      const target = project ? project.id : (await api.dailyNote()).id
      await api.addItem(target, kind === 'gap' ? 'gap' : 'task', text)
      setNewText('')
      onChanged()
    } catch (e) {
      onError(String(e))
    }
  }

  const controls = (
    <>
      <select value={kind} onChange={(e) => setQuery(withTerm(e.target.value === 'gap' ? withTerm(query, 'status', status === 'open' ? 'open' : null) : query, 'type', e.target.value))} aria-label="Kind">
        <option value="task">Tasks</option>
        <option value="gap">Open questions</option>
        <option value="item">Both</option>
      </select>
      <select value={status} onChange={(e) => setQuery(withTerm(query, 'status', e.target.value || null))} aria-label="State">
        <option value="open">Open</option>
        <option value="doing">In progress</option>
        <option value="done">Done</option>
        <option value="">All</option>
      </select>
      <select value={projectTitle} onChange={(e) => setQuery(withTerm(query, 'project', e.target.value ? `[[${e.target.value}]]` : null))} aria-label="Project">
        <option value="">Every project</option>
        {projects.map((p) => (
          <option key={p.id} value={p.title}>
            {p.title}
          </option>
        ))}
        {projectTitle && !projects.some((p) => p.title === projectTitle) && <option value={projectTitle}>{projectTitle}</option>}
      </select>
    </>
  )

  return (
    <div className="tasks">
      <header className="tasks__header">
        <h2>Tasks</h2>
        <form
          className="tasks__add"
          onSubmit={(e) => {
            e.preventDefault()
            void add()
          }}
        >
          <input value={newText} onChange={(e) => setNewText(e.target.value)} placeholder={projectTitle ? `Add a ${kind === 'gap' ? 'question' : 'task'} to ${projectTitle}…` : `Add a ${kind === 'gap' ? 'question' : 'task'} to today's note…`} aria-label="New task" />
          <button type="submit" className="icon-btn" title="Add" disabled={!newText.trim()}>
            <Plus size={16} />
          </button>
        </form>
      </header>
      <QueryView query={query} onQueryChange={setQuery} view={view} onViewChange={setView} version={version} onOpen={onOpen} onFollowLink={onFollowLink} onChanged={onChanged} onError={onError} onSave={onSaveQuery} extra={controls} />
    </div>
  )
}
