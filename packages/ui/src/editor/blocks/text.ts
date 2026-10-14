import { Heading1, Heading2, Heading3, Pilcrow, Quote, Minus } from 'lucide-react'
import type { BlockDef } from './types'

const heading = (level: 1 | 2 | 3, icon: BlockDef['icon']): BlockDef => ({
  id: `heading${level}`,
  label: `Heading ${level}`,
  description: level === 1 ? 'Section title' : level === 2 ? 'Sub-section' : 'Small heading',
  icon,
  group: 'text',
  keywords: ['h' + level, 'title'],
  shortcut: `Ctrl+Alt+${level}`,
  insert: (editor) => editor.chain().focus().toggleHeading({ level }).run(),
  isActive: (editor) => editor.isActive('heading', { level }),
})

export const paragraph: BlockDef = {
  id: 'paragraph',
  label: 'Text',
  description: 'Plain paragraph',
  icon: Pilcrow,
  group: 'text',
  keywords: ['p', 'plain', 'body'],
  shortcut: 'Ctrl+Alt+0',
  insert: (editor) => editor.chain().focus().setParagraph().run(),
  isActive: (editor) => editor.isActive('paragraph'),
  toolbar: false,
}

export const heading1 = heading(1, Heading1)
export const heading2 = heading(2, Heading2)
export const heading3 = heading(3, Heading3)

export const blockquote: BlockDef = {
  id: 'blockquote',
  label: 'Quote',
  description: 'Callout or quoted text',
  icon: Quote,
  group: 'text',
  keywords: ['callout', 'note', 'cite'],
  shortcut: 'Ctrl+Shift+B',
  insert: (editor) => editor.chain().focus().toggleBlockquote().run(),
  isActive: (editor) => editor.isActive('blockquote'),
}

export const divider: BlockDef = {
  id: 'divider',
  label: 'Divider',
  description: 'Horizontal rule',
  icon: Minus,
  group: 'structure',
  keywords: ['hr', 'rule', 'separator', 'line'],
  insert: (editor) => editor.chain().focus().setHorizontalRule().run(),
}
