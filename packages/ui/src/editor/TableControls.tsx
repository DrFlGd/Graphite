import type { Editor } from '@tiptap/core'
import { useEditorState } from '@tiptap/react'
import { Columns3, Rows3, Trash2, BetweenHorizontalStart, BetweenVerticalStart, Heading } from 'lucide-react'
import { ToolButton } from './Toolbar'

/** Row/column controls shown while the caret is inside a table (C12). */
export function TableControls({ editor }: { editor: Editor }) {
  const inTable = useEditorState({ editor, selector: ({ editor: e }) => e.isActive('table') })
  if (!inTable) return null
  const c = () => editor.chain().focus()
  return (
    <div className="gr-table-controls" role="toolbar" aria-label="Table">
      <ToolButton label="Add row below" onClick={() => c().addRowAfter().run()}>
        <BetweenHorizontalStart size={16} />
      </ToolButton>
      <ToolButton label="Add column after" onClick={() => c().addColumnAfter().run()}>
        <BetweenVerticalStart size={16} />
      </ToolButton>
      <ToolButton label="Delete row" onClick={() => c().deleteRow().run()}>
        <Rows3 size={16} />
      </ToolButton>
      <ToolButton label="Delete column" onClick={() => c().deleteColumn().run()}>
        <Columns3 size={16} />
      </ToolButton>
      <ToolButton label="Toggle header row" onClick={() => c().toggleHeaderRow().run()}>
        <Heading size={16} />
      </ToolButton>
      <ToolButton label="Delete table" onClick={() => c().deleteTable().run()}>
        <Trash2 size={16} />
      </ToolButton>
    </div>
  )
}
