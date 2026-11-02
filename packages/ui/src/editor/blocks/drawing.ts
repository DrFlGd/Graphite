import { PenTool } from 'lucide-react'
import type { BlockDef } from './types'
import { FileEmbed } from './fileEmbed'

/**
 * A new Excalidraw drawing (E11): the host creates the `.excalidraw` file,
 * the note embeds it like any file (`![Name](path)`), and the embed shows
 * the picture; clicking it opens the drawing editor.
 */
export const drawing: BlockDef = {
  id: 'drawing',
  label: 'Drawing',
  description: 'Sketch or diagram by hand; saved in the vault as an Excalidraw file',
  icon: PenTool,
  group: 'media',
  keywords: ['sketch', 'draw', 'excalidraw', 'whiteboard', 'diagram', 'doodle'],
  toolbar: false,
  available: (ctx) => !!ctx.newDrawing,
  extensions: [FileEmbed],
  insert: async (editor, ctx) => {
    const src = ctx.newDrawing ? await ctx.newDrawing() : null
    if (!src) return
    const file = src.split('/').pop() ?? src
    let name = file
    try {
      name = decodeURIComponent(file)
    } catch {
      // keep the encoded name
    }
    name = name.replace(/\.excalidraw$/i, '')
    editor.chain().focus().insertContent({ type: 'fileEmbed', attrs: { src, name } }).run()
    ctx.openFile?.(src)
  },
}
