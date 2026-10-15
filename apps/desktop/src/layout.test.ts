import { describe, expect, it } from 'vitest'
import { activeTab, closeActiveTab, closeTab, cycleTab, dropNotes, emptyLayout, findNote, open, parseLayout, selectTab, splitRight } from './layout'

const note = (noteId: string) => ({ kind: 'note' as const, noteId })

describe('layout', () => {
  it('reuses an empty tab, then replaces the active tab, unless a new tab is asked for', () => {
    let l = emptyLayout()
    l = open(l, note('a'))
    expect(l.panes[0].tabs.map((t) => t.kind)).toEqual(['note'])
    l = open(l, note('b'))
    expect(l.panes[0].tabs).toHaveLength(1)
    expect(activeTab(l)).toMatchObject({ kind: 'note', noteId: 'b' })
    l = open(l, note('c'), true)
    expect(l.panes[0].tabs).toHaveLength(2)
    expect(activeTab(l)).toMatchObject({ noteId: 'c' })
  })

  it('has one tasks tab per pane and keeps it across a reload', () => {
    let l = open(emptyLayout(), { kind: 'tasks' })
    l = open(l, { kind: 'tasks' }, true)
    expect(l.panes[0].tabs.filter((t) => t.kind === 'tasks')).toHaveLength(1)
    const parsed = parseLayout(JSON.parse(JSON.stringify(l)))
    expect(parsed && activeTab(parsed).kind).toBe('tasks')
    expect(parseLayout({ panes: [{ id: 'p', tabs: [{ id: 't', kind: 'bogus' }], active: 0 }], active: 0 })?.panes[0].tabs[0].kind).toBe('empty')
  })

  it('focuses an existing tab instead of opening the note twice', () => {
    let l = open(open(emptyLayout(), note('a')), note('b'), true)
    l = open(l, note('a'), true)
    expect(l.panes[0].tabs).toHaveLength(2)
    expect(activeTab(l)).toMatchObject({ noteId: 'a' })
    l = open(l, { kind: 'graph' }, true)
    l = open(l, { kind: 'graph' }, true)
    expect(l.panes[0].tabs.filter((t) => t.kind === 'graph')).toHaveLength(1)
  })

  it('closing keeps a sensible active tab and never leaves a pane without tabs', () => {
    let l = open(open(open(emptyLayout(), note('a')), note('b'), true), note('c'), true)
    l = selectTab(l, 0, 1)
    l = closeActiveTab(l)
    expect(l.panes[0].tabs.map((t) => (t as { noteId: string }).noteId)).toEqual(['a', 'c'])
    expect(activeTab(l)).toMatchObject({ noteId: 'c' })
    l = closeTab(l, 0, 0)
    expect(activeTab(l)).toMatchObject({ noteId: 'c' })
    l = closeActiveTab(l)
    expect(l.panes).toHaveLength(1)
    expect(activeTab(l).kind).toBe('empty')
  })

  it('splits to the right, cycles tabs, and removes a pane when its last tab closes', () => {
    let l = open(emptyLayout(), note('a'))
    l = splitRight(l)
    expect(l.panes).toHaveLength(2)
    expect(l.active).toBe(1)
    expect(activeTab(l)).toMatchObject({ noteId: 'a' })
    l = open(l, note('b'), true)
    l = cycleTab(l, 1)
    expect(activeTab(l)).toMatchObject({ noteId: 'a' })
    l = cycleTab(l, -1)
    expect(activeTab(l)).toMatchObject({ noteId: 'b' })
    l = closeActiveTab(l)
    l = closeActiveTab(l)
    expect(l.panes).toHaveLength(1)
    expect(l.active).toBe(0)
  })

  it('keeps one tab per file and updates its page in place', () => {
    let l = open(emptyLayout(), { kind: 'asset', path: 'attachments/a.pdf' })
    l = open(l, { kind: 'asset', path: 'attachments/b.pdf' }, true)
    l = open(l, { kind: 'asset', path: 'attachments/a.pdf', page: 3 }, true)
    expect(l.panes[0].tabs).toHaveLength(2)
    expect(l.panes[0].active).toBe(0)
    expect(l.panes[0].tabs[0]).toMatchObject({ kind: 'asset', path: 'attachments/a.pdf', page: 3 })
    l = open(l, { kind: 'files' }, true)
    l = open(l, { kind: 'files' }, true)
    expect(l.panes[0].tabs).toHaveLength(3)
    l = open(l, { kind: 'sources' }, true)
    l = open(l, { kind: 'sources' }, true)
    expect(l.panes[0].tabs).toHaveLength(4)
    expect(parseLayout(JSON.parse(JSON.stringify(l)))?.panes[0].tabs).toHaveLength(4)
  })

  it('drops tabs of deleted notes and finds where a note is open', () => {
    let l = open(open(emptyLayout(), note('a')), note('b'), true)
    l = splitRight(l)
    expect(findNote(l, 'a')).toEqual({ pi: 0, ti: 0 })
    l = dropNotes(l, (id) => id === 'a')
    expect(l.panes).toHaveLength(1)
    expect(l.panes[0].tabs.map((t) => t.kind)).toEqual(['note'])
    expect(findNote(l, 'b')).toBeNull()
  })

  it('parses stored layouts defensively', () => {
    expect(parseLayout(null)).toBeNull()
    expect(parseLayout({ panes: [] })).toBeNull()
    const l = parseLayout({ panes: [{ id: 'p', tabs: [{ id: 't', kind: 'note', noteId: 'x' }, { id: 'bad', kind: 'nope' }], active: 5 }], active: 9 })
    expect(l?.panes[0].tabs).toHaveLength(1)
    expect(l?.panes[0].active).toBe(0)
    expect(l?.active).toBe(0)
  })
})
