import { describe, expect, it } from 'vitest'
import { hasTag, tagTree } from './tags'

describe('tag tree', () => {
  const notes = [{ tags: ['garden', 'garden/beds'] }, { tags: ['Garden/Tools', 'idea'] }, { tags: ['project/garden'] }, { tags: [] }]

  it('nests tags on slashes and counts each note once per tag', () => {
    const tree = tagTree(notes)
    expect(tree.map((t) => [t.name, t.count])).toEqual([
      ['garden', 2],
      ['idea', 1],
      ['project', 1],
    ])
    expect(tree[0].children.map((t) => [t.path, t.count])).toEqual([
      ['garden/beds', 1],
      ['Garden/Tools', 1],
    ])
  })

  it('matches a parent tag against its children', () => {
    expect(hasTag(notes[1], 'garden')).toBe(true)
    expect(hasTag(notes[1], '#garden/tools')).toBe(true)
    expect(hasTag(notes[2], 'garden')).toBe(false)
    expect(hasTag(notes[0], 'gard')).toBe(false)
  })
})
