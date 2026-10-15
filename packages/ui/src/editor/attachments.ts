import type { Editor } from '@tiptap/core'
import { isMediaSrc } from './fileKinds'

/** A file that has already been copied into the vault. */
export interface AttachmentItem {
  /** Vault-relative path, exactly as it should appear in the Markdown. */
  src: string
  /** Shown as the image alt text or the link text. */
  name: string
}

const IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'avif']

export const isImageSrc = (src: string): boolean => IMAGE_EXTS.includes(src.split(/[?#]/)[0].split('.').pop()?.toLowerCase() ?? '')

/**
 * Make a vault path safe as a Markdown link destination: CommonMark does not
 * allow spaces or parentheses there, so they are percent-encoded (the core's
 * link extractor and `assetUrl` decode them again). Already-encoded paths
 * are left alone.
 */
export const encodeSrc = (path: string): string => path.replace(/%(?![0-9A-Fa-f]{2})/g, '%25').replace(/ /g, '%20').replace(/\(/g, '%28').replace(/\)/g, '%29')

/**
 * Insert dropped or pasted files into the note: images become image blocks
 * (`![name](path)`), video, audio, PDFs and 3D models become file embeds
 * (same syntax, previewed inline), anything else a link (`[name](path)`).
 * `pos` is where to insert (e.g. the drop point); by default the current selection.
 */
export function insertAttachments(editor: Editor, items: AttachmentItem[], pos?: number): boolean {
  if (items.length === 0 || editor.isDestroyed) return false
  // Insert after whatever is selected (e.g. a just-inserted image) rather than replacing it.
  let chain = editor.chain().focus(pos ?? editor.state.selection.to)
  for (const it of items) {
    const src = encodeSrc(it.src)
    if (isImageSrc(src)) {
      chain = chain.setImage({ src, alt: it.name })
    } else if (isMediaSrc(src)) {
      chain = chain.insertContent({ type: 'fileEmbed', attrs: { src, name: it.name } })
    } else {
      chain = chain.insertContent([
        { type: 'text', marks: [{ type: 'link', attrs: { href: src } }], text: it.name },
        { type: 'text', text: ' ' },
      ])
    }
  }
  return chain.run()
}
