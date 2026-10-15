import { describe, expect, it } from 'vitest'
import { markdownToDoc, roundTrip } from './markdown'

/**
 * ADR-005: every block type ships with a Markdown → doc → Markdown
 * round-trip test. Each case is exactly what Graphite writes for that block.
 */
const CASES: Record<string, string> = {
  paragraph: 'Just a paragraph.',
  headings: '# One\n\n## Two\n\n### Three',
  bulletList: '- a\n- b\n  - nested',
  orderedList: '1. first\n2. second',
  taskList: '- [ ] todo\n- [x] done',
  taskStates: '- [ ] open\n- [/] doing\n- [x] done\n- [-] cancelled\n- [?] a gap',
  taskNested: '- [ ] parent due:: 2026-11-01 #p1\n  - [?] child question\n  - [/] child doing',
  taskLinked: '- [?] [[Pump]] current? blocks:: [[Order parts]]',
  blockquote: '> quoted\n> text',
  codeBlock: '```rust\nfn main() {}\n```',
  codeBlockNoLang: '```\nplain\n```',
  table: '| a   | b   |\n| --- | --- |\n| 1   | 2   |',
  image: '![chassis](attachments/chassis.png)',
  divider: 'above\n\n---\n\nbelow',
  wikiLink: 'See [[Garden Robot]] and [[Garden Robot#Power|the power bit]].',
  typedLink: 'depends-on:: [[Battery]]',
  inlineMarks: 'some **bold**, *italic*, `code` and ~~gone~~ text',
  webLink: 'a [site](https://example.com) here',
}

describe('markdown round-trip', () => {
  for (const [name, md] of Object.entries(CASES)) {
    it(name, () => {
      expect(roundTrip(md).trim()).toBe(md)
    })
  }

  it('parses wiki links into nodes rather than text', () => {
    const doc = markdownToDoc('x [[A|b]] y')
    const para = doc.content?.[0]
    const link = para?.content?.find((n) => n.type === 'wikiLink')
    expect(link?.attrs).toEqual({ target: 'A', heading: null, display: 'b' })
  })

  it('gives task items their state', () => {
    const doc = markdownToDoc('- [?] why\n- [/] now\n- [x] did')
    const list = doc.content?.[0]
    expect(list?.type).toBe('taskList')
    expect(list?.content?.map((n) => n.attrs?.state)).toEqual(['gap', 'doing', 'done'])
    expect(list?.content?.[0].content?.[0].content?.[0].text).toBe('why')
    const fence = '```\n- [?] not a task\n```'
    expect(roundTrip(fence).trim()).toBe(fence)
  })

  it('leaves code untouched', () => {
    const md = '`[[not a link]]`\n\n```\n[[nor this]]\n```'
    expect(roundTrip(md).trim()).toBe(md)
  })
})
