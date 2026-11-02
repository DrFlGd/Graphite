import { Extension, type Editor } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import { Decoration, DecorationSet } from '@tiptap/pm/view'

/**
 * Link anchors inside a note, as Obsidian writes them: `[[Note#Heading]]`
 * (nested headings as `#Parts#Motors`) and `[[Note#^id]]` for a block whose
 * last line ends with ` ^id`. The core reads the same rules
 * (`core/src/sections.rs`).
 */
const BLOCK_ID = /(^|\s)\^([A-Za-z0-9-]+)\s*$/

/** The block id at the end of `text`, if any. */
export function blockIdOf(text: string): string | null {
  return BLOCK_ID.exec(text)?.[2] ?? null
}

const norm = (s: string) => s.replace(/\s+/g, ' ').trim().toLowerCase()

/** Document position of the heading or block an anchor names, or null. */
export function findAnchor(doc: PMNode, anchor: string): number | null {
  const a = anchor.trim()
  let found: number | null = null
  if (a.startsWith('^')) {
    const id = a.slice(1)
    let previous: number | null = null
    doc.descendants((node, pos) => {
      if (found !== null) return false
      if (node.isTextblock) {
        const text = node.textContent
        if (text.trim() === `^${id}`) {
          // A bare `^id` line names the block above it.
          found = previous ?? pos
          return false
        }
        if (blockIdOf(text) === id) {
          found = pos
          return false
        }
        previous = pos
        return false
      }
      if (node.isBlock && !node.isTextblock && node.type.name !== 'doc') previous = pos
      return true
    })
    return found
  }
  const parts = a.split('#').map(norm).filter(Boolean)
  if (parts.length === 0) return null
  const headings: { pos: number; level: number; text: string }[] = []
  doc.descendants((node, pos) => {
    if (node.type.name === 'heading') headings.push({ pos, level: Number(node.attrs.level) || 1, text: norm(node.textContent.replace(BLOCK_ID, '')) })
    return !node.isTextblock
  })
  // `Parts#Motors`: each part is a heading inside the section of the one before.
  let from = 0
  let within: number | null = null
  let hit: (typeof headings)[number] | null = null
  for (const part of parts) {
    hit = null
    for (let i = from; i < headings.length; i++) {
      const h = headings[i]
      if (within !== null && h.level <= within) break
      if (h.text === part) {
        hit = h
        from = i + 1
        within = h.level
        break
      }
    }
    if (!hit) return null
  }
  return hit?.pos ?? null
}

/** The open note's headings, with the anchor that links to each (`Parts#Motors` when nested). */
export function outlineOf(doc: PMNode): { level: number; text: string; anchor: string; pos: number }[] {
  const out: { level: number; text: string; anchor: string; pos: number }[] = []
  const stack: { level: number; text: string }[] = []
  doc.forEach((node, pos) => {
    if (node.type.name !== 'heading') return
    const level = Number(node.attrs.level) || 1
    const text = node.textContent.replace(BLOCK_ID, '').trim()
    if (!text) return
    while (stack.length && stack[stack.length - 1].level >= level) stack.pop()
    stack.push({ level, text })
    out.push({ level, text, anchor: stack.map((s) => s.text).join('#'), pos })
  })
  return out
}

/** Scroll the editor to an anchor and flash it; false when the note has no such heading or block. */
export function scrollToAnchor(editor: Editor, anchor: string): boolean {
  const pos = findAnchor(editor.state.doc, anchor)
  if (pos === null) return false
  scrollToPos(editor, pos)
  return true
}

/** Put the cursor in the block at `pos`, scroll it into view and flash it. */
export function scrollToPos(editor: Editor, pos: number) {
  editor.chain().focus().setTextSelection(pos + 1).run()
  const dom = editor.view.nodeDOM(pos) as HTMLElement | null
  const el = dom && dom.nodeType === 1 ? dom : (editor.view.domAtPos(pos + 1).node as HTMLElement | null)?.parentElement
  if (el) {
    el.scrollIntoView({ block: 'center' })
    el.classList.add('is-flash')
    window.setTimeout(() => el.classList.remove('is-flash'), 1200)
  }
}

/** A short random block id, six lowercase letters and digits, as Obsidian makes them. */
export function newBlockId(): string {
  const chars = 'abcdefghijklmnopqrstuvwxyz0123456789'
  let id = ''
  for (let i = 0; i < 6; i++) id += chars[Math.floor(Math.random() * chars.length)]
  return id
}

/**
 * A wiki link to where the cursor is: the heading it is in, or the block,
 * giving the block an id (` ^abc123` at its end) when it has none yet.
 * Returns null outside any text block.
 */
export function linkToCursor(editor: Editor, title: string): string | null {
  const { $from } = editor.state.selection
  const block = $from.parent
  if (!block.isTextblock) return null
  if (block.type.name === 'heading') {
    const text = block.textContent.replace(BLOCK_ID, '').trim()
    return text ? `[[${title}#${text}]]` : `[[${title}]]`
  }
  let id = blockIdOf(block.textContent)
  if (!id) {
    id = newBlockId()
    const end = $from.end()
    const pad = block.textContent.length === 0 || /\s$/.test(block.textContent) ? '' : ' '
    editor.chain().insertContentAt(end, `${pad}^${id}`).run()
  }
  return `[[${title}#^${id}]]`
}

function decorate(doc: PMNode): DecorationSet {
  const decos: Decoration[] = []
  doc.descendants((node, pos) => {
    if (node.type.name === 'codeBlock') return false
    if (!node.isTextblock) return true
    // The id is at the end of the block's last run of text (links and other atoms take no characters).
    const last = node.lastChild
    if (last?.isText && last.text) {
      const m = BLOCK_ID.exec(last.text)
      if (m) {
        const at = pos + 1 + node.content.size - last.nodeSize
        decos.push(Decoration.inline(at + m.index + m[1].length, at + last.text.trimEnd().length, { class: 'gr-block-id', title: 'Block id: links to this block use it' }))
      }
    }
    return false
  })
  return DecorationSet.create(doc, decos)
}

/** Block ids (` ^abc123`) drawn small and muted, so they read as markers, not text. */
export const BlockIds = Extension.create({
  name: 'blockIds',
  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey('blockIds'),
        state: {
          init: (_, state) => decorate(state.doc),
          apply: (tr, old) => (tr.docChanged ? decorate(tr.doc) : old),
        },
        props: {
          decorations(state) {
            return this.getState(state)
          },
        },
      }),
    ]
  },
})
