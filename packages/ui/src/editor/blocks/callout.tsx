import { Node, mergeAttributes, type JSONContent, type MarkdownToken } from '@tiptap/core'
import { NodeViewContent, NodeViewWrapper, ReactNodeViewRenderer, type NodeViewProps } from '@tiptap/react'
import {
  Bug,
  Check,
  ChevronRight,
  CircleAlert,
  CircleHelp,
  ClipboardList,
  Flame,
  Info,
  Lightbulb,
  List,
  MessageSquareWarning,
  Pencil,
  Quote,
  TriangleAlert,
  X,
  Zap,
  type LucideIcon,
} from 'lucide-react'
import { useState } from 'react'
import type { BlockDef } from './types'

/**
 * Obsidian-style callouts: `> [!tip] Title`, optionally foldable with
 * `[!tip]+` (starts open) or `[!tip]-` (starts closed). GitHub reads the
 * same syntax for its five alert types, and anything else sees a quote.
 */
export interface CalloutKind {
  id: string
  label: string
  icon: LucideIcon
  /** CSS colour token suffix: `--gr-callout-<tone>`. */
  tone: 'blue' | 'teal' | 'green' | 'yellow' | 'orange' | 'red' | 'purple' | 'grey'
  aliases?: string[]
}

export const CALLOUT_KINDS: CalloutKind[] = [
  { id: 'note', label: 'Note', icon: Pencil, tone: 'blue' },
  { id: 'info', label: 'Info', icon: Info, tone: 'blue' },
  { id: 'tip', label: 'Tip', icon: Lightbulb, tone: 'teal', aliases: ['hint', 'important'] },
  { id: 'abstract', label: 'Summary', icon: ClipboardList, tone: 'teal', aliases: ['summary', 'tldr'] },
  { id: 'todo', label: 'To do', icon: List, tone: 'blue' },
  { id: 'success', label: 'Success', icon: Check, tone: 'green', aliases: ['check', 'done'] },
  { id: 'question', label: 'Question', icon: CircleHelp, tone: 'yellow', aliases: ['help', 'faq'] },
  { id: 'warning', label: 'Warning', icon: TriangleAlert, tone: 'orange', aliases: ['caution', 'attention'] },
  { id: 'failure', label: 'Failure', icon: X, tone: 'red', aliases: ['fail', 'missing'] },
  { id: 'danger', label: 'Danger', icon: Zap, tone: 'red', aliases: ['error'] },
  { id: 'bug', label: 'Bug', icon: Bug, tone: 'red' },
  { id: 'example', label: 'Example', icon: Flame, tone: 'purple' },
  { id: 'quote', label: 'Quote', icon: Quote, tone: 'grey', aliases: ['cite'] },
]

const GENERIC: Omit<CalloutKind, 'id' | 'label'> = { icon: MessageSquareWarning, tone: 'blue' }

/** The look of a callout type, by id or alias, case-insensitive. Unknown types look like a note. */
export function calloutKind(id: string): CalloutKind {
  const k = id.toLowerCase()
  return CALLOUT_KINDS.find((c) => c.id === k || c.aliases?.includes(k)) ?? { ...GENERIC, id: k, label: id ? id[0].toUpperCase() + id.slice(1) : 'Note', icon: CircleAlert }
}

export type CalloutFold = '' | '+' | '-'

const HEADER_RE = /^ {0,3}> ?\[!([A-Za-z][\w-]*)\]([+-]?)[ \t]*(.*)$/
const QUOTE_LINE_RE = /^ {0,3}>/

/** Split a callout off the start of `src`: its header fields and the body with the `>` markers removed. */
export function matchCallout(src: string): { raw: string; kind: string; fold: CalloutFold; title: string; body: string } | null {
  const lines = src.split('\n')
  const head = HEADER_RE.exec(lines[0])
  if (!head) return null
  let n = 1
  while (n < lines.length && QUOTE_LINE_RE.test(lines[n])) n++
  const raw = lines.slice(0, n).join('\n') + (n < lines.length ? '\n' : '')
  const body = lines
    .slice(1, n)
    .map((l) => l.replace(/^ {0,3}> ?/, ''))
    .join('\n')
  return { raw, kind: head[1], fold: head[2] as CalloutFold, title: head[3].trim(), body }
}

