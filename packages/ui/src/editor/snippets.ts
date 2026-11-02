import type { Editor, JSONContent } from '@tiptap/core'
import { TextQuote } from 'lucide-react'
import type { BlockDef, SnippetItem } from './blocks/types'
import { prepareTaskMarkers } from './blocks/taskItem'

/** How many snippets the `/` menu lists at once. */
const MENU_LIMIT = 8

/**
 * Insert Markdown at the cursor. One paragraph goes into the line as text;
 * anything more goes in as blocks.
 */
export function insertMarkdown(editor: Editor, markdown: string): boolean {
  const text = prepareTaskMarkers(markdown.replace(/\s+$/, ''))
  if (!text.trim()) return false
  const manager = (editor as Editor & { markdown?: { parse: (md: string) => JSONContent } }).markdown
  const doc = manager?.parse(text)
  const blocks = doc?.content ?? []
  if (blocks.length === 1 && blocks[0].type === 'paragraph' && editor.state.selection.$from.parent.isTextblock) {
    return editor.chain().focus().insertContent(blocks[0].content ?? []).run()
  }
  return editor.chain().focus().insertContent(text, { contentType: 'markdown' }).run()
}

/** The selection as Markdown, or '' when nothing is selected. */
export function selectionMarkdown(editor: Editor): string {
  const { selection } = editor.state
  if (selection.empty) return ''
  const manager = (editor as Editor & { markdown?: { serialize: (doc: JSONContent) => string } }).markdown
  const slice = selection.content()
  const content = slice.content.toJSON() as JSONContent[] | null
  if (!manager || !content) return editor.state.doc.textBetween(selection.from, selection.to, '\n\n')
  // An inline selection comes out as bare text nodes; wrap them so they serialise.
  const blocks = content.every((n) => n.type === 'text' || editor.schema.nodes[n.type ?? '']?.isInline) ? [{ type: 'paragraph', content }] : content
  return manager.serialize({ type: 'doc', content: blocks }).trim()
}

/** A snippet as an entry of the `/` menu or the Insert menu. */
export function snippetBlock(s: SnippetItem): BlockDef {
  return {
    id: `snippet:${s.id}`,
    label: s.name,
    description: s.description || 'Snippet',
    icon: TextQuote,
    group: 'text',
    toolbar: false,
    insert: async (editor, ctx) => {
      const md = await ctx.renderSnippet?.(s.id)
      if (md) insertMarkdown(editor, md)
    },
  }
}

/** Snippets matching what was typed after `/`; every one of them for "snippet". */
export function matchSnippets(list: SnippetItem[], query: string): SnippetItem[] {
  const q = query.trim().toLowerCase()
  if (!q || 'snippets'.startsWith(q)) return list.slice(0, q ? list.length : MENU_LIMIT)
  const starts = list.filter((s) => s.name.toLowerCase().startsWith(q))
  const within = list.filter((s) => !s.name.toLowerCase().startsWith(q) && `${s.name} ${s.description}`.toLowerCase().includes(q))
  return [...starts, ...within].slice(0, MENU_LIMIT)
}
