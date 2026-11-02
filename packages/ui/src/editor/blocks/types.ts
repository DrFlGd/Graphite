import type { AnyExtension, Editor } from '@tiptap/core'
import type { LucideIcon } from 'lucide-react'

/**
 * One insertable block type. The toolbar, the `/` menu and the keyboard
 * shortcuts are all generated from the registry of these, so adding a block
 * type to Graphite is one file in this folder plus one line in `index.ts`
 * (ADR-005).
 */
export interface BlockDef {
  /** Stable id, also used as the React key. */
  id: string
  label: string
  description: string
  icon: LucideIcon
  group: 'text' | 'list' | 'media' | 'structure' | 'link'
  /** Extra words the `/` menu should match on. */
  keywords?: string[]
  /** Human-readable shortcut for display only; the extension binds the real one. */
  shortcut?: string
  /** Extensions this block contributes to the editor (omit when StarterKit already provides them). */
  extensions?: AnyExtension[]
  /** Insert or toggle the block at the current selection. */
  insert: (editor: Editor, ctx: BlockContext) => void | boolean | Promise<void>
  /** Whether the toolbar button should show as active. */
  isActive?: (editor: Editor) => boolean
  /** Hide from the toolbar (still in the `/` menu). */
  toolbar?: boolean
}

/** Host-provided callbacks a block may need (file pickers, note lookup). */
export interface BlockContext {
  /** Ask the host for an image; resolves to a src usable in Markdown (relative path or URL), or null. */
  pickImage?: () => Promise<string | null>
  /** Ask the host for any file; resolves to its Markdown src and display name, or null. */
  pickFile?: () => Promise<{ src: string; name: string } | null>
  /**
   * What a link points at, for embeds (`![[Note#Heading]]`) and hover
   * previews: the note's title and the Markdown of the note, heading section
   * or `^block`. `markdown` is null when the note has no such part; the
   * promise resolves to null when there is no such note.
   */
  loadSection?: (target: string, anchor: string | null) => Promise<SectionPreview | null>
  /** Follow a link from inside an embed or a preview. */
  followLink?: (target: string, anchor: string | null) => void
  /** How deep this editor sits inside embeds (0 = the note itself). Embeds stop at depth 2. */
  embedDepth?: number
}

export interface SectionPreview {
  title: string
  /** Note type, for its colour. */
  type?: string
  markdown: string | null
}
