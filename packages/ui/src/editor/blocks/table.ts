import { Table as TableIcon } from 'lucide-react'
import { TableKit } from '@tiptap/extension-table'
import type { BlockDef } from './types'

export const table: BlockDef = {
  id: 'table',
  label: 'Table',
  description: '3×3 table with a header row',
  icon: TableIcon,
  group: 'structure',
  keywords: ['grid', 'rows', 'columns', 'spreadsheet'],
  extensions: [TableKit.configure({ table: { resizable: true, lastColumnResizable: false } })],
  insert: (editor) => editor.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run(),
  isActive: (editor) => editor.isActive('table'),
}
