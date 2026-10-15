import type { SyncStatus } from '../api'

/** One short line for the sidebar: whether sync is on and who else is in. */
export function syncSummary(status: SyncStatus | null): string {
  if (!status || !status.enabled) return 'Sync off'
  const others = status.devices.filter((d) => !d.this_device).length
  if (others === 0) return 'Sync on'
  return others === 1 ? '1 other device' : `${others} other devices`
}

/** "just now", "3 min ago", "2 h ago", or a date. */
export function ago(iso: string | null, now = Date.now()): string {
  if (!iso) return 'never'
  const t = Date.parse(iso)
  if (Number.isNaN(t)) return 'never'
  const s = Math.max(0, Math.round((now - t) / 1000))
  if (s < 60) return 'just now'
  const m = Math.round(s / 60)
  if (m < 60) return `${m} min ago`
  const h = Math.round(m / 60)
  if (h < 48) return `${h} h ago`
  return new Date(t).toLocaleDateString()
}
