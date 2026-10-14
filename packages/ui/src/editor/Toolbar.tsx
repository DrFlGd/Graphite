import type { Editor } from '@tiptap/core'
import { useEditorState } from '@tiptap/react'
import { Bold, Italic, Strikethrough, Code, Link, Undo2, Redo2 } from 'lucide-react'
import type { ReactNode } from 'react'
import { BLOCKS, type BlockContext } from './blocks'

interface ToolbarProps {
  editor: Editor
  context: BlockContext
  /** Extra buttons rendered at the right end (e.g. a raw-Markdown toggle). */
  trailing?: ReactNode
}

interface MarkButton {
  id: string
  label: string
  icon: typeof Bold
  shortcut: string
  run: (editor: Editor) => void
  isActive: (editor: Editor) => boolean
}

const MARKS: MarkButton[] = [
  { id: 'bold', label: 'Bold', icon: Bold, shortcut: 'Ctrl+B', run: (e) => e.chain().focus().toggleBold().run(), isActive: (e) => e.isActive('bold') },
  { id: 'italic', label: 'Italic', icon: Italic, shortcut: 'Ctrl+I', run: (e) => e.chain().focus().toggleItalic().run(), isActive: (e) => e.isActive('italic') },
  { id: 'strike', label: 'Strikethrough', icon: Strikethrough, shortcut: 'Ctrl+Shift+S', run: (e) => e.chain().focus().toggleStrike().run(), isActive: (e) => e.isActive('strike') },
  { id: 'code', label: 'Inline code', icon: Code, shortcut: 'Ctrl+E', run: (e) => e.chain().focus().toggleCode().run(), isActive: (e) => e.isActive('code') },
  {
    id: 'link',
    label: 'Web link',
    icon: Link,
    shortcut: '',
    run: (e) => {
      const prev = e.getAttributes('link').href as string | undefined
      const href = window.prompt('Link URL', prev ?? 'https://')
      if (href === null) return
      if (href === '') e.chain().focus().unsetLink().run()
      else e.chain().focus().extendMarkRange('link').setLink({ href }).run()
    },
    isActive: (e) => e.isActive('link'),
  },
]

/**
 * Icon toolbar generated from the block registry and the inline marks
 * (C11). Every button has a tooltip with its shortcut.
 */
export function Toolbar({ editor, context, trailing }: ToolbarProps) {
  // Re-render on selection/transaction so active states stay current.
  const active = useEditorState({
    editor,
    selector: ({ editor: e }) => ({
      marks: MARKS.map((m) => m.isActive(e)),
      blocks: BLOCKS.map((b) => b.isActive?.(e) ?? false),
      canUndo: e.can().undo(),
      canRedo: e.can().redo(),
    }),
  })

  return (
    <div className="gr-toolbar" role="toolbar" aria-label="Formatting">
      <div className="gr-toolbar__group">
        <ToolButton label="Undo" shortcut="Ctrl+Z" disabled={!active?.canUndo} onClick={() => editor.chain().focus().undo().run()}>
          <Undo2 size={16} />
        </ToolButton>
        <ToolButton label="Redo" shortcut="Ctrl+Shift+Z" disabled={!active?.canRedo} onClick={() => editor.chain().focus().redo().run()}>
          <Redo2 size={16} />
        </ToolButton>
      </div>
      <div className="gr-toolbar__group">
        {MARKS.map((m, i) => (
          <ToolButton key={m.id} label={m.label} shortcut={m.shortcut} active={active?.marks[i]} onClick={() => m.run(editor)}>
            <m.icon size={16} />
          </ToolButton>
        ))}
      </div>
      <div className="gr-toolbar__group">
        {BLOCKS.filter((b) => b.toolbar !== false).map((b) => {
          const i = BLOCKS.indexOf(b)
          return (
            <ToolButton key={b.id} label={b.label} shortcut={b.shortcut} active={active?.blocks[i]} onClick={() => void b.insert(editor, context)}>
              <b.icon size={16} />
            </ToolButton>
          )
        })}
      </div>
      {trailing && <div className="gr-toolbar__group gr-toolbar__trailing">{trailing}</div>}
    </div>
  )
}

export function ToolButton({
  label,
  shortcut,
  active,
  disabled,
  onClick,
  children,
}: {
  label: string
  shortcut?: string
  active?: boolean
  disabled?: boolean
  onClick: () => void
  children: ReactNode
}) {
  const title = shortcut ? `${label} (${shortcut})` : label
  return (
    <button
      type="button"
      className={`gr-tool ${active ? 'is-active' : ''}`}
      title={title}
      aria-label={label}
      aria-pressed={active}
      disabled={disabled}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
    >
      {children}
    </button>
  )
}
