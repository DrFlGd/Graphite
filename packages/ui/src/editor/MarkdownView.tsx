import { useState } from 'react'
import { NoteEditor } from './Editor'
import type { BlockContext } from './blocks'

let seq = 0

/**
 * Markdown shown read-only with the full editor rendering (callouts, maths,
 * diagrams, tasks…), for embeds and link previews. Links inside follow
 * through the context; embeds inside go one level deeper.
 */
export function MarkdownView({ markdown, context, resolveSrc, title }: { markdown: string; context: BlockContext; resolveSrc?: (src: string) => string; title?: string }) {
  const [key] = useState(() => `view-${(seq = (seq + 1) % 1_000_000)}`)
  return (
    <div className="gr-markdown-view">
      <NoteEditor
        docKey={key}
        markdown={markdown}
        onChange={() => {}}
        editable={false}
        toolbar={false}
        title={title}
        resolveSrc={resolveSrc}
        blockContext={{ ...context, embedDepth: (context.embedDepth ?? 0) + 1 }}
        onFollowLink={(target, heading) => context.followLink?.(target, heading)}
      />
    </div>
  )
}
