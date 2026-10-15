import { Download, FileText, Globe, GitBranch, GitFork, Video } from 'lucide-react'
import type { SourceKind } from '../api'

/** Icon for an importer kind, used by the sources list, the hub and the note bar. */
export function SourceKindIcon({ kind, size = 14 }: { kind: SourceKind; size?: number }) {
  switch (kind) {
    case 'github':
      return <GitFork size={size} />
    case 'git':
      return <GitBranch size={size} />
    case 'video':
      return <Video size={size} />
    case 'page':
      return <Globe size={size} />
    case 'pdf':
      return <FileText size={size} />
    default:
      return <Download size={size} />
  }
}

/** `github.com/octocat/Hello-World` for a URL, or the URL itself when it has no host. */
export function shortUrl(url: string): string {
  try {
    const u = new URL(url)
    const path = u.pathname.replace(/\/$/, '')
    return `${u.host}${path.length > 40 ? path.slice(0, 38) + '…' : path}`
  } catch {
    return url
  }
}

/** "3 minutes ago", "yesterday", "12 Sep" for a timestamp. */
export function timeAgo(iso: string | null | undefined): string {
  if (!iso) return 'never'
  const t = Date.parse(iso)
  if (Number.isNaN(t)) return iso
  const s = Math.max(0, (Date.now() - t) / 1000)
  if (s < 60) return 'just now'
  if (s < 3600) return `${Math.round(s / 60)} min ago`
  if (s < 86400) return `${Math.round(s / 3600)} h ago`
  if (s < 172800) return 'yesterday'
  if (s < 86400 * 14) return `${Math.round(s / 86400)} days ago`
  return new Date(t).toLocaleDateString(undefined, { day: 'numeric', month: 'short' })
}

/** The first thing in a text that looks like a URL, if any. */
export function firstUrl(text: string): string | null {
  const m = /(?:https?:\/\/|git@)[^\s<>()"']+/.exec(text)
  return m ? m[0].replace(/[.,;:]+$/, '') : null
}

/** Whether a palette query is a URL rather than a search. */
export function looksLikeUrl(text: string): boolean {
  const t = text.trim()
  return /^(https?:\/\/|git@)\S+$/.test(t) && !/\s/.test(t)
}
