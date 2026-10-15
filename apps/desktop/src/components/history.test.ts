import { describe, expect, it } from 'vitest'
import { diffLines, diffSummary } from './history'

describe('diffLines', () => {
  it('keeps common lines and marks the rest', () => {
    const d = diffLines('a\nb\nc\n', 'a\nx\nc\nd\n')
    expect(d.map((l) => `${l.kind}:${l.text}`)).toEqual(['same:a', 'del:b', 'add:x', 'same:c', 'add:d', 'same:'])
    expect(diffSummary(d)).toBe('+2 −1')
  })

  it('reports no changes for equal text', () => {
    const d = diffLines('one\ntwo', 'one\ntwo')
    expect(d.every((l) => l.kind === 'same')).toBe(true)
    expect(diffSummary(d)).toBe('no changes')
  })

  it('handles empty sides', () => {
    expect(diffLines('', 'a').map((l) => l.kind)).toEqual(['del', 'add'])
    expect(diffLines('a', '').map((l) => l.kind)).toEqual(['del', 'add'])
  })
})
