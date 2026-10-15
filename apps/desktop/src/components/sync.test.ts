import { describe, expect, it } from 'vitest'
import { ago, syncSummary } from './sync'

const status = (enabled: boolean, others: number, relay = false) => ({
  enabled,
  relay: relay ? { url: 'https://r.example', last: null, files: 2, kept_local: [] } : null,
  device: { id: 'a', name: 'laptop' },
  devices: [{ id: 'a', name: 'laptop', last_sync: null, notes: 1, this_device: true }].concat(
    Array.from({ length: others }, (_, i) => ({ id: `d${i}`, name: `dev${i}`, last_sync: null, notes: 1, this_device: false })),
  ),
  last_sync: null,
})

describe('syncSummary', () => {
  it('says off, on, or how many other devices', () => {
    expect(syncSummary(null)).toBe('Sync off')
    expect(syncSummary(status(false, 0))).toBe('Sync off')
    expect(syncSummary(status(true, 0))).toBe('Sync on')
    expect(syncSummary(status(true, 1))).toBe('1 other device')
    expect(syncSummary(status(true, 3))).toBe('3 other devices')
    expect(syncSummary(status(true, 0, true))).toBe('Sync on via relay')
    expect(syncSummary(status(true, 2, true))).toBe('2 other devices via relay')
  })
})

describe('ago', () => {
  const now = Date.parse('2026-10-15T12:00:00Z')
  it('rounds to a readable unit', () => {
    expect(ago(null, now)).toBe('never')
    expect(ago('not a date', now)).toBe('never')
    expect(ago('2026-10-15T11:59:50Z', now)).toBe('just now')
    expect(ago('2026-10-15T11:57:00Z', now)).toBe('3 min ago')
    expect(ago('2026-10-15T09:00:00Z', now)).toBe('3 h ago')
    expect(ago('2026-10-01T09:00:00Z', now)).toMatch(/2026|10|1/)
  })
})