function CalloutView({ node, updateAttributes, editor }: NodeViewProps) {
  const kind = calloutKind(String(node.attrs.kind ?? 'note'))
  const fold = (node.attrs.fold ?? '') as CalloutFold
  const title = String(node.attrs.title ?? '')
  const [closed, setClosed] = useState(fold === '-')
  const editable = editor.isEditable
  const Icon = kind.icon
  const known = CALLOUT_KINDS.some((c) => c.id === kind.id)
  return (
    <NodeViewWrapper className={`gr-callout gr-callout--${kind.tone} ${fold && closed ? 'is-closed' : ''}`} data-callout={kind.id}>
      <div className="gr-callout__head" contentEditable={false}>
        {fold ? (
          <button type="button" className="gr-callout__fold" aria-label={closed ? 'Open' : 'Fold'} aria-expanded={!closed} onClick={() => setClosed((c) => !c)}>
            <ChevronRight size={14} />
          </button>
        ) : null}
        <Icon size={16} className="gr-callout__icon" aria-hidden />
        {editable ? (
          <>
            <input className="gr-callout__title" value={title} placeholder={kind.label} aria-label="Callout title" onChange={(e) => updateAttributes({ title: e.target.value })} />
            <span className="gr-callout__controls">
              <select className="gr-callout__kind" value={known ? kind.id : String(node.attrs.kind)} aria-label="Callout type" onChange={(e) => updateAttributes({ kind: e.target.value })}>
                {!known && <option value={String(node.attrs.kind)}>{kind.label}</option>}
                {CALLOUT_KINDS.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.label}
                  </option>
                ))}
              </select>
              <select
                className="gr-callout__foldmode"
                value={fold}
                aria-label="Folding"
                onChange={(e) => {
                  const v = e.target.value as CalloutFold
                  updateAttributes({ fold: v })
                  setClosed(v === '-')
                }}
              >
                <option value="">Always open</option>
                <option value="+">Folds, starts open</option>
                <option value="-">Folds, starts closed</option>
              </select>
            </span>
          </>
        ) : (
          <span className="gr-callout__title gr-callout__title--read">{title || kind.label}</span>
        )}
      </div>
      <NodeViewContent className="gr-callout__body" />
    </NodeViewWrapper>
  )
}

export const Callout = Node.create({
  name: 'callout',
  group: 'block',
  content: 'block+',
  defining: true,
  markdownTokenName: 'callout',

  addAttributes() {
    return {
      kind: { default: 'note' },
      title: { default: '' },
      fold: { default: '' },
    }
  },
  parseHTML() {
    return [
      {
        tag: 'div[data-callout]',
        contentElement: '.gr-callout__body',
        getAttrs: (el) => {
          const e = el as HTMLElement
          return { kind: e.getAttribute('data-callout') ?? 'note', title: e.getAttribute('data-title') ?? '', fold: e.getAttribute('data-fold') ?? '' }
        },
      },
    ]
  },
  renderHTML({ node, HTMLAttributes }) {
    return ['div', mergeAttributes(HTMLAttributes, { 'data-callout': node.attrs.kind, 'data-title': node.attrs.title, 'data-fold': node.attrs.fold }), ['div', { class: 'gr-callout__body' }, 0]]
  },
  addNodeView() {
    return ReactNodeViewRenderer(CalloutView)
  },

  markdownTokenizer: {
    name: 'callout',
    level: 'block',
    start: (src) => {
      const m = /(^|\n) {0,3}> ?\[!/.exec(src)
      return m ? m.index + m[1].length : -1
    },
    tokenize: (src, _tokens, lexer) => {
      const m = matchCallout(src)
      if (!m) return undefined
      return { type: 'callout', raw: m.raw, kind: m.kind, fold: m.fold, title: m.title, tokens: m.body.trim() ? lexer.blockTokens(m.body) : [] }
    },
  },
  parseMarkdown: (token, h) => {
    const parse = h.parseBlockChildren ?? h.parseChildren
    let content = parse((token.tokens ?? []) as MarkdownToken[]) as JSONContent[]
    content = content.filter((c) => c.type !== 'paragraph' || (c.content?.length ?? 0) > 0 || content.length === 1)
    if (content.length === 0) content = [h.createNode('paragraph', {}, [])]
    return h.createNode('callout', { kind: token.kind ?? 'note', fold: token.fold ?? '', title: token.title ?? '' }, content)
  },
  renderMarkdown: (node, h) => {
    const attrs = (node.attrs ?? {}) as { kind?: string; fold?: string; title?: string }
    const header = `> [!${attrs.kind || 'note'}]${attrs.fold ?? ''}${attrs.title ? ` ${attrs.title}` : ''}`
    const children = (node.content ?? []) as JSONContent[]
    const body = children
      .map((child, i) => (h.renderChild?.(child, i) ?? h.renderChildren([child])).split('\n').map((l) => (l.trim() === '' ? '>' : `> ${l}`)).join('\n'))
      .filter((s) => s !== '>')
    return body.length ? `${header}\n${body.join('\n>\n')}` : header
  },
})

export const callout: BlockDef = {
  id: 'callout',
  label: 'Callout',
  description: 'A coloured box for a tip, warning, note or question',
  icon: MessageSquareWarning,
  group: 'text',
  keywords: ['callout', 'admonition', 'tip', 'warning', 'note', 'info', 'alert', 'box', 'details', 'fold', 'collapse'],
  extensions: [Callout],
  insert: (editor) => {
    if (editor.isActive('callout')) return editor.chain().focus().lift('callout').run()
    return editor.chain().focus().wrapIn('callout', { kind: 'note', title: '', fold: '' }).run()
  },
  isActive: (editor) => editor.isActive('callout'),
  toolbar: false,
}
