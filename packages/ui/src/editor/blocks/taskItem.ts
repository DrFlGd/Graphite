// Task items with the five states Graphite writes (see the vault spec,
// "Tasks and gaps"): `[ ]` open, `[/]` doing, `[x]` done, `[-]` cancelled
// and `[?]` a gap (something to find out). Tiptap's TaskItem only knows
// checked/unchecked, so this extends it with a `state` attribute, a node
// view that shows the state, and Markdown parse/render for every marker.

import { renderNestedMarkdownContent, wrappingInputRule, type JSONContent } from '@tiptap/core'
import TaskItem from '@tiptap/extension-task-item'

export type TaskState = 'open' | 'doing' | 'done' | 'cancelled' | 'gap'
export const TASK_STATES: TaskState[] = ['open', 'doing', 'done', 'cancelled', 'gap']

export const MARKERS: Record<TaskState, string> = { open: ' ', doing: '/', done: 'x', cancelled: '-', gap: '?' }
const BY_MARKER: Record<string, TaskState> = { ' ': 'open', '/': 'doing', x: 'done', X: 'done', '-': 'cancelled', '?': 'gap' }

/**
 * The Markdown parser only treats `[ ]` and `[x]` as task boxes, so before
 * parsing, other markers are rewritten to `[ ]` with the real marker tucked
 * behind a private-use character; `parseMarkdown` below reads it back.
 */
