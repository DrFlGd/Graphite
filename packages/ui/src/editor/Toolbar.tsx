import type { Editor } from '@tiptap/core'
import { useEditorState } from '@tiptap/react'
import { Bold, Italic, Strikethrough, Code, Highlighter, Link, Undo2, Redo2, Plus } from 'lucide-react'
import { useEffect, useRef, useState, type ReactNode } from 'react'
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
  { id: 'highlight', label: 'Highlight', icon: Highlighter, shortcut: 'Ctrl+Shift+H', run: (e) => e.chain().focus().toggleHighlight().run(), isActive: (e) => e.isActive('highlight') },
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
        <MoreBlocks editor={editor} context={context} />
      </div>
      {trailing && <div className="gr-toolbar__group gr-toolbar__trailing">{trailing}</div>}
    </div>
  )
}

/** The blocks that have no toolbar button of their own, behind one "Insert" button. */
function MoreBlocks({ editor, context }: { editor: Editor; context: BlockContext }) {
  const [open, setOpen] = useState(false)
  const [at, setAt] = useState<{ top: number; left: number }>({ top: 0, left: 0 })
  const root = useRef<HTMLDivElement>(null)
  const extra = BLOCKS.filter((b) => b.toolbar === false && b.id !== 'paragraph')
  useEffect(() => {
    if (!open) return
    const onDown = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false)
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false)
    }
    window.addEventListener('mousedown', onDown)
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('mousedown', onDown)
      window.removeEventListener('keydown', onKey)
    }
  }, [open])
  if (extra.length === 0) return null
  return (
    <div className="gr-more" ref={root}>
      <ToolButton
        label="Insert"
        active={open}
        onClick={() => {
          // The toolbar scrolls sideways, which would clip a menu inside it, so the menu is fixed to the window.
          const r = root.current?.getBoundingClientRect()
          if (r) setAt({ top: r.bottom + 4, left: Math.max(8, Math.min(r.left, window.innerWidth - 328)) })
          setOpen((o) => !o)
        }}
      >
        <Plus size={16} />
      </ToolButton>
      {open && (
        <div className="gr-more__menu" role="menu" style={{ top: at.top, left: at.left }}>
          {extra.map((b) => (
            <button
              key={b.id}
              type="button"
              role="menuitem"
              className="gr-more__item"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => {
                setOpen(false)
                void b.insert(editor, context)
              }}
            >
              <b.icon size={16} />
              <span className="gr-more__label">{b.label}</span>
              <span className="gr-more__desc">{b.description}</span>
            </button>
          ))}
        </div>
      )}
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
