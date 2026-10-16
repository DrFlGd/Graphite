import { describe, expect, it } from 'vitest'
import { fieldRanges } from './extensions/InlineFields'

const pick = (text: string) => fieldRanges(text).map((r) => [text.slice(...r.key), r.value ? text.slice(...r.value) : null])

describe('fieldRanges', () => {
  it('finds each field and its value up to the next field', () => {
    expect(pick('Call the plumber project:: Kitchen remodel due:: 2026-10-16')).toEqual([
      ['project::', 'Kitchen remodel'],
      ['due::', '2026-10-16'],
    ])
  })
  it('handles a field whose value is a separate link node', () => {
    expect(pick('part-of:: ')).toEqual([['part-of::', null]])
  })
  it('ignores double colons that are not fields', () => {
    expect(pick('std::vector and a::b')).toEqual([])
  })
})
