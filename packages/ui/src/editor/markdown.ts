import type { JSONContent } from '@tiptap/core'
import { MarkdownManager } from '@tiptap/markdown'
import { buildMarkdownExtensions } from './extensions'
import { prepareTaskMarkers } from './blocks/taskItem'

let manager: MarkdownManager | null = null

function getManager(): MarkdownManager {
  if (!manager) manager = new MarkdownManager({ extensions: buildMarkdownExtensions() })
  return manager
}

/** Markdown → Tiptap JSON, without an editor instance (used by tests and tools). */
export function markdownToDoc(markdown: string): JSONContent {
  return getManager().parse(prepareTaskMarkers(markdown))
}

/** Tiptap JSON → Markdown. */
export function docToMarkdown(doc: JSONContent): string {
  return getManager().serialize(doc)
}

/** Parse and re-serialise; the result should equal the input for everything Graphite writes. */
export function roundTrip(markdown: string): string {
  return docToMarkdown(markdownToDoc(markdown))
}
