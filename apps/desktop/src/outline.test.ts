import { describe, expect, it } from 'vitest'
import { markdownOutline } from './outline'

describe('outline', () => {
  it('lists headings with nested anchors, skipping code', () => {
    const body = '# Robot\n\nIntro\n\n## Power **supply**\n\n```\n# not a heading\n```\n\n### Motors ^m1\n\n## See [[Wiring|the wiring]]\n\n# Notes\n\n## Power supply\n'
    expect(markdownOutline(body)).toEqual([
      { level: 1, text: 'Robot', anchor: 'Robot' },
      { level: 2, text: 'Power supply', anchor: 'Robot#Power supply' },
      { level: 3, text: 'Motors', anchor: 'Robot#Power supply#Motors' },
      { level: 2, text: 'See the wiring', anchor: 'Robot#See the wiring' },
      { level: 1, text: 'Notes', anchor: 'Notes' },
      { level: 2, text: 'Power supply', anchor: 'Notes#Power supply' },
    ])
    expect(markdownOutline(body, 'robot').map((h) => h.anchor).slice(0, 2)).toEqual(['Power supply', 'Power supply#Motors'])
  })
})