const SENTINEL = ''
export function prepareTaskMarkers(markdown: string): string {
  let inFence = false
  return markdown
    .split('\n')
    .map((line) => {
      if (/^\s*(```|~~~)/.test(line)) inFence = !inFence
      if (inFence) return line
      return line.replace(/^(\s*(?:[-*+]|\d+[.)])\s+)\[([/?-])\]\s/, (_m, lead: string, marker: string) => `${lead}[ ] ${SENTINEL}${marker}`)
    })
    .join('\n')
}

const stripSentinel = (text: string): { text: string; state: TaskState | null } => {
  if (text.startsWith(SENTINEL) && text.length >= 2) {
    const state = BY_MARKER[text[1]] ?? null
    return { text: text.slice(2).replace(/^ /, ''), state }
  }
  return { text, state: null }
}

const nextState = (s: TaskState): TaskState => (s === 'open' ? 'doing' : s === 'doing' ? 'done' : s === 'done' ? 'cancelled' : s === 'cancelled' ? 'open' : 'gap')

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    graphiteTaskItem: {
      /** Set the state of the task item around the selection. */
      setTaskState: (state: TaskState) => ReturnType
      /** open → doing → done → cancelled → open (gaps stay gaps). */
      cycleTaskState: () => ReturnType
      /** Start a new `- [?]` line (or turn the current list item into one). */
      insertGap: () => ReturnType
    }
  }
}

export const GraphiteTaskItem = TaskItem.extend({
  addAttributes() {
    return {
      state: {
        default: 'open' as TaskState,
        keepOnSplit: false,
        parseHTML: (el: HTMLElement) => {
          const s = el.getAttribute('data-state')
          if (s && (TASK_STATES as string[]).includes(s)) return s
          return el.getAttribute('data-checked') === 'true' ? 'done' : 'open'
        },
        renderHTML: (attrs: { state?: TaskState }) => ({ 'data-state': attrs.state ?? 'open', 'data-checked': attrs.state === 'done' ? 'true' : 'false' }),
      },
      // Kept for anything that reads Tiptap's own attribute; derived from `state`.
      checked: {
        default: false,
        keepOnSplit: false,
        parseHTML: (el: HTMLElement) => el.getAttribute('data-state') === 'done' || el.getAttribute('data-checked') === 'true',
        renderHTML: () => ({}),
      },
    }
  },

  parseMarkdown(token, h) {
    let state: TaskState = token.checked ? 'done' : 'open'
    const tokens = (token.tokens ?? []) as { type: string; text?: string; raw?: string }[]
    if (tokens.length > 0) {
      const first = tokens[0]
      if (typeof first.text === 'string') {
        const s = stripSentinel(first.text)
        if (s.state) {
          state = s.state
          first.text = s.text
          if (typeof first.raw === 'string') first.raw = stripSentinel(first.raw).text
        }
      }
    } else if (typeof token.text === 'string') {
      const s = stripSentinel(token.text)
      if (s.state) {
        state = s.state
        token.text = s.text
      }
    }
    const content: JSONContent[] = []
    if (tokens.length > 0) content.push(h.createNode('paragraph', {}, h.parseInline(tokens as never)))
    else if (token.text) content.push(h.createNode('paragraph', {}, [h.createNode('text', { text: token.text })]))
    else content.push(h.createNode('paragraph', {}, []))
    const nested = token.nestedTokens as unknown[] | undefined
    if (nested && nested.length > 0) content.push(...(h.parseChildren(nested as never) as JSONContent[]))
    return h.createNode('taskItem', { state, checked: state === 'done' }, content)
  },

  renderMarkdown(node, h) {
    const state = ((node as { attrs?: { state?: TaskState } }).attrs?.state ?? 'open') as TaskState
    return renderNestedMarkdownContent(node, h, `- [${MARKERS[state]}] `)
  },

  addCommands() {
    const set = (state: TaskState) => ({ tr, state: editorState }: { tr: import('@tiptap/pm/state').Transaction; state: import('@tiptap/pm/state').EditorState }) => {
      const { $from } = editorState.selection
      for (let d = $from.depth; d > 0; d--) {
        const node = $from.node(d)
        if (node.type.name === 'taskItem') {
          tr.setNodeMarkup($from.before(d), undefined, { ...node.attrs, state, checked: state === 'done' })
          return true
        }
      }
      return false
    }
    return {
      setTaskState:
        (state) =>
        ({ tr, state: s }) =>
          set(state)({ tr, state: s }),
      cycleTaskState:
        () =>
        ({ tr, state: s }) => {
          const { $from } = s.selection
          for (let d = $from.depth; d > 0; d--) {
            const node = $from.node(d)
            if (node.type.name === 'taskItem') {
              const next = nextState(node.attrs.state as TaskState)
              tr.setNodeMarkup($from.before(d), undefined, { ...node.attrs, state: next, checked: next === 'done' })
              return true
            }
          }
          return false
        },
      insertGap:
        () =>
        ({ chain, editor }) => {
          if (editor.isActive('taskItem')) return chain().setTaskState('gap').run()
          return chain().toggleTaskList().setTaskState('gap').run()
        },
    }
  },

  addKeyboardShortcuts() {
    return {
      ...this.parent?.(),
      'Mod-Enter': () => this.editor.commands.cycleTaskState(),
    }
  },

  addInputRules() {
    return [
      wrappingInputRule({
        find: /^\s*\[([ xX/?-])\]\s$/,
        type: this.type,
        getAttributes: (match) => {
          const state = BY_MARKER[match[1]] ?? 'open'
          return { state, checked: state === 'done' }
        },
      }),
    ]
  },

  addNodeView() {
    return ({ node, HTMLAttributes, getPos, editor }) => {
      const li = document.createElement('li')
      li.dataset.type = 'taskItem'
      const control = document.createElement('label')
      control.contentEditable = 'false'
      control.className = 'gr-task__control'
      const box = document.createElement('input')
      box.type = 'checkbox'
      box.addEventListener('mousedown', (e) => e.preventDefault())
      const gapMark = document.createElement('button')
      gapMark.type = 'button'
      gapMark.className = 'gr-task__gap'
      gapMark.textContent = '?'
      gapMark.title = 'Open question: something to find out. Click to turn into a task.'
      gapMark.addEventListener('mousedown', (e) => e.preventDefault())
      const content = document.createElement('div')

      const setState = (state: TaskState) => {
        if (!editor.isEditable || typeof getPos !== 'function') return
        editor
          .chain()
          .focus(undefined, { scrollIntoView: false })
          .command(({ tr }) => {
            const pos = getPos()
            if (typeof pos !== 'number') return false
            const current = tr.doc.nodeAt(pos)
            if (!current) return false
            tr.setNodeMarkup(pos, undefined, { ...current.attrs, state, checked: state === 'done' })
            return true
          })
          .run()
      }
      box.addEventListener('change', () => {
        const state = li.dataset.state as TaskState
        setState(state === 'done' ? 'open' : 'done')
      })
      box.addEventListener('contextmenu', (e) => {
        e.preventDefault()
        setState(nextState(li.dataset.state as TaskState))
      })
      gapMark.addEventListener('click', () => setState('open'))

      const render = (n: typeof node) => {
        const state = (n.attrs.state as TaskState) ?? 'open'
        li.dataset.state = state
        li.dataset.checked = state === 'done' ? 'true' : 'false'
        control.replaceChildren(state === 'gap' ? gapMark : box)
        box.checked = state === 'done'
        box.indeterminate = state === 'doing'
        box.title = state === 'doing' ? 'In progress (right-click to change)' : state === 'cancelled' ? 'Cancelled (right-click to change)' : 'Right-click to change state'
        box.setAttribute('aria-label', `${state} task: ${n.textContent || 'empty'}`)
      }
      Object.entries(HTMLAttributes).forEach(([k, v]) => {
        if (v !== null && v !== undefined) li.setAttribute(k, String(v))
      })
      render(node)
      li.append(control, content)
      return {
        dom: li,
        contentDOM: content,
        update: (updated) => {
          if (updated.type !== node.type) return false
          render(updated)
          return true
        },
      }
    }
  },
})
