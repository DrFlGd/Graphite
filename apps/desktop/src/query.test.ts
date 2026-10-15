import { describe, expect, it } from 'vitest'
import type { ItemRow } from './api'
import { buildTree, cellValue, domainOf, sliceGroups, termValue, tokenize, withTerm } from './query'

const item = (note_id: string, line: number, parent: number | null): ItemRow =>
  ({ note_id, line, parent, kind: 'task', status: 'open', text: `t${line}`, raw: '', depth: parent === null ? 0 : 1, heading: null, due: null, priority: null, tags: [], project: null, answer: null, fields: {}, note_title: note_id, note_path: `${note_id}.md`, project_id: null, project_title: null }) as ItemRow

describe('query helpers', () => {
  it('tokenizes keeping quotes and links whole', () => {
    expect(tokenize('type:task project:[[Garden Robot]] "two words" sort:-due')).toEqual(['type:task', 'project:[[Garden Robot]]', '"two words"', 'sort:-due'])
  })

  it('reads, sets and removes terms', () => {
    const q = 'type:task project:[[Garden Robot]] sort:due'
    expect(termValue(q, 'project')).toBe('[[Garden Robot]]')
    expect(termValue(q, 'group')).toBeNull()
    expect(withTerm(q, 'group', 'status')).toBe('type:task project:[[Garden Robot]] sort:due group:status')
    expect(withTerm(q, 'project', '[[Other]]')).toBe('type:task sort:due project:[[Other]]')
    expect(withTerm(q, 'sort', null)).toBe('type:task project:[[Garden Robot]]')
    expect(domainOf(q)).toBe('items')
    expect(domainOf('type:project')).toBe('notes')
    expect(domainOf('motors')).toBe('notes')
  })

  it('nests items by parent line within a note', () => {
    const rows = [item('a', 1, null), item('a', 2, 1), item('a', 3, 2), item('b', 2, 1), item('a', 9, null)]
    const tree = buildTree(rows)
    expect(tree.map((n) => `${n.row.note_id}:${n.row.line}`)).toEqual(['a:1', 'b:2', 'a:9'])
    expect(tree[0].children[0].row.line).toBe(2)
    expect(tree[0].children[0].children[0].row.line).toBe(3)
  })

  it('slices grouped rows by the group counts', () => {
    const rows = ['a', 'b', 'c']
    expect(sliceGroups(rows, { groups: [] })).toEqual([{ key: '', rows }])
    expect(sliceGroups(rows, { groups: [{ key: 'x', count: 2 }, { key: '', count: 1 }] })).toEqual([
      { key: 'x', rows: ['a', 'b'] },
      { key: '', rows: ['c'] },
    ])
  })

  it('keeps the type of an edited property', () => {
    expect(cellValue('42', 7)).toBe(42)
    expect(cellValue('true', false)).toBe(true)
    expect(cellValue('a, b', ['x'])).toEqual(['a', 'b'])
    expect(cellValue('', 'x')).toBeNull()
    expect(cellValue('hello', 3)).toBe('hello')
  })
})
