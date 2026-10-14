import { Link2 } from 'lucide-react'
import type { BlockDef } from './types'

/** Not a block, but it belongs in the insert menus: starts a `[[` note link. */
export const noteLink: BlockDef = {
  id: 'noteLink',
  label: 'Link to note',
  description: 'Type a title; creates the note if it does not exist',
  icon: Link2,
  group: 'link',
  keywords: ['wiki', 'backlink', '[['],
  shortcut: '[[',
  insert: (editor) => editor.chain().focus().insertContent('[[').run(),
}
