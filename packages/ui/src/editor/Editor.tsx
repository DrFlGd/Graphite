import type { Editor } from '@tiptap/core'
import { EditorContent, useEditor } from '@tiptap/react'
import { useEffect, useMemo, useRef, type ReactNode } from 'react'
import { buildExtensions } from './extensions'
import { prepareTaskMarkers } from './blocks/taskItem'
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
  /** A file embed or a relative `[text](path)` link was clicked. */
  onOpenFile?: (src: string) => void
  /** An external `[text](https://…)` link was clicked. */
  onOpenUrl?: (href: string) => void
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
  const { markdown, docKey, onChange, onFollowLink, onOpenFile, onOpenUrl, searchNotes, resolveSrc, blockContext, placeholder, editable = true, toolbarTrailing, autofocus, onReady, onFiles } = props
  const lastEmitted = useRef<string>(markdown)
  /**
   * The editor's own serialisation of the text it last loaded. Parsing and
   * re-serialising can differ from the file in harmless ways (blank lines,
   * list spacing); a note that was only looked at must not be rewritten for
   * that, so updates that still serialise to this are not edits.
   */
  const loaded = useRef<{ markdown: string | null; edited: boolean }>({ markdown: null, edited: false })
  const context = useMemo<BlockContext>(() => blockContext ?? {}, [blockContext])
  const callbacks = useRef({ onChange, onFollowLink, onOpenFile, onOpenUrl, searchNotes, onReady, onFiles })
  callbacks.current = { onChange, onFollowLink, onOpenFile, onOpenUrl, searchNotes, onReady, onFiles }

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
      content: prepareTaskMarkers(markdown),
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
          if (node.type.name === 'fileEmbed' && (event.target as HTMLElement | null)?.closest?.('[data-file-open]')) {
            callbacks.current.onOpenFile?.(node.attrs.src as string)
            return true
          }
          return false
        },
        handleClick: (_view, _pos, event) => {
          // Plain Markdown links: a relative path opens the file, a URL leaves the app.
          const a = (event.target as HTMLElement | null)?.closest?.('a[href]') as HTMLAnchorElement | null
          if (!a || event.shiftKey) return false
          const href = a.getAttribute('href') ?? ''
          if (!href || href.startsWith('#')) return false
          event.preventDefault()
          if (/^[a-z][a-z0-9+.-]*:/i.test(href)) callbacks.current.onOpenUrl?.(href)
          else callbacks.current.onOpenFile?.(href.replace(/^\.\//, ''))
          return true
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
      onCreate: ({ editor: e }) => {
        loaded.current = { markdown: e.getMarkdown(), edited: false }
      },
      onUpdate: ({ editor: e, transaction }) => {
        if (!transaction.docChanged) return
        const md = e.getMarkdown()
        if (!loaded.current.edited && md === loaded.current.markdown) return
        loaded.current.edited = true
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
    editor.commands.setContent(prepareTaskMarkers(markdown), { contentType: 'markdown', emitUpdate: false })
    loaded.current = { markdown: editor.getMarkdown(), edited: false }
  }, [markdown, editor])

  useEffect(() => {
    if (editor && !editor.isDestroyed) editor.setEditable(editable, false)
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
