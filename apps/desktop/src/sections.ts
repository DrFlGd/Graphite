import type { SectionPreview } from '@graphite/ui'
import { api } from './api'

/** What an embed or a link preview shows, from the core's `read_section`. */
export async function loadSection(target: string, anchor: string | null): Promise<SectionPreview | null> {
  const s = await api.readSection(target, anchor)
  return s ? { title: s.note.title, type: s.note.type, markdown: s.markdown } : null
}
