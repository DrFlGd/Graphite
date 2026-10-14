import { Extension } from '@tiptap/core'
import { PluginKey } from '@tiptap/pm/state'
import Suggestion from '@tiptap/suggestion'
import { createElement } from 'react'
import { FileText, Plus } from 'lucide-react'
import { suggestionMenu } from './SuggestionMenu'

export interface NoteCandidate {
  id: string | null
  title: string
  /** Shown under the title (path, type…). */
  subtitle?: string
}

export interface NoteLinkSuggestOptions {
  /** Return notes matching a prefix; the host decides how (index query). */
  search: (query: string) => Promise<NoteCandidate[]>
}

/**
 * `[[` autocomplete. Selecting an item inserts a `wikiLink` node; typing a
 * title that does not exist offers to create it (the link resolves on
 * first follow, so nothing is created until then).
 */
export const NoteLinkSuggest = Extension.create<NoteLinkSuggestOptions>({
  name: 'noteLinkSuggest',

  addOptions() {
    return { search: async () => [] }
  },

  addProseMirrorPlugins() {
    return [
      Suggestion<NoteCandidate, NoteCandidate>({
        pluginKey: new PluginKey('noteLinkSuggest'),
        editor: this.editor,
        char: '[[',
        allowSpaces: true,
        allowedPrefixes: null,
        items: async ({ query }) => {
          const found = await this.options.search(query)
          const exact = found.some((n) => n.title.toLowerCase() === query.trim().toLowerCase())
          if (query.trim() && !exact) found.push({ id: null, title: query.trim(), subtitle: 'Create new note' })
          return found
        },
        command: ({ editor, range, props }) => {
          // Swallow a `]]` the user may already have typed after the caret.
          const after = editor.state.doc.textBetween(range.to, Math.min(range.to + 2, editor.state.doc.content.size), undefined, '')
          const to = after === ']]' ? range.to + 2 : range.to
          editor
            .chain()
            .focus()
            .deleteRange({ from: range.from, to })
            .insertContent([{ type: 'wikiLink', attrs: { target: props.title, heading: null, display: null } }, { type: 'text', text: ' ' }])
            .run()
        },
        render: suggestionMenu<NoteCandidate>({
          emptyLabel: 'Type a note title',
          className: 'gr-menu--notes',
          toView: (n) => ({
            key: n.id ?? `new:${n.title}`,
            icon: createElement(n.id ? FileText : Plus, { size: 16 }),
            label: n.title,
            description: n.subtitle,
          }),
        }),
      }),
    ]
  },
})
