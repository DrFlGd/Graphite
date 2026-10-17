import { describe, expect, it } from 'vitest'
import { megabytes, shortLabel } from './JobsPanel'

describe('job labels', () => {
  it('shortens URLs', () => {
    expect(shortLabel('https://www.youtube.com/watch?v=e0_sAln0q30')).toBe('youtube.com/watch?v=e0_sAln0q30')
    expect(shortLabel('https://github.com/acme/widget/')).toBe('github.com/acme/widget')
    expect(shortLabel('Video tools')).toBe('Video tools')
    expect(shortLabel(`https://example.com/${'a'.repeat(80)}`)).toHaveLength(48)
  })
  it('writes sizes in megabytes', () => {
    expect(megabytes(1_500_000)).toBe('1.5 MB')
    expect(megabytes(42_630_221)).toBe('43 MB')
  })
})
