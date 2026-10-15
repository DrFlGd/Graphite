import { Node, mergeAttributes } from '@tiptap/core'
import { NodeViewWrapper, ReactNodeViewRenderer, type NodeViewProps } from '@tiptap/react'
import { Box, FileText, Music, Paperclip, Video } from 'lucide-react'
import type { BlockDef } from './types'
import { insertAttachments } from '../attachments'
import { embedKind, extOf } from '../fileKinds'

export interface FileEmbedOptions {
  /** Map a Markdown src (usually a vault-relative path) to something the webview can load. */
  resolveSrc?: (src: string) => string
}

function FileEmbedView({ node, extension, selected }: NodeViewProps) {
  const src = String(node.attrs.src ?? '')
  const name = String(node.attrs.name ?? '') || src.split('/').pop() || src
  const resolve = (extension.options as FileEmbedOptions).resolveSrc ?? ((s: string) => s)
  const kind = embedKind(src)
  const Icon = kind === 'video' ? Video : kind === 'audio' ? Music : kind === 'pdf' ? FileText : kind === 'model' ? Box : Paperclip
  return (
    <NodeViewWrapper className={`gr-file-embed gr-file-embed--${kind} ${selected ? 'is-selected' : ''}`} data-file-embed="" data-src={src} contentEditable={false}>
      {kind === 'video' && <video className="gr-file-embed__media" controls preload="metadata" src={resolve(src)} />}
      {kind === 'audio' && <audio className="gr-file-embed__media" controls preload="metadata" src={resolve(src)} />}
      <button type="button" className="gr-file-embed__card" title={`Open ${name}`} data-file-open="">
        <Icon size={16} />
        <span className="gr-file-embed__name">{name}</span>
        <span className="gr-file-embed__ext">{extOf(src).toUpperCase()}</span>
      </button>
    </NodeViewWrapper>
  )
}

/**
 * A non-image file embedded with `![name](path)`: video and audio play
 * inline, everything else is a card that opens the file's own view in the
 * host. The Markdown is the same image syntax, so other tools see a plain
 * file reference.
 */
export const FileEmbed = Node.create<FileEmbedOptions>({
  name: 'fileEmbed',
  group: 'block',
  atom: true,
  draggable: true,
  selectable: true,

  addOptions() {
    return { resolveSrc: (s: string) => s }
  },

  addAttributes() {
    return {
      src: { default: '' },
      name: { default: '' },
    }
  },

  parseHTML() {
    return [
      {
        tag: 'div[data-file-embed]',
        getAttrs: (el) => {
          const e = el as HTMLElement
          return { src: e.getAttribute('data-src') ?? '', name: e.getAttribute('data-name') ?? '' }
        },
      },
    ]
  },

  renderHTML({ node, HTMLAttributes }) {
    return ['div', mergeAttributes(HTMLAttributes, { 'data-file-embed': '', 'data-src': node.attrs.src, 'data-name': node.attrs.name, class: 'gr-file-embed' }), String(node.attrs.name ?? node.attrs.src)]
  },

  renderText({ node }) {
    return `![${node.attrs.name ?? ''}](${node.attrs.src ?? ''})`
  },

  addNodeView() {
    return ReactNodeViewRenderer(FileEmbedView, {
      // Let the media controls work; everything else goes to ProseMirror (selection, open-on-click).
      stopEvent: ({ event }) => !!(event.target as HTMLElement | null)?.closest?.('video, audio'),
    })
  },

  // Parsed by the image extension's `parseMarkdown` (see `image.ts`); only serialising lives here.
  renderMarkdown: (node) => `![${(node.attrs?.name as string) ?? ''}](${(node.attrs?.src as string) ?? ''})`,
})

export const file: BlockDef = {
  id: 'file',
  label: 'File',
  description: 'Attach a file: video, audio, PDF, 3D model or anything else',
  icon: Paperclip,
  group: 'media',
  keywords: ['attach', 'attachment', 'video', 'audio', 'pdf', 'model', '3d', 'stl', 'glb', 'upload'],
  extensions: [FileEmbed],
  insert: async (editor, ctx) => {
    const picked = ctx.pickFile ? await ctx.pickFile() : null
    if (picked) insertAttachments(editor, [picked])
  },
}
