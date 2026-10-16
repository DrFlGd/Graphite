import { Extension } from '@tiptap/core'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import { Decoration, DecorationSet } from '@tiptap/pm/view'

export interface DuplicateTitleOptions {
  /** The note's title as shown above the editor; read on every redraw. */
  title: () => string | undefined
}

export const duplicateTitleKey = new PluginKey('duplicateTitle')

/** Whether `doc` starts with a level-1 heading that repeats `title`. */
export function leadingTitleMatches(doc: { firstChild: { type: { name: string }; attrs: Record<string, unknown>; textContent: string } | null }, title: string | undefined): boolean {
  const first = doc.firstChild
  if (!first || !title || first.type.name !== 'heading' || first.attrs.level !== 1) return false
  const norm = (s: string) => s.trim().replace(/\s+/g, ' ').toLowerCase()
  return norm(first.textContent) === norm(title)
}

/**
 * Many notes start with `# Their title`, which would show the title twice
 * under the title field. The heading stays in the file; the editor just
 * hides it until the cursor moves into it.
 */
export const DuplicateTitle = Extension.create<DuplicateTitleOptions>({
  name: 'duplicateTitle',
  addOptions() {
    return { title: () => undefined }
  },
  addProseMirrorPlugins() {
    const { title } = this.options
    return [
      new Plugin<boolean>({
        key: duplicateTitleKey,
        // Whether the editor has focus: the heading shows while the cursor is in it.
        state: {
          init: () => false,
          apply: (tr, focused) => (tr.getMeta(duplicateTitleKey) as boolean | undefined) ?? focused,
        },
        props: {
          handleDOMEvents: {
            focus: (view) => {
              view.dispatch(view.state.tr.setMeta(duplicateTitleKey, true))
              return false
            },
            blur: (view) => {
              view.dispatch(view.state.tr.setMeta(duplicateTitleKey, false))
              return false
            },
          },
          decorations(state) {
            const { doc, selection } = state
            if (!leadingTitleMatches(doc, title())) return null
            const end = doc.firstChild!.nodeSize
            if (duplicateTitleKey.getState(state) && selection.from < end) return null
            return DecorationSet.create(doc, [Decoration.node(0, end, { class: 'gr-dup-title' })])
          },
        },
      }),
    ]
  },
})
