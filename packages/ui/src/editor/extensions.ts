import type { AnyExtension } from '@tiptap/core'
import StarterKit from '@tiptap/starter-kit'
import Placeholder from '@tiptap/extension-placeholder'
import { Markdown } from '@tiptap/markdown'
import { blockExtensions, type BlockContext } from './blocks'
import { VaultImage } from './blocks/image'
import { FileEmbed } from './blocks/fileEmbed'
import { WikiLink } from './extensions/WikiLink'
import { SlashMenu } from './extensions/SlashMenu'
import { NoteLinkSuggest, type NoteCandidate } from './extensions/NoteLinkSuggest'

export interface EditorExtensionOptions {
  placeholder?: string
  resolveSrc?: (src: string) => string
  searchNotes?: (query: string) => Promise<NoteCandidate[]>
  blockContext?: BlockContext
  /** Skip the interactive menus (for headless parsing/serialising in tests). */
  headless?: boolean
}

/**
 * The full extension set. The Markdown round-trip (`buildMarkdownExtensions`)
 * and the interactive editor share this so what the editor can show is
 * exactly what it can save.
 */
export function buildExtensions(opts: EditorExtensionOptions = {}): AnyExtension[] {
  const base: AnyExtension[] = [
    StarterKit.configure({
      codeBlock: false, // replaced by the lowlight block with a language picker
      heading: { levels: [1, 2, 3, 4, 5, 6] },
      link: { openOnClick: false, autolink: true },
    }),
    Markdown,
    WikiLink,
    VaultImage.configure({ resolveSrc: opts.resolveSrc, inline: false, allowBase64: false }),
    FileEmbed.configure({ resolveSrc: opts.resolveSrc }),
    ...blockExtensions().filter((e) => e.name !== 'fileEmbed'),
  ]
  if (opts.headless) return base
  return [
    ...base,
    Placeholder.configure({ placeholder: opts.placeholder ?? "Write, or type '/' for blocks and '[[' to link a note" }),
    SlashMenu.configure({ context: opts.blockContext ?? {} }),
    NoteLinkSuggest.configure({ search: opts.searchNotes ?? (async () => []) }),
  ]
}

export function buildMarkdownExtensions(): AnyExtension[] {
  return buildExtensions({ headless: true })
}
