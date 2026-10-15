import type { Editor } from '@tiptap/core'

/** A file that has already been copied into the vault. */
export interface AttachmentItem {
  /** Vault-relative path, exactly as it should appear in the Markdown. */
  src: string
  /** Shown as the image alt text or the link text. */
  name: string
}

const IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'avif']

export const isImageSrc = (src: string): boolean => IMAGE_EXTS.includes(src.split('.').pop()?.toLowerCase() ?? '')

/**
 * Insert dropped or pasted files into the note: images become image blocks
 * (`![name](path)`), anything else a link (`[name](path)`). `pos` is where to
 * insert (e.g. the drop point); by default the current selection.
 */
export function insertAttachments(editor: Editor, items: AttachmentItem[], pos?: number): boolean {
  if (items.length === 0 || editor.isDestroyed) return false
  // Insert after whatever is selected (e.g. a just-inserted image) rather than replacing it.
  let chain = editor.chain().focus(pos ?? editor.state.selection.to)
  for (const it of items) {
    if (isImageSrc(it.src)) {
      chain = chain.setImage({ src: it.src, alt: it.name })
    } else {
      chain = chain.insertContent([
        { type: 'text', marks: [{ type: 'link', attrs: { href: it.src } }], text: it.name },
        { type: 'text', text: ' ' },
      ])
    }
  }
  return chain.run()
}
