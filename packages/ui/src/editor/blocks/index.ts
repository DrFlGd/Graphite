import type { AnyExtension } from '@tiptap/core'
import type { BlockDef } from './types'
import { paragraph, heading1, heading2, heading3, blockquote, divider } from './text'
import { bulletList, orderedList, taskList, gap } from './lists'
import { codeBlock } from './codeBlock'
import { table } from './table'
import { image } from './image'
import { file } from './fileEmbed'
import { noteLink } from './noteLink'

export type { BlockDef, BlockContext } from './types'

/** Every insertable block, in menu order. Add a new block type here. */
export const BLOCKS: BlockDef[] = [
  paragraph,
  heading1,
  heading2,
  heading3,
  bulletList,
  orderedList,
  taskList,
  gap,
  blockquote,
  codeBlock,
  table,
  image,
  file,
  divider,
  noteLink,
]

export const blockById = (id: string): BlockDef | undefined => BLOCKS.find((b) => b.id === id)

/** Extensions contributed by the registered blocks (deduplicated). */
export function blockExtensions(): AnyExtension[] {
  const seen = new Set<string>()
  const out: AnyExtension[] = []
  for (const b of BLOCKS) {
    for (const ext of b.extensions ?? []) {
      if (!seen.has(ext.name)) {
        seen.add(ext.name)
        out.push(ext)
      }
    }
  }
  return out
}

/** Case-insensitive match on label, id and keywords, for the `/` menu. */
export function filterBlocks(query: string): BlockDef[] {
  const q = query.trim().toLowerCase()
  if (!q) return BLOCKS
  return BLOCKS.filter(
    (b) =>
      b.label.toLowerCase().includes(q) ||
      b.id.toLowerCase().includes(q) ||
      (b.keywords ?? []).some((k) => k.toLowerCase().includes(q)),
  )
}
