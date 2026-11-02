import { InputRule, Node, mergeAttributes } from '@tiptap/core'
import { NodeViewWrapper, ReactNodeViewRenderer, type NodeViewProps } from '@tiptap/react'
import katex from 'katex'
import 'katex/dist/katex.min.css'
import { Radical, Sigma } from 'lucide-react'
import { useEffect, useMemo, useRef } from 'react'
import type { BlockDef } from './types'

/** KaTeX to HTML; a formula that does not parse shows its source and the error. */
export function renderLatex(latex: string, display: boolean): { html: string; error: string | null } {
  try {
    return { html: katex.renderToString(latex, { displayMode: display, throwOnError: true, output: 'htmlAndMathml', trust: false }), error: null }
  } catch (e) {
    return { html: '', error: e instanceof Error ? e.message.replace(/^KaTeX parse error: /, '') : String(e) }
  }
}

/**
 * Inline `$…$`, with the rules Obsidian and Pandoc use so prices stay prices:
 * the opening `$` is not followed by a space, the closing one is not preceded
 * by a space or followed by a digit, and `\$` is a literal dollar.
 */
export function matchInlineMath(src: string): { raw: string; latex: string } | null {
  if (src[0] !== '$' || src[1] === '$' || src[1] === undefined || /\s/.test(src[1])) return null
  for (let i = 1; i < src.length; i++) {
    const c = src[i]
    if (c === '\n') return null
    if (c === '\\') {
      i++
      continue
    }
    if (c === '$') {
      if (/\s/.test(src[i - 1]) || /\d/.test(src[i + 1] ?? '')) return null
      return { raw: src.slice(0, i + 1), latex: src.slice(1, i) }
    }
  }
  return null
}

const BLOCK_RE = /^ {0,3}\$\$([\s\S]+?)\$\$[ \t]*(?:\n|$)/

function MathView({ node, updateAttributes, editor, selected, deleteNode }: NodeViewProps) {
  const display = node.type.name === 'mathBlock'
  const latex = String(node.attrs.latex ?? '')
  const out = useMemo(() => renderLatex(latex, display), [latex, display])
  const editing = selected && editor.isEditable
  const input = useRef<HTMLTextAreaElement & HTMLInputElement>(null)
  useEffect(() => {
    if (editing) input.current?.focus()
  }, [editing])

  // Leave the formula: Enter (inline) / Ctrl+Enter (block) or Escape put the cursor after it.
  const leave = () => {
    const pos = editor.state.selection.to
    editor.chain().focus().setTextSelection(pos).run()
  }
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape' || (e.key === 'Enter' && (!display || e.ctrlKey || e.metaKey))) {
      e.preventDefault()
      if (!latex.trim()) deleteNode()
      else leave()
    }
  }
  const Tag = display ? 'div' : 'span'
  return (
    <NodeViewWrapper as={Tag} className={`gr-math gr-math--${display ? 'block' : 'inline'} ${editing ? 'is-editing' : ''}`} data-math="">
      {editing &&
        (display ? (
          <textarea ref={input} className="gr-math__source" value={latex} rows={Math.max(2, latex.split('\n').length)} spellCheck={false} placeholder="LaTeX, e.g. E = mc^2" aria-label="Formula" onChange={(e) => updateAttributes({ latex: e.target.value })} onKeyDown={onKeyDown} />
        ) : (
          <input ref={input} className="gr-math__source" value={latex} spellCheck={false} placeholder="LaTeX" aria-label="Formula" size={Math.max(4, latex.length)} onChange={(e) => updateAttributes({ latex: e.target.value })} onKeyDown={onKeyDown} />
        ))}
      {out.error ? (
        <Tag className="gr-math__error" title={out.error}>
          {latex || (display ? 'Empty formula' : '$ $')}
        </Tag>
      ) : (
        <Tag className="gr-math__render" contentEditable={false} dangerouslySetInnerHTML={{ __html: out.html }} />
      )}
    </NodeViewWrapper>
  )
}

