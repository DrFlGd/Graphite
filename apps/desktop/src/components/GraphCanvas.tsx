import { useEffect, useRef } from 'react'
import { forceCenter, forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY, type Simulation, type SimulationLinkDatum, type SimulationNodeDatum } from 'd3-force'
import type { Graph, GraphEdge, GraphNode } from '../api'
import { TYPE_COLOURS } from '../noteTypes'

interface Props {
  graph: Graph
  /** Drawn larger and never dimmed. */
  focusId?: string | null
  /** Only nodes whose title contains this (and their edges) are drawn. */
  filter?: string
  onOpen: (node: GraphNode) => void
  /** Small embedded version: tighter layout, labels always on. */
  compact?: boolean
  /** Screen pixels at the bottom covered by something else (the legend). */
  insetBottom?: number
  className?: string
}

type SimNode = SimulationNodeDatum & { id: string; node: GraphNode; degree: number; r: number }
type SimLink = SimulationLinkDatum<SimNode> & { edge: GraphEdge }

interface View {
  x: number
  y: number
  k: number
}


const DRAG_THRESHOLD = 4

/**
 * Force-directed link graph on a canvas: pan (drag), zoom (wheel), drag
 * nodes, hover to highlight neighbours, click to open. Positions survive
 * data refreshes so the picture does not jump when a note is saved.
 */
