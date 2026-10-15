// Tabs and split panes: a pure model the app renders and the tests check.

/** What a tab shows. */
export type TabSpec =
  | { kind: 'note'; noteId: string }
  | { kind: 'inbox' }
  | { kind: 'graph' }
  | { kind: 'tasks' }
  | { kind: 'files' }
  | { kind: 'sources' }
  | { kind: 'asset'; path: string; page?: number }
  | { kind: 'empty' }
export type Tab = TabSpec & { id: string }

export interface Pane {
  id: string
  tabs: Tab[]
  /** Index into `tabs`. */
  active: number
}

export interface Layout {
  panes: Pane[]
  /** Index into `panes`. */
  active: number
}

export const MAX_PANES = 3

let counter = 0
export const newId = (): string => `${Date.now().toString(36)}${(counter++).toString(36)}`

export const emptyTab = (): Tab => ({ id: newId(), kind: 'empty' })

export function emptyLayout(): Layout {
  return { panes: [{ id: newId(), tabs: [emptyTab()], active: 0 }], active: 0 }
}

export const activePane = (l: Layout): Pane => l.panes[Math.min(l.active, l.panes.length - 1)]
export const activeTab = (l: Layout): Tab => {
  const p = activePane(l)
  return p.tabs[Math.min(p.active, p.tabs.length - 1)]
}

const sameTarget = (a: Tab, b: TabSpec): boolean => {
  if (a.kind !== b.kind) return false
  if (a.kind === 'note' && b.kind === 'note') return a.noteId === b.noteId
  if (a.kind === 'asset' && b.kind === 'asset') return a.path === b.path
  return true
}

/**
 * Show `what` in the active pane: focus an existing tab for it, else put it
 * in a new tab (`newTab`) or replace the active tab's content. An empty tab
 * is always reused.
 */
export function open(l: Layout, what: TabSpec, newTab = false): Layout {
  const pi = Math.min(l.active, l.panes.length - 1)
  const pane = l.panes[pi]
  const existing = pane.tabs.findIndex((t) => sameTarget(t, what))
  if (existing >= 0) {
    // Reopening a file at another page updates the tab in place.
    const tabs = what.kind === 'asset' ? pane.tabs.map((t, i) => (i === existing ? { ...t, ...what } : t)) : pane.tabs
    return setPane(l, pi, { ...pane, tabs, active: existing })
  }
  const tab: Tab = { ...what, id: newId() }
  const cur = pane.tabs[pane.active]
  if (!newTab || cur?.kind === 'empty') {
    const tabs = pane.tabs.map((t, i) => (i === pane.active ? tab : t))
    return setPane(l, pi, { ...pane, tabs })
  }
  const tabs = [...pane.tabs.slice(0, pane.active + 1), tab, ...pane.tabs.slice(pane.active + 1)]
  return setPane(l, pi, { ...pane, tabs, active: pane.active + 1 })
}

/** Close a tab; the last tab of a pane becomes empty, unless another pane exists, in which case the pane goes. */
export function closeTab(l: Layout, pi: number, ti: number): Layout {
  const pane = l.panes[pi]
  if (!pane || !pane.tabs[ti]) return l
  if (pane.tabs.length === 1) {
    if (l.panes.length === 1) return setPane(l, pi, { ...pane, tabs: [emptyTab()], active: 0 })
    const panes = l.panes.filter((_, i) => i !== pi)
    return { panes, active: Math.min(l.active > pi ? l.active - 1 : l.active, panes.length - 1) }
  }
  const tabs = pane.tabs.filter((_, i) => i !== ti)
  const active = ti < pane.active ? pane.active - 1 : Math.min(pane.active, tabs.length - 1)
  return setPane(l, pi, { ...pane, tabs, active })
}

export const closeActiveTab = (l: Layout): Layout => closeTab(l, l.active, activePane(l).active)

export function selectTab(l: Layout, pi: number, ti: number): Layout {
  const pane = l.panes[pi]
  if (!pane || !pane.tabs[ti]) return l
  return { ...setPane(l, pi, { ...pane, active: ti }), active: pi }
}

/** Ctrl+Tab / Ctrl+Shift+Tab. */
export function cycleTab(l: Layout, dir: 1 | -1): Layout {
  const pane = activePane(l)
  const n = pane.tabs.length
  return setPane(l, l.active, { ...pane, active: (pane.active + dir + n) % n })
}

export function focusPane(l: Layout, pi: number): Layout {
  return pi >= 0 && pi < l.panes.length ? { ...l, active: pi } : l
}

/** Open a copy of the active tab in a new pane to the right. */
export function splitRight(l: Layout): Layout {
  if (l.panes.length >= MAX_PANES) return l
  const tab = activeTab(l)
  const pane: Pane = { id: newId(), tabs: [{ ...tab, id: newId() }], active: 0 }
  const panes = [...l.panes.slice(0, l.active + 1), pane, ...l.panes.slice(l.active + 1)]
  return { panes, active: l.active + 1 }
}

/** Remove tabs for notes that no longer exist (after a delete or an external change). */
export function dropNotes(l: Layout, exists: (noteId: string) => boolean): Layout {
  let out = l
  for (let pi = out.panes.length - 1; pi >= 0; pi--) {
    for (let ti = out.panes[pi].tabs.length - 1; ti >= 0; ti--) {
      const t = out.panes[pi]?.tabs[ti]
      if (t?.kind === 'note' && !exists(t.noteId)) out = closeTab(out, pi, ti)
    }
  }
  return out
}

/** Find where a note is open, if anywhere. */
export function findNote(l: Layout, noteId: string): { pi: number; ti: number } | null {
  for (let pi = 0; pi < l.panes.length; pi++) {
    const ti = l.panes[pi].tabs.findIndex((t) => t.kind === 'note' && t.noteId === noteId)
    if (ti >= 0) return { pi, ti }
  }
  return null
}

function setPane(l: Layout, pi: number, pane: Pane): Layout {
  return { ...l, panes: l.panes.map((p, i) => (i === pi ? pane : p)) }
}

/** Parse a stored layout, dropping anything malformed. */
export function parseLayout(raw: unknown): Layout | null {
  if (!raw || typeof raw !== 'object') return null
  const l = raw as Partial<Layout>
  if (!Array.isArray(l.panes) || l.panes.length === 0) return null
  const panes: Pane[] = []
  for (const p of l.panes) {
    if (!p || !Array.isArray(p.tabs)) return null
    const tabs = p.tabs.filter((t: Tab) => t && typeof t.id === 'string' && (t.kind === 'empty' || t.kind === 'inbox' || t.kind === 'graph' || t.kind === 'tasks' || t.kind === 'files' || t.kind === 'sources' || (t.kind === 'note' && typeof t.noteId === 'string') || (t.kind === 'asset' && typeof t.path === 'string')))
    if (tabs.length === 0) tabs.push(emptyTab())
    panes.push({ id: typeof p.id === 'string' ? p.id : newId(), tabs, active: Math.min(Math.max(0, Number(p.active) || 0), tabs.length - 1) })
  }
  return { panes: panes.slice(0, MAX_PANES), active: Math.min(Math.max(0, Number(l.active) || 0), panes.length - 1) }
}
