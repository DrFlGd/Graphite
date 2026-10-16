import { describe, expect, it } from 'vitest'
import { snippetParts, snippetText } from './snippet'

const S = '\u0002'
const E = '\u0003'

describe('snippetParts', () => {
  it('flags matches', () => {
    expect(snippetParts(`A ${S}solar${E} panel`)).toEqual([
      { text: 'A ', match: false },
      { text: 'solar', match: true },
      { text: ' panel', match: false },
    ])
  })
  it('drops Markdown syntax around and inside matches', () => {
    expect(snippetText(`## Wiring\n- [ ] Print [[${S}Chassis${E} v3]] and **bold** see [the docs](https://x.y)`)).toBe('Wiring Print Chassis v3 and bold see the docs')
    expect(snippetParts(`![[${S}mount${E}.stl]]`)).toEqual([
      { text: 'mount', match: true },
      { text: '.stl', match: false },
    ])
    expect(snippetText('[[Garden Robot|the robot]] _quick_ `code`')).toBe('the robot quick code')
  })
})
