import { describe, expect, it } from 'vitest'
import { datedDailyTitle } from './noteTypes'

describe('datedDailyTitle', () => {
  it('adds the date to a renamed daily note', () => {
    expect(datedDailyTitle('Garden day', 'notes/Daily/2026-10-14.md')).toBe('Garden day (2026-10-14)')
    expect(datedDailyTitle('  ', 'notes/Daily/2026-10-14.md')).toBe('2026-10-14')
  })
  it('leaves titles that hold the date, and undated files, alone', () => {
    expect(datedDailyTitle('Garden day (2026-10-14)', 'notes/Daily/2026-10-14.md')).toBe('Garden day (2026-10-14)')
    expect(datedDailyTitle('2026-10-14', 'notes/Daily/2026-10-14.md')).toBe('2026-10-14')
    expect(datedDailyTitle('Log', 'notes/Daily/log.md')).toBe('Log')
  })
})
