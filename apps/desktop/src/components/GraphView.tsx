import { useEffect, useState } from 'react'
import { Globe, Locate } from 'lucide-react'
import { api, type Graph, type GraphNode, type NoteSummary } from '../api'
import { GraphCanvas } from './GraphCanvas'

export type GraphMode = 'global' | 'local'

interface Props {
  /** The note the local graph is centred on (and highlighted in the global one). */
  current: NoteSummary | null
  mode: GraphMode
  onModeChange: (m: GraphMode) => void
  /** Bumped whenever the vault changes so the graph refetches. */
  version: number
  onOpen: (id: string) => void
  /** A hollow node: a link to a note that does not exist yet. */
  onCreate: (title: string) => void
  onError: (e: string) => void
}

const LEGEND: [string, string][] = [
  ['project', '#3b6ef5'],
  ['idea', '#e0a100'],
  ['area', '#2e9e5b'],
  ['source', '#8a5cf6'],
  ['daily', '#1aa3a3'],
  ['capture', '#9a9ca4'],
]

/** Full-size graph: the whole vault, or the neighbourhood of the open note. */
export function GraphView({ current, mode, onModeChange, version, onOpen, onCreate, onError }: Props) {
  const [graph, setGraph] = useState<Graph>({ nodes: [], edges: [] })
  const [hops, setHops] = useState(1)
  const [filter, setFilter] = useState('')
  const local = mode === 'local' && current !== null

  useEffect(() => {
    let cancelled = false
    const load = local ? api.localGraph(current.id, hops) : api.graph()
    load
      .then((g) => {
        if (!cancelled) setGraph(g)
      })
      .catch((e) => onError(String(e)))
    return () => {
      cancelled = true
    }
  }, [local, current?.id, hops, version, onError])

  const open = (n: GraphNode) => {
    if (n.missing) onCreate(n.title)
    else onOpen(n.id)
  }

  return (
    <div className="graph">
      <div className="graph__toolbar">
        <div className="graph__modes" role="group" aria-label="Graph scope">
          <button type="button" className={`graph__mode ${!local ? 'is-current' : ''}`} onClick={() => onModeChange('global')}>
            <Globe size={14} /> Whole vault
          </button>
          <button type="button" className={`graph__mode ${local ? 'is-current' : ''}`} onClick={() => onModeChange('local')} disabled={!current} title={current ? `Around “${current.title}”` : 'Open a note first'}>
            <Locate size={14} /> Around {current ? `“${current.title.length > 24 ? current.title.slice(0, 23) + '…' : current.title}”` : 'the open note'}
          </button>
        </div>
        {local && (
          <label className="graph__hops">
            Hops
            <select value={hops} onChange={(e) => setHops(Number(e.target.value))} aria-label="Hops">
              {[1, 2, 3].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </label>
        )}
        <input className="graph__filter" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Filter by title" aria-label="Filter nodes" />
        <span className="graph__count">
          {graph.nodes.length} notes · {graph.edges.length} links
        </span>
      </div>
      <GraphCanvas graph={graph} focusId={current?.id ?? null} filter={filter} onOpen={open} className="graph__canvas" />
      <ul className="graph__legend" aria-label="Legend">
        {LEGEND.map(([t, c]) => (
          <li key={t}>
            <span className="graph__dot" style={{ background: c }} /> {t}
          </li>
        ))}
        <li>
          <span className="graph__dot graph__dot--missing" /> not created yet
        </li>
        <li>
          <span className="graph__line" /> typed link
        </li>
      </ul>
      {graph.nodes.length === 0 && <p className="graph__empty">{local ? 'This note has no links yet. Add [[links]] to see them here.' : 'No notes yet.'}</p>}
    </div>
  )
}
