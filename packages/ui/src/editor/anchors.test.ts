import { getSchema } from '@tiptap/core'
import { Node as PMNode } from '@tiptap/pm/model'
import { describe, expect, it } from 'vitest'
import { blockIdOf, findAnchor } from './anchors'
import { buildMarkdownExtensions } from './extensions'
import { markdownToDoc } from './markdown'

const schema = getSchema(buildMarkdownExtensions())
const doc = (md: string) => PMNode.fromJSON(schema, markdownToDoc(md))

describe('anchors', () => {
  const d = doc('# Robot\n\nIntro.\n\n## Power\n\nBattery is 12 V. ^battery\n\n### Motors\n\nTwo motors.\n\n- wheels ^wheels\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n^table')

  it('finds headings by text, ignoring case and nesting', () => {
    const at = findAnchor(d, 'power')
    expect(at).not.toBeNull()
    expect(d.nodeAt(at!)?.textContent).toBe('Power')
    expect(d.nodeAt(findAnchor(d, 'Power#Motors')!)?.textContent).toBe('Motors')
    expect(findAnchor(d, 'Nope')).toBeNull()
  })

  it('finds blocks by id', () => {
    expect(d.nodeAt(findAnchor(d, '^battery')!)?.textContent).toBe('Battery is 12 V. ^battery')
    expect(d.nodeAt(findAnchor(d, '^wheels')!)?.textContent).toBe('wheels ^wheels')
    expect(findAnchor(d, '^table')).not.toBeNull()
    expect(findAnchor(d, '^missing')).toBeNull()
  })

  it('reads block ids only at the end, after a space', () => {
    expect(blockIdOf('text ^ab-1')).toBe('ab-1')
    expect(blockIdOf('^ab')).toBe('ab')
    expect(blockIdOf('x^2')).toBeNull()
    expect(blockIdOf('a ^b c')).toBeNull()
  })
})
