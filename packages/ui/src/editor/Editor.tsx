import type { Editor } from '@tiptap/core'
import { EditorContent, useEditor } from '@tiptap/react'
import { useEffect, useMemo, useRef, type ReactNode } from 'react'
import { buildExtensions } from './extensions'
import type { BlockContext } from './blocks'
import type { NoteCandidate } from './extensions/NoteLinkSuggest'
import { Toolbar } from './Toolbar'
import { TableControls } from './TableControls'

export interface NoteEditorProps {
  /** Markdown body. When this changes from outside (file watcher, switching notes) the editor reloads. */
  markdown: string
  /** Identity of the document; changing it resets undo history. */
  docKey: string
  onChange: (markdown: string) => void
  onFollowLink?: (target: string, heading: string | null) => void
  searchNotes?: (query: string) => Promise<NoteCandidate[]>
  resolveSrc?: (src: string) => string
  blockContext?: BlockContext
  placeholder?: string
  editable?: boolean
  /** Rendered at the right end of the toolbar. */
  toolbarTrailing?: ReactNode
  autofocus?: boolean
  /** Called with the editor instance once it exists (and again per `docKey`). */
  onReady?: (editor: Editor) => void
  /**
   * Files pasted (or dropped, outside Tauri) into the editor. The host copies
   * them into the vault and calls `insertAttachments` with the result. `pos`
   * is the document position of a drop, if known.
   */
  onFiles?: (files: File[], pos?: number) => void
}

/**
 * The Graphite note editor: Tiptap with the block registry's toolbar, the
 * `/` menu, `[[` note links and table/code controls. Markdown in, Markdown
 * out (ADR-005).
 */
export function NoteEditor(props: NoteEditorProps) {
  const { markdown, docKey, onChange, onFollowLink, searchNotes, resolveSrc, blockContext, placeholder, editable = true, toolbarTrailing, autofocus, onReady, onFiles } = props
  const lastEmitted = useRef<string>(markdown)
  const context = useMemo<BlockContext>(() => blockContext ?? {}, [blockContext])
  const callbacks = useRef({ onChange, onFollowLink, searchNotes, onReady, onFiles })
  callbacks.current = { onChange, onFollowLink, searchNotes, onReady, onFiles }

  const extensions = useMemo(
    () =>
      buildExtensions({
        placeholder,
        resolveSrc,
        blockContext: context,
        searchNotes: (q) => callbacks.current.searchNotes?.(q) ?? Promise.resolve([]),
      }),
    // Extensions are built once per document; options that change are read through refs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [docKey],
  )

  const editor = useEditor(
    {
      extensions,
      content: markdown,
      contentType: 'markdown',
      editable,
      autofocus: autofocus ? 'end' : false,
      editorProps: {
        attributes: { class: 'gr-prose', spellcheck: 'true' },
        handleClickOn: (_view, _pos, node, _nodePos, event) => {
          if (node.type.name === 'wikiLink' && !(event as MouseEvent).shiftKey) {
            callbacks.current.onFollowLink?.(node.attrs.target as string, (node.attrs.heading as string | null) ?? null)
            return true
          }
          return false
        },
        handleKeyDown: (view, event) => {
          // Enter on a selected link chip follows it.
          if (event.key === 'Enter') {
            const { selection } = view.state
            const node = 'node' in selection ? (selection as { node?: { type: { name: string }; attrs: Record<string, unknown> } }).node : undefined
            if (node?.type.name === 'wikiLink') {
              callbacks.current.onFollowLink?.(node.attrs.target as string, (node.attrs.heading as string | null) ?? null)
              return true
            }
          }
          return false
        },
        handlePaste: (_view, event) => {
          const files = Array.from(event.clipboardData?.files ?? [])
          if (files.length === 0 || !callbacks.current.onFiles) return false
          event.preventDefault()
          callbacks.current.onFiles(files)
          return true
        },
        handleDrop: (view, event) => {
          const files = Array.from(event.dataTransfer?.files ?? [])
          if (files.length === 0 || !callbacks.current.onFiles) return false
          event.preventDefault()
          const pos = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos
          callbacks.current.onFiles(files, pos)
          return true
        },
      },
      onUpdate: ({ editor: e }) => {
        const md = e.getMarkdown()
        if (md !== lastEmitted.current) {
          lastEmitted.current = md
          callbacks.current.onChange(md)
        }
      },
    },
    [docKey],
  )

  // External change (watcher, revert): reload without clobbering the user's own edits.
  useEffect(() => {
    if (!editor || editor.isDestroyed) return
    if (markdown === lastEmitted.current) return
    lastEmitted.current = markdown
    editor.commands.setContent(markdown, { contentType: 'markdown', emitUpdate: false })
  }, [markdown, editor])

  useEffect(() => {
    if (editor && !editor.isDestroyed) editor.setEditable(editable)
  }, [editable, editor])

  useEffect(() => {
    if (editor && !editor.isDestroyed) callbacks.current.onReady?.(editor)
  }, [editor])

  if (!editor) return null
  return (
    <div className="gr-editor">
      <Toolbar editor={editor} context={context} trailing={toolbarTrailing} />
      <TableControls editor={editor} />
      <EditorContent editor={editor} className="gr-editor__content" />
    </div>
  )
}