export function GraphCanvas({ graph, focusId, filter, onOpen, compact = false, insetBottom = 0, className }: Props) {
  const host = useRef<HTMLDivElement>(null)
  const canvas = useRef<HTMLCanvasElement>(null)
  const state = useRef<{
    sim: Simulation<SimNode, SimLink> | null
    nodes: SimNode[]
    links: SimLink[]
    view: View
    hover: SimNode | null
    raf: number
    dirty: boolean
    size: { w: number; h: number }
    settled: boolean
    /** The user panned, zoomed or dragged: stop fitting the view for them. */
    userMoved: boolean
  }>({ sim: null, nodes: [], links: [], view: { x: 0, y: 0, k: 1 }, hover: null, raf: 0, dirty: true, size: { w: 0, h: 0 }, settled: false, userMoved: false })
  const latest = useRef({ onOpen, focusId, filter, compact, insetBottom })
  latest.current = { onOpen, focusId, filter, compact, insetBottom }

  // Build or update the simulation when the data changes.
  useEffect(() => {
    const st = state.current
    const prev = new Map(st.nodes.map((n) => [n.id, n]))
    const degree = new Map<string, number>()
    for (const e of graph.edges) {
      degree.set(e.from, (degree.get(e.from) ?? 0) + 1)
      degree.set(e.to, (degree.get(e.to) ?? 0) + 1)
    }
    const { w, h } = st.size
    const nodes: SimNode[] = graph.nodes.map((n) => {
      const d = degree.get(n.id) ?? 0
      const r = (compact ? 3 : 4) + Math.min(10, Math.sqrt(d) * (compact ? 1.2 : 2))
      const old = prev.get(n.id)
      if (old) return Object.assign(old, { node: n, degree: d, r })
      return { id: n.id, node: n, degree: d, r, x: w / 2 + (Math.random() - 0.5) * 60, y: h / 2 + (Math.random() - 0.5) * 60 }
    })
    const byId = new Map(nodes.map((n) => [n.id, n]))
    const links: SimLink[] = graph.edges.filter((e) => byId.has(e.from) && byId.has(e.to)).map((e) => ({ source: e.from, target: e.to, edge: e }))
    st.nodes = nodes
    st.links = links
    st.sim?.stop()
    const sim = forceSimulation<SimNode>(nodes)
      .force(
        'link',
        forceLink<SimNode, SimLink>(links)
          .id((d) => d.id)
          .distance(compact ? 46 : 70)
          .strength(0.6),
      )
      .force('charge', forceManyBody<SimNode>().strength(compact ? -90 : -180).distanceMax(600))
      .force('center', forceCenter(w / 2, h / 2).strength(0.05))
      // Weak pull to the middle so notes without links do not drift off.
      .force('x', forceX<SimNode>(w / 2).strength(0.06))
      .force('y', forceY<SimNode>(h / 2).strength(0.06))
      .force('collide', forceCollide<SimNode>().radius((d) => d.r + 3))
      .alpha(prev.size ? 0.5 : 1)
      .alphaDecay(0.03)
      .on('tick', () => {
        // Keep everything in view while the layout settles.
        if (!st.userMoved) fit(st, latest.current.compact, latest.current.insetBottom)
        st.dirty = true
      })
      .on('end', () => {
        if (!st.userMoved) fit(st, latest.current.compact, latest.current.insetBottom)
        st.settled = true
        st.dirty = true
      })
    st.sim = sim
    st.settled = false
    st.dirty = true
    return () => {
      sim.stop()
    }
  }, [graph, compact])

  // A new focus or size of graph: fit it again until the user moves it.
  useEffect(() => {
    const st = state.current
    st.userMoved = false
    fit(st, compact, insetBottom)
    st.dirty = true
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusId, compact])

  // Canvas sizing, drawing loop, and pointer handling.
  useEffect(() => {
    const el = host.current
    const cv = canvas.current
    if (!el || !cv) return
    const st = state.current
    const ctx = cv.getContext('2d')
    if (!ctx) return

    const resize = () => {
      const rect = el.getBoundingClientRect()
      const dpr = window.devicePixelRatio || 1
      st.size = { w: rect.width, h: rect.height }
      cv.width = Math.max(1, Math.round(rect.width * dpr))
      cv.height = Math.max(1, Math.round(rect.height * dpr))
      cv.style.width = `${rect.width}px`
      cv.style.height = `${rect.height}px`
      st.sim?.force('center', forceCenter(rect.width / 2, rect.height / 2).strength(0.05))
      st.sim?.force('x', forceX<SimNode>(rect.width / 2).strength(0.06))
      st.sim?.force('y', forceY<SimNode>(rect.height / 2).strength(0.06))
      if (!st.userMoved) fit(st, latest.current.compact, latest.current.insetBottom)
      st.dirty = true
    }
    resize()
    const ro = new ResizeObserver(resize)
    ro.observe(el)

    const colours = () => {
      const cs = getComputedStyle(el)
      const v = (name: string, fallback: string) => cs.getPropertyValue(name).trim() || fallback
      return { fg: v('--gr-fg', '#1f2023'), muted: v('--gr-fg-muted', '#6b6e76'), border: v('--gr-border', '#d0d0cc'), accent: v('--gr-accent', '#3b6ef5'), bg: v('--gr-bg-elev', '#fff') }
    }

    const draw = () => {
      st.raf = requestAnimationFrame(draw)
      if (!st.dirty) return
      st.dirty = false
      const { w, h } = st.size
      const dpr = window.devicePixelRatio || 1
      const c = colours()
      const { x: tx, y: ty, k } = st.view
      const { focusId: focus, filter: f, compact: small } = latest.current
      const q = (f ?? '').trim().toLowerCase()
      const visible = new Set<string>()
      for (const n of st.nodes) if (!q || n.node.title.toLowerCase().includes(q)) visible.add(n.id)
      const hover = st.hover
      const neighbours = new Set<string>()
      if (hover) {
        neighbours.add(hover.id)
        for (const l of st.links) {
          const s = l.source as SimNode
          const t = l.target as SimNode
          if (s.id === hover.id) neighbours.add(t.id)
          if (t.id === hover.id) neighbours.add(s.id)
        }
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
      ctx.clearRect(0, 0, w, h)
      ctx.translate(tx, ty)
      ctx.scale(k, k)

      // Edges
      ctx.lineCap = 'round'
      for (const l of st.links) {
        const s = l.source as SimNode
        const t = l.target as SimNode
        if (s.x === undefined || s.y === undefined || t.x === undefined || t.y === undefined) continue
        if (!visible.has(s.id) || !visible.has(t.id)) continue
        const lit = hover ? neighbours.has(s.id) && neighbours.has(t.id) && (s.id === hover.id || t.id === hover.id) : true
        ctx.globalAlpha = lit ? 0.9 : 0.15
        ctx.strokeStyle = l.edge.relation ? c.accent : c.border
        ctx.lineWidth = (l.edge.kind === 'embed' ? 0.8 : 1.2 + Math.min(2, (l.edge.count - 1) * 0.5)) / k
        ctx.setLineDash(l.edge.relation ? [4 / k, 3 / k] : [])
        ctx.beginPath()
        ctx.moveTo(s.x, s.y)
        ctx.lineTo(t.x, t.y)
        ctx.stroke()
        ctx.setLineDash([])
        // Arrowhead at the target's edge.
        const dx = t.x - s.x
        const dy = t.y - s.y
        const len = Math.hypot(dx, dy) || 1
        const ux = dx / len
        const uy = dy / len
        const ax = t.x - ux * (t.r + 2)
        const ay = t.y - uy * (t.r + 2)
        const size = 4 / Math.sqrt(k)
        ctx.fillStyle = l.edge.relation ? c.accent : c.border
        ctx.beginPath()
        ctx.moveTo(ax, ay)
        ctx.lineTo(ax - ux * size - uy * size * 0.6, ay - uy * size + ux * size * 0.6)
        ctx.lineTo(ax - ux * size + uy * size * 0.6, ay - uy * size - ux * size * 0.6)
        ctx.closePath()
        ctx.fill()
        if (l.edge.relation && (k >= 1.3 || lit) && hover) {
          ctx.globalAlpha = 1
          ctx.fillStyle = c.accent
          ctx.font = `${10 / k}px ${getComputedStyle(el).fontFamily}`
          ctx.textAlign = 'center'
          ctx.fillText(l.edge.relation, (s.x + t.x) / 2, (s.y + t.y) / 2 - 3 / k)
        }
      }

      // Nodes
      const fontPx = (small ? 11 : 12) / k
      ctx.font = `${fontPx}px ${getComputedStyle(el).fontFamily}`
      ctx.textAlign = 'center'
      ctx.textBaseline = 'top'
      const showAllLabels = small || k >= 1.1 || st.nodes.length <= 60
      for (const n of st.nodes) {
        if (n.x === undefined || n.y === undefined || !visible.has(n.id)) continue
        const isFocus = n.id === focus
        const lit = hover ? neighbours.has(n.id) : true
        ctx.globalAlpha = lit ? 1 : 0.2
        const r = isFocus ? n.r + 3 : n.r
        ctx.beginPath()
        ctx.arc(n.x, n.y, r, 0, Math.PI * 2)
        if (n.node.missing) {
          ctx.fillStyle = c.bg
          ctx.fill()
          ctx.lineWidth = 1.2 / k
          ctx.strokeStyle = c.muted
          ctx.setLineDash([2 / k, 2 / k])
          ctx.stroke()
          ctx.setLineDash([])
        } else {
          ctx.fillStyle = TYPE_COLOURS[n.node.type] ?? c.muted
          ctx.fill()
          if (isFocus || n === hover) {
            ctx.lineWidth = 2 / k
            ctx.strokeStyle = c.fg
            ctx.stroke()
          }
        }
        if (showAllLabels || isFocus || n === hover || n.degree >= 6) {
          ctx.fillStyle = n.node.missing ? c.muted : c.fg
          const max = labelChars(small)
          const label = n.node.title.length > max ? n.node.title.slice(0, max - 1) + '…' : n.node.title
          ctx.fillText(label, n.x, n.y + r + 2 / k)
        }
      }
      ctx.globalAlpha = 1
    }
    st.raf = requestAnimationFrame(draw)

    // Pointer: hit test, pan, zoom, drag.
    const toWorld = (ev: { clientX: number; clientY: number }) => {
      const rect = cv.getBoundingClientRect()
      const { x, y, k } = st.view
      return { x: (ev.clientX - rect.left - x) / k, y: (ev.clientY - rect.top - y) / k }
    }
    const hit = (p: { x: number; y: number }): SimNode | null => {
      const q = (latest.current.filter ?? '').trim().toLowerCase()
      let best: SimNode | null = null
      let bestD = Infinity
      for (const n of st.nodes) {
        if (n.x === undefined || n.y === undefined) continue
        if (q && !n.node.title.toLowerCase().includes(q)) continue
        const d = Math.hypot(n.x - p.x, n.y - p.y)
        if (d <= n.r + 4 / st.view.k && d < bestD) {
          best = n
          bestD = d
        }
      }
      return best
    }
    let drag: { node: SimNode | null; startX: number; startY: number; moved: boolean; viewX: number; viewY: number } | null = null
    const onDown = (ev: PointerEvent) => {
      if (ev.button !== 0) return
      cv.setPointerCapture(ev.pointerId)
      const node = hit(toWorld(ev))
      drag = { node, startX: ev.clientX, startY: ev.clientY, moved: false, viewX: st.view.x, viewY: st.view.y }
      if (node) {
        node.fx = node.x
        node.fy = node.y
        st.sim?.alphaTarget(0.2).restart()
      }
    }
    const onMove = (ev: PointerEvent) => {
      if (drag) {
        const dx = ev.clientX - drag.startX
        const dy = ev.clientY - drag.startY
        if (!drag.moved && Math.hypot(dx, dy) > DRAG_THRESHOLD) {
          drag.moved = true
          st.userMoved = true
        }
        if (!drag.moved) return
        if (drag.node) {
          const p = toWorld(ev)
          drag.node.fx = p.x
          drag.node.fy = p.y
        } else {
          st.view = { ...st.view, x: drag.viewX + dx, y: drag.viewY + dy }
        }
        st.dirty = true
        return
      }
      const h = hit(toWorld(ev))
      if (h !== st.hover) {
        st.hover = h
        cv.style.cursor = h ? 'pointer' : 'grab'
        st.dirty = true
      }
    }
    const onUp = (ev: PointerEvent) => {
      if (!drag) return
      const d = drag
      drag = null
      if (d.node) {
        d.node.fx = null
        d.node.fy = null
        st.sim?.alphaTarget(0)
        if (!d.moved) latest.current.onOpen(d.node.node)
      }
      st.dirty = true
      ev.preventDefault()
    }
    const onWheel = (ev: WheelEvent) => {
      ev.preventDefault()
      st.userMoved = true
      const rect = cv.getBoundingClientRect()
      const px = ev.clientX - rect.left
      const py = ev.clientY - rect.top
      const factor = Math.exp(-ev.deltaY * 0.0015)
      const k = Math.min(6, Math.max(0.15, st.view.k * factor))
      const { x, y, k: k0 } = st.view
      st.view = { k, x: px - ((px - x) * k) / k0, y: py - ((py - y) * k) / k0 }
      st.dirty = true
    }
    const onLeave = () => {
      if (st.hover) {
        st.hover = null
        st.dirty = true
      }
    }
    cv.addEventListener('pointerdown', onDown)
    cv.addEventListener('pointermove', onMove)
    cv.addEventListener('pointerup', onUp)
    cv.addEventListener('pointercancel', onUp)
    cv.addEventListener('pointerleave', onLeave)
    cv.addEventListener('wheel', onWheel, { passive: false })
    cv.style.cursor = 'grab'
    return () => {
      cancelAnimationFrame(st.raf)
      ro.disconnect()
      cv.removeEventListener('pointerdown', onDown)
      cv.removeEventListener('pointermove', onMove)
      cv.removeEventListener('pointerup', onUp)
      cv.removeEventListener('pointercancel', onUp)
      cv.removeEventListener('pointerleave', onLeave)
      cv.removeEventListener('wheel', onWheel)
    }
  }, [])

  return (
    <div ref={host} className={`graph-canvas ${className ?? ''}`} data-node-count={graph.nodes.length} data-edge-count={graph.edges.length}>
      <canvas ref={canvas} aria-label="Link graph" role="img" />
    </div>
  )
}

/** Longest label drawn under a node, in characters; shorter in the side panel. */
const labelChars = (compact: boolean) => (compact ? 18 : 28)

/** Zoom/pan so every node and its label is in view. */
function fit(st: { nodes: SimNode[]; view: View; size: { w: number; h: number } }, compact = false, insetBottom = 0) {
  const pts = st.nodes.filter((n) => n.x !== undefined && n.y !== undefined)
  if (pts.length === 0 || st.size.w === 0) return
  let minX = Infinity
  let minY = Infinity
  let maxX = -Infinity
  let maxY = -Infinity
  for (const n of pts) {
    minX = Math.min(minX, n.x! - n.r)
    minY = Math.min(minY, n.y! - n.r)
    maxX = Math.max(maxX, n.x! + n.r)
    maxY = Math.max(maxY, n.y! + n.r)
  }
  // Labels are drawn at a fixed screen size, so their room is reserved in
  // screen pixels: half the widest label on each side, a line below.
  const fontPx = compact ? 11 : 12
  const longest = Math.min(labelChars(compact), Math.max(0, ...pts.map((n) => n.node.title.length)))
  const padX = Math.max(16, (longest * fontPx * 0.56) / 2 + 6)
  const padTop = 12
  const padBottom = fontPx + 10 + insetBottom
  const availW = Math.max(20, st.size.w - padX * 2)
  const availH = Math.max(20, st.size.h - padTop - padBottom)
  const k = Math.min(compact ? 1.4 : 2, Math.max(0.15, Math.min(availW / Math.max(1, maxX - minX), availH / Math.max(1, maxY - minY))))
  const cx = (minX + maxX) / 2
  const cy = (minY + maxY) / 2
  st.view = { k, x: st.size.w / 2 - cx * k, y: padTop + availH / 2 - cy * k }
}