/** `$x$` inside a line. */
export const MathInline = Node.create({
  name: 'mathInline',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  markdownTokenName: 'mathInline',

  addAttributes() {
    return { latex: { default: '' } }
  },
  parseHTML() {
    return [{ tag: 'span[data-math-inline]', getAttrs: (el) => ({ latex: (el as HTMLElement).getAttribute('data-latex') ?? '' }) }]
  },
  renderHTML({ node, HTMLAttributes }) {
    return ['span', mergeAttributes(HTMLAttributes, { 'data-math-inline': '', 'data-latex': node.attrs.latex }), `$${node.attrs.latex}$`]
  },
  renderText({ node }) {
    return `$${node.attrs.latex}$`
  },
  addNodeView() {
    return ReactNodeViewRenderer(MathView)
  },
  addInputRules() {
    return [
      new InputRule({
        // `$x$` typed by hand: the closing `$` turns it into a formula.
        find: (text) => {
          const at = text.lastIndexOf('$', text.length - 2)
          if (at < 0 || !text.endsWith('$') || (at > 0 && /[\\$]/.test(text[at - 1]))) return null
          const m = matchInlineMath(text.slice(at))
          return m && m.raw.length === text.length - at ? { index: at, text: m.raw, data: { latex: m.latex } } : null
        },
        handler: ({ state, range, match }) => {
          const latex = (match as unknown as { data?: { latex?: string } }).data?.latex
          if (typeof latex !== 'string') return null
          state.tr.replaceWith(range.from, range.to, this.type.create({ latex }))
        },
      }),
    ]
  },
  markdownTokenizer: {
    name: 'mathInline',
    level: 'inline',
    start: (src) => {
      let i = src.indexOf('$')
      while (i > 0 && src[i - 1] === '\\') i = src.indexOf('$', i + 1)
      return i
    },
    tokenize: (src) => {
      const m = matchInlineMath(src)
      return m ? { type: 'mathInline', raw: m.raw, latex: m.latex } : undefined
    },
  },
  parseMarkdown: (token, h) => h.createNode('mathInline', { latex: token.latex ?? '' }),
  renderMarkdown: (node) => `$${(node.attrs?.latex as string) ?? ''}$`,
})

/** A `$$ … $$` formula on its own lines. */
export const MathBlock = Node.create({
  name: 'mathBlock',
  group: 'block',
  atom: true,
  selectable: true,
  markdownTokenName: 'mathBlock',

  addAttributes() {
    return { latex: { default: '' } }
  },
  parseHTML() {
    return [{ tag: 'div[data-math-block]', getAttrs: (el) => ({ latex: (el as HTMLElement).getAttribute('data-latex') ?? '' }) }]
  },
  renderHTML({ node, HTMLAttributes }) {
    return ['div', mergeAttributes(HTMLAttributes, { 'data-math-block': '', 'data-latex': node.attrs.latex }), `$$${node.attrs.latex}$$`]
  },
  addNodeView() {
    return ReactNodeViewRenderer(MathView)
  },
  markdownTokenizer: {
    name: 'mathBlock',
    level: 'block',
    start: (src) => {
      const m = /(^|\n) {0,3}\$\$/.exec(src)
      return m ? m.index + m[1].length : -1
    },
    tokenize: (src) => {
      const m = BLOCK_RE.exec(src)
      if (!m) return undefined
      return { type: 'mathBlock', raw: m[0], latex: m[1].replace(/^\n/, '').replace(/\n$/, '') }
    },
  },
  parseMarkdown: (token, h) => h.createNode('mathBlock', { latex: token.latex ?? '' }),
  renderMarkdown: (node) => `$$\n${(node.attrs?.latex as string) ?? ''}\n$$`,
})

export const mathBlock: BlockDef = {
  id: 'mathBlock',
  label: 'Formula',
  description: 'Maths on its own line, written in LaTeX ($$ … $$)',
  icon: Sigma,
  group: 'structure',
  keywords: ['math', 'maths', 'equation', 'latex', 'katex', 'tex', '$$'],
  extensions: [MathBlock],
  insert: (editor) => {
    editor.chain().focus().insertContent({ type: 'mathBlock', attrs: { latex: '' } }).run()
    // Select it so the source box opens.
    const pos = editor.state.selection.from - 1
    if (pos >= 0 && editor.state.doc.nodeAt(pos)?.type.name === 'mathBlock') editor.commands.setNodeSelection(pos)
  },
  isActive: (editor) => editor.isActive('mathBlock'),
  toolbar: false,
}

export const mathInline: BlockDef = {
  id: 'mathInline',
  label: 'Inline formula',
  description: 'Maths inside a line of text ($ … $)',
  icon: Radical,
  group: 'text',
  keywords: ['math', 'maths', 'equation', 'latex', 'katex', '$'],
  extensions: [MathInline],
  insert: (editor) => {
    const { from, to } = editor.state.selection
    const selected = editor.state.doc.textBetween(from, to, ' ')
    editor.chain().focus().insertContent({ type: 'mathInline', attrs: { latex: selected } }).run()
    const pos = editor.state.selection.from - 1
    if (pos >= 0 && editor.state.doc.nodeAt(pos)?.type.name === 'mathInline') editor.commands.setNodeSelection(pos)
  },
  isActive: (editor) => editor.isActive('mathInline'),
  toolbar: false,
}
