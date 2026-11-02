import { Node, mergeAttributes, type Editor, type JSONContent, type MarkdownToken } from '@tiptap/core'
import { NodeViewContent, NodeViewWrapper, ReactNodeViewRenderer, type NodeViewProps } from '@tiptap/react'
import { Superscript } from 'lucide-react'
import type { BlockDef } from './types'

/**
 * Footnotes as Obsidian, GitHub and Pandoc write them: `[^1]` in the text and
 * `[^1]: the note` on a line of its own (usually at the end). Consecutive
 * definitions form one `footnotes` block so they stay on adjacent lines.
 */
const DEF_RE = /^ {0,3}\[\^([^\]\s]+)\]:[ \t]?(.*)$/
const REF_RE = /^\[\^([^\]\s]+)\](?!:)/

export function matchFootnotes(src: string): { raw: string; defs: { label: string; text: string }[] } | null {
  const lines = src.split('\n')
  const defs: { label: string; text: string }[] = []
  let n = 0
  while (n < lines.length) {
    const m = DEF_RE.exec(lines[n])
    if (m) {
      defs.push({ label: m[1], text: m[2] })
      n++
      continue
    }
    // An indented line continues the definition above it.
    if (defs.length && /^( {4}|\t)\S/.test(lines[n])) {
      defs[defs.length - 1].text += ' ' + lines[n].trim()
      n++
      continue
    }
    break
  }
  if (!defs.length) return null
  return { raw: lines.slice(0, n).join('\n') + (n < lines.length ? '\n' : ''), defs }
}

/** The text of the definition for `label`, for the reference's tooltip. */
function definitionText(editor: Editor, label: string): string {
  let text = ''
  editor.state.doc.descendants((n) => {
    if (text) return false
    if (n.type.name === 'footnoteDef' && n.attrs.label === label) {
      text = n.textContent
      return false
    }
    return true
  })
  return text
}

function scrollToFootnote(editor: Editor, selector: string) {
  const el = editor.view.dom.querySelector(selector) as HTMLElement | null
  el?.scrollIntoView({ block: 'center', behavior: 'smooth' })
  el?.classList.add('is-flash')
  window.setTimeout(() => el?.classList.remove('is-flash'), 900)
}

function RefView({ node, editor }: NodeViewProps) {
  const label = String(node.attrs.label)
  return (
    <NodeViewWrapper as="sup" className="gr-footnote-ref" data-footnote-ref={label}>
      <button type="button" contentEditable={false} title={definitionText(editor, label) || `Footnote ${label}`} onClick={() => scrollToFootnote(editor, `[data-footnote-def="${CSS.escape(label)}"]`)}>
        {label}
      </button>
    </NodeViewWrapper>
  )
}

function DefView({ node, editor }: NodeViewProps) {
  const label = String(node.attrs.label)
  return (
    <NodeViewWrapper className="gr-footnote-def" data-footnote-def={label}>
      <button type="button" className="gr-footnote-def__label" contentEditable={false} title="Back to the text" onClick={() => scrollToFootnote(editor, `[data-footnote-ref="${CSS.escape(label)}"]`)}>
        {label}
      </button>
      <NodeViewContent as={'span' as never} className="gr-footnote-def__text" />
    </NodeViewWrapper>
  )
}

export const FootnoteRef = Node.create({
  name: 'footnoteRef',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  markdownTokenName: 'footnoteRef',
  addAttributes() {
    return { label: { default: '1' } }
  },
  parseHTML() {
    return [{ tag: 'sup[data-footnote-ref]', getAttrs: (el) => ({ label: (el as HTMLElement).getAttribute('data-footnote-ref') ?? '1' }) }]
  },
  renderHTML({ node, HTMLAttributes }) {
    return ['sup', mergeAttributes(HTMLAttributes, { 'data-footnote-ref': node.attrs.label }), String(node.attrs.label)]
  },
  renderText({ node }) {
    return `[^${node.attrs.label}]`
  },
  addNodeView() {
    return ReactNodeViewRenderer(RefView)
  },
  markdownTokenizer: {
    name: 'footnoteRef',
    level: 'inline',
    start: (src) => src.indexOf('[^'),
    tokenize: (src) => {
      const m = REF_RE.exec(src)
      return m ? { type: 'footnoteRef', raw: m[0], label: m[1] } : undefined
    },
  },
  parseMarkdown: (token, h) => h.createNode('footnoteRef', { label: token.label }),
  renderMarkdown: (node) => `[^${node.attrs?.label ?? '1'}]`,
})

