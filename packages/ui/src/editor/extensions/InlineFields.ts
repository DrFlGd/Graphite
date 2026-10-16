import { Extension } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import { Decoration, DecorationSet } from '@tiptap/pm/view'

const FIELD = /(^|\s)([A-Za-z][\w-]*)::(?=\s|$)/g

/** Character ranges of `key::` markers and their values within one string. */
export function fieldRanges(text: string): { key: [number, number]; value: [number, number] | null }[] {
  const out: { key: [number, number]; value: [number, number] | null }[] = []
  const matches = [...text.matchAll(FIELD)]
  matches.forEach((m, i) => {
    const start = m.index + m[1].length
    const keyEnd = m.index + m[0].length
    const next = i + 1 < matches.length ? matches[i + 1].index + matches[i + 1][1].length : text.length
    const raw = text.slice(keyEnd, next)
    const lead = raw.length - raw.trimStart().length
    const value = raw.trim()
    out.push({ key: [start, keyEnd], value: value ? [keyEnd + lead, keyEnd + lead + value.length] : null })
  })
  return out
}

function decorate(doc: PMNode): DecorationSet {
  const decos: Decoration[] = []
  doc.descendants((node, pos) => {
    if (node.type.name === 'codeBlock') return false
    if (!node.isText || !node.text || node.marks.some((m) => m.type.name === 'code')) return
    for (const r of fieldRanges(node.text)) {
      decos.push(Decoration.inline(pos + r.key[0], pos + r.key[1], { class: 'gr-field__key' }))
      if (r.value) decos.push(Decoration.inline(pos + r.value[0], pos + r.value[1], { class: 'gr-field__value' }))
    }
  })
  return DecorationSet.create(doc, decos)
}

/**
 * Inline fields (`due:: 2026-10-16`, `project:: [[Kitchen remodel]]`) drawn
 * as small labelled chips, so they read as metadata rather than as part of
 * the sentence. The text itself is unchanged.
 */
export const InlineFields = Extension.create({
  name: 'inlineFields',
  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey('inlineFields'),
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
