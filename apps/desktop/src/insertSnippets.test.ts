import { describe, expect, it } from 'vitest'
import { snippetName } from './insertSnippets'

describe('snippet names', () => {
  it('uses the first words of the first sentence', () => {
    expect(snippetName('Seedlings for the beds. #garden/beds')).toBe('Seedlings for the beds')
    expect(snippetName('\n## Weekly **review** checklist for the team and more\n\n- one')).toBe('Weekly review checklist for the')
    expect(snippetName('- [ ] Water the beds')).toBe('Water the beds')
    expect(snippetName('```\n```')).toBe('Snippet')
  })
})