export const FootnoteDef = Node.create({
  name: 'footnoteDef',
  content: 'inline*',
  defining: true,
  addAttributes() {
    return { label: { default: '1' } }
  },
  parseHTML() {
    return [{ tag: 'div[data-footnote-def]', contentElement: '.gr-footnote-def__text', getAttrs: (el) => ({ label: (el as HTMLElement).getAttribute('data-footnote-def') ?? '1' }) }]
  },
  renderHTML({ node, HTMLAttributes }) {
    return ['div', mergeAttributes(HTMLAttributes, { 'data-footnote-def': node.attrs.label }), ['span', { class: 'gr-footnote-def__text' }, 0]]
  },
  addNodeView() {
    return ReactNodeViewRenderer(DefView)
  },
  addKeyboardShortcuts() {
    return {
      // Enter in a definition starts the next footnote rather than splitting this one.
      Enter: ({ editor }) => {
        const { $from } = editor.state.selection
        if ($from.parent.type.name !== 'footnoteDef') return false
        const after = $from.after()
        return editor.chain().insertContentAt(after, { type: 'footnoteDef', attrs: { label: nextFootnoteLabel(editor) } }).setTextSelection(after + 1).run()
      },
    }
  },
})

export const Footnotes = Node.create({
  name: 'footnotes',
  group: 'block',
  content: 'footnoteDef+',
  markdownTokenName: 'footnotes',
  parseHTML() {
    return [{ tag: 'section[data-footnotes]' }]
  },
  renderHTML({ HTMLAttributes }) {
    return ['section', mergeAttributes(HTMLAttributes, { 'data-footnotes': '', class: 'gr-footnotes' }), 0]
  },
  markdownTokenizer: {
    name: 'footnotes',
    level: 'block',
    start: (src) => {
      const m = /(^|\n) {0,3}\[\^[^\]\s]+\]:/.exec(src)
      return m ? m.index + m[1].length : -1
    },
    tokenize: (src, _tokens, lexer) => {
      const m = matchFootnotes(src)
      if (!m) return undefined
      return { type: 'footnotes', raw: m.raw, defs: m.defs.map((d) => ({ label: d.label, tokens: lexer.inlineTokens(d.text) })) }
    },
  },
  parseMarkdown: (token, h) => {
    const defs = (token.defs ?? []) as { label: string; tokens: MarkdownToken[] }[]
    return h.createNode(
      'footnotes',
      {},
      defs.map((d) => h.createNode('footnoteDef', { label: d.label }, h.parseInline(d.tokens) as JSONContent[])),
    )
  },
  renderMarkdown: (node, h) =>
    ((node.content ?? []) as JSONContent[]).map((d) => `[^${(d.attrs?.label as string) ?? '1'}]: ${h.renderChildren((d.content ?? []) as JSONContent[])}`).join('\n'),
})

/** One more than the largest numeric label in the note. */
export function nextFootnoteLabel(editor: Editor): string {
  let max = 0
  editor.state.doc.descendants((n) => {
    if (n.type.name === 'footnoteRef' || n.type.name === 'footnoteDef') {
      const v = Number(n.attrs.label)
      if (Number.isInteger(v) && v > max) max = v
    }
    return true
  })
  return String(max + 1)
}

/** Put `[^n]` at the cursor and an empty `[^n]:` at the end of the note, then move there to type it. */
export function insertFootnote(editor: Editor): boolean {
  const label = nextFootnoteLabel(editor)
  if (!editor.chain().focus().insertContent({ type: 'footnoteRef', attrs: { label } }).run()) return false
  // Add to the last list of definitions if the note has one, else start one at the end.
  let list: { pos: number; size: number } | null = null
  editor.state.doc.forEach((n, pos) => {
    if (n.type.name === 'footnotes') list = { pos, size: n.nodeSize }
  })
  const found = list as { pos: number; size: number } | null
  if (found) {
    const at = found.pos + found.size - 1
    editor.chain().insertContentAt(at, { type: 'footnoteDef', attrs: { label } }).setTextSelection(at + 1).scrollIntoView().run()
  } else {
    const at = editor.state.doc.content.size
    editor.chain().insertContentAt(at, { type: 'footnotes', content: [{ type: 'footnoteDef', attrs: { label } }] }).setTextSelection(at + 2).scrollIntoView().run()
  }
  return true
}

export const footnote: BlockDef = {
  id: 'footnote',
  label: 'Footnote',
  description: 'A numbered note at the end; [^1] in the text',
  icon: Superscript,
  group: 'structure',
  keywords: ['footnote', 'reference', 'citation', 'note', '[^'],
  extensions: [FootnoteRef, FootnoteDef, Footnotes],
  insert: (editor) => insertFootnote(editor),
  toolbar: false,
}
