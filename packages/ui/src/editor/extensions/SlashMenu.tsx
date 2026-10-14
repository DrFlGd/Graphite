import { Extension } from '@tiptap/core'
import { PluginKey } from '@tiptap/pm/state'
import Suggestion from '@tiptap/suggestion'
import { createElement } from 'react'
import { filterBlocks, type BlockContext, type BlockDef } from '../blocks'
import { suggestionMenu } from './SuggestionMenu'

export interface SlashMenuOptions {
  context: BlockContext
}

/**
 * Type `/` at the start of a line (or after a space) to get the block
 * picker. Items come straight from the block registry (C11).
 */
export const SlashMenu = Extension.create<SlashMenuOptions>({
  name: 'slashMenu',

  addOptions() {
    return { context: {} }
  },

  addProseMirrorPlugins() {
    return [
      Suggestion<BlockDef, BlockDef>({
        pluginKey: new PluginKey('slashMenu'),
        editor: this.editor,
        char: '/',
        allowSpaces: false,
        startOfLine: false,
        allowedPrefixes: [' ', ''],
        items: ({ query }) => filterBlocks(query).filter((b) => b.id !== 'paragraph' || query.length > 0),
        command: ({ editor, range, props: block }) => {
          editor.chain().focus().deleteRange(range).run()
          void block.insert(editor, this.options.context)
        },
        allow: ({ state, range }) => {
          // Only trigger when the `/` is the first thing in the textblock or follows whitespace.
          const $from = state.doc.resolve(range.from)
          const before = $from.parent.textBetween(0, $from.parentOffset, undefined, ' ')
          return before.trim() === '' && !state.doc.resolve(range.from).parent.type.spec.code
        },
        render: suggestionMenu<BlockDef>({
          emptyLabel: 'No matching block',
          className: 'gr-menu--blocks',
          toView: (b) => ({
            key: b.id,
            icon: createElement(b.icon, { size: 16 }),
            label: b.label,
            description: b.description,
            hint: b.shortcut,
          }),
        }),
      }),
    ]
  },
})
