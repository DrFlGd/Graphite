import { Image as ImageIcon } from 'lucide-react'
import Image, { type ImageOptions as BaseImageOptions } from '@tiptap/extension-image'
import type { BlockDef } from './types'

export interface ImageOptions extends BaseImageOptions {
  /** Map a Markdown src (usually a vault-relative path) to something the webview can load. */
  resolveSrc?: (src: string) => string
}

/**
 * Image node that keeps the Markdown src (e.g. `attachments/x.png`) in the
 * document and only resolves it to a loadable URL when rendering.
 */
export const VaultImage = Image.extend<ImageOptions>({
  addOptions() {
    return { ...(this.parent?.() as BaseImageOptions), resolveSrc: (s: string) => s }
  },
  renderHTML({ HTMLAttributes }) {
    const src = typeof HTMLAttributes.src === 'string' ? HTMLAttributes.src : ''
    return ['img', { ...this.options.HTMLAttributes, ...HTMLAttributes, src: this.options.resolveSrc?.(src) ?? src, 'data-src': src }]
  },
  parseHTML() {
    return [
      {
        tag: 'img[src]',
        getAttrs: (el) => {
          const e = el as HTMLElement
          return { src: e.getAttribute('data-src') ?? e.getAttribute('src'), alt: e.getAttribute('alt'), title: e.getAttribute('title') }
        },
      },
    ]
  },
})

export const image: BlockDef = {
  id: 'image',
  label: 'Image',
  description: 'Picture from the vault or a file',
  icon: ImageIcon,
  group: 'media',
  keywords: ['picture', 'photo', 'png', 'jpg', 'figure'],
  insert: async (editor, ctx) => {
    const src = ctx.pickImage ? await ctx.pickImage() : null
    if (src) editor.chain().focus().setImage({ src }).run()
  },
}
