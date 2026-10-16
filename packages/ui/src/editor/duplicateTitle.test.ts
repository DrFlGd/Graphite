import { describe, expect, it } from 'vitest'
import { getSchema } from '@tiptap/core'
import { buildMarkdownExtensions } from './extensions'
import { markdownToDoc } from './markdown'
import { leadingTitleMatches } from './extensions/DuplicateTitle'

const schema = getSchema(buildMarkdownExtensions())
const doc = (md: string) => schema.nodeFromJSON(markdownToDoc(md))

describe('leadingTitleMatches', () => {
  it('matches a first level-1 heading that repeats the title', () => {
    expect(leadingTitleMatches(doc('# Counter material\n\nQuartz.'), 'Counter material')).toBe(true)
    expect(leadingTitleMatches(doc('#  counter   Material \n\nQuartz.'), 'Counter material')).toBe(true)
  })
  it('leaves other first blocks alone', () => {
    expect(leadingTitleMatches(doc('## Counter material'), 'Counter material')).toBe(false)
    expect(leadingTitleMatches(doc('# Something else'), 'Counter material')).toBe(false)
    expect(leadingTitleMatches(doc('Intro\n\n# Counter material'), 'Counter material')).toBe(false)
    expect(leadingTitleMatches(doc('# Counter material'), '')).toBe(false)
  })
})
