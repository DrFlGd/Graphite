import { NodeViewWrapper, type NodeViewProps } from '@tiptap/react'
import { FileText } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import type { BlockContext, SectionPreview } from '../blocks'
import { MarkdownView } from '../MarkdownView'
import type { WikiLinkAttrs } from './WikiLink'

/** Whether a link target names a note (no file extension, or `.md`) rather than a file. */
export function isNoteTarget(target: string): boolean {
  const name = target.split('/').pop() ?? target
  return !/\.[a-z0-9]{1,8}$/i.test(name) || /\.md$/i.test(name)
}

export function anchorLabel(heading: string | null): string {
  if (!heading) return ''
  return heading.startsWith('^') ? '' : heading.split('#').filter(Boolean).pop() ?? heading
}

/** Load a section and keep it current while `target`/`anchor` stay the same. */
export function useSection(context: BlockContext, target: string, anchor: string | null): { section: SectionPreview | null; state: 'loading' | 'ready' | 'missing' } {
  const [out, setOut] = useState<{ section: SectionPreview | null; state: 'loading' | 'ready' | 'missing' }>({ section: null, state: 'loading' })
  // Tiptap hands node views a fresh copy of their options on every read, so the context is not a stable dependency.
  const ctx = useRef(context)
  ctx.current = context
  useEffect(() => {
    let cancelled = false
    setOut({ section: null, state: 'loading' })
    const load = ctx.current.loadSection
    if (!load) {
      setOut({ section: null, state: 'missing' })
      return
    }
    load(target, anchor)
      .then((s) => {
        if (!cancelled) setOut({ section: s, state: s ? 'ready' : 'missing' })
      })
      .catch(() => {
        if (!cancelled) setOut({ section: null, state: 'missing' })
      })
    return () => {
      cancelled = true
    }
  }, [target, anchor])
  return out
}

/**
 * `![[Note]]`, `![[Note#Heading]]` or `![[Note#^id]]`: the note or its part
 * shown in place, read-only, with a header that opens the source.
 */
export function NoteEmbedView({ node, extension, selected }: NodeViewProps) {
  const attrs = node.attrs as WikiLinkAttrs
  const options = extension.options as { context?: BlockContext; resolveSrc?: (src: string) => string }
  const context = options.context ?? {}
  const { section, state } = useSection(context, attrs.target, attrs.heading)
  const part = anchorLabel(attrs.heading)
  const label = attrs.display ?? (part ? `${attrs.target} › ${part}` : attrs.target)
  return (
    <NodeViewWrapper as="span" className={`gr-embed ${selected ? 'is-selected' : ''}`} data-wiki-link="" data-embed="" data-target={attrs.target} contentEditable={false}>
      <button type="button" className="gr-embed__head" title={`Open ${attrs.target}`} onClick={() => context.followLink?.(attrs.target, attrs.heading)}>
        <FileText size={14} aria-hidden />
        <span className="gr-embed__title">{section?.title && !attrs.display ? (part ? `${section.title} › ${part}` : section.title) : label}</span>
      </button>
      <span className="gr-embed__body">
        {state === 'loading' && <span className="gr-embed__note">Loading…</span>}
        {state === 'missing' && <span className="gr-embed__note">No note called “{attrs.target}” yet.</span>}
        {state === 'ready' && section && section.markdown === null && <span className="gr-embed__note">{part ? `“${part}” is not in this note.` : 'That block is not in this note.'}</span>}
        {state === 'ready' && section && section.markdown !== null && (section.markdown.trim() ? <MarkdownView markdown={section.markdown} context={context} resolveSrc={options.resolveSrc} /> : <span className="gr-embed__note">Empty.</span>)}
      </span>
    </NodeViewWrapper>
  )
}
