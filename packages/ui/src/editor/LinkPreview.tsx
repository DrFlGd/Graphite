import type { Editor } from '@tiptap/core'
import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import type { BlockContext } from './blocks'
import { MarkdownView } from './MarkdownView'
import { anchorLabel, isNoteTarget, useSection } from './extensions/NoteEmbed'

const SHOW_DELAY_MS = 450
const HIDE_DELAY_MS = 250

interface Hover {
  target: string
  heading: string | null
  rect: DOMRect
}

/**
 * Hovering a `[[link]]` for a moment shows the note (or its heading/block)
 * in a small card; moving into the card keeps it open so it can be scrolled
 * and its links followed.
 */
export function LinkPreview({ editor, context, resolveSrc }: { editor: Editor; context: BlockContext; resolveSrc?: (src: string) => string }) {
  const [hover, setHover] = useState<Hover | null>(null)
  const timers = useRef<{ show: number | null; hide: number | null }>({ show: null, hide: null })

  useEffect(() => {
    if (!context.loadSection) return
    const dom = editor.view.dom
    const clear = (k: 'show' | 'hide') => {
      if (timers.current[k] !== null) window.clearTimeout(timers.current[k]!)
      timers.current[k] = null
    }
    const onOver = (e: MouseEvent) => {
      const el = (e.target as HTMLElement | null)?.closest?.('.gr-wiki-link:not(.gr-wiki-link--embed)') as HTMLElement | null
      if (!el) return
      const target = el.getAttribute('data-target') ?? ''
      if (!target || !isNoteTarget(target)) return
      clear('hide')
      clear('show')
      timers.current.show = window.setTimeout(() => setHover({ target, heading: el.getAttribute('data-heading'), rect: el.getBoundingClientRect() }), SHOW_DELAY_MS)
    }
    const onOut = (e: MouseEvent) => {
      const el = (e.target as HTMLElement | null)?.closest?.('.gr-wiki-link')
      if (!el) return
      clear('show')
      clear('hide')
      timers.current.hide = window.setTimeout(() => setHover(null), HIDE_DELAY_MS)
    }
    const onDown = () => {
      clear('show')
      setHover(null)
    }
    dom.addEventListener('mouseover', onOver)
    dom.addEventListener('mouseout', onOut)
    dom.addEventListener('mousedown', onDown)
    dom.addEventListener('keydown', onDown)
    return () => {
      dom.removeEventListener('mouseover', onOver)
      dom.removeEventListener('mouseout', onOut)
      dom.removeEventListener('mousedown', onDown)
      dom.removeEventListener('keydown', onDown)
      clear('show')
      clear('hide')
    }
  }, [editor, context])

  if (!hover) return null
  return createPortal(
    <PreviewCard
      hover={hover}
      context={context}
      resolveSrc={resolveSrc}
      onEnter={() => {
        if (timers.current.hide !== null) window.clearTimeout(timers.current.hide)
        timers.current.hide = null
      }}
      onLeave={() => {
        timers.current.hide = window.setTimeout(() => setHover(null), HIDE_DELAY_MS)
      }}
      onFollow={() => setHover(null)}
    />,
    document.body,
  )
}

function PreviewCard({ hover, context, resolveSrc, onEnter, onLeave, onFollow }: { hover: Hover; context: BlockContext; resolveSrc?: (src: string) => string; onEnter: () => void; onLeave: () => void; onFollow: () => void }) {
  const { section, state } = useSection(context, hover.target, hover.heading)
  const width = 420
  const below = hover.rect.bottom + 8
  const roomBelow = window.innerHeight - below
  const top = roomBelow > 220 ? below : Math.max(8, hover.rect.top - 8 - Math.min(360, hover.rect.top - 16))
  const left = Math.max(8, Math.min(hover.rect.left, window.innerWidth - width - 8))
  const follow = {
    ...context,
    followLink: (t: string, a: string | null) => {
      onFollow()
      context.followLink?.(t, a)
    },
  }
  return (
    <div className="gr-preview" style={{ top, left, width }} onMouseEnter={onEnter} onMouseLeave={onLeave} role="dialog" aria-label={`Preview of ${hover.target}`}>
      <button type="button" className="gr-preview__head" onClick={() => follow.followLink(hover.target, hover.heading)}>
        {section?.title ?? hover.target}
        {anchorLabel(hover.heading) ? <span className="gr-preview__anchor"> › {anchorLabel(hover.heading)}</span> : null}
      </button>
      <div className="gr-preview__body">
        {state === 'loading' && <p className="gr-embed__note">Loading…</p>}
        {state === 'missing' && <p className="gr-embed__note">No note called “{hover.target}” yet. Click the link to create it.</p>}
        {state === 'ready' && section?.markdown === null && <p className="gr-embed__note">{anchorLabel(hover.heading) ? `“${anchorLabel(hover.heading)}” is not in this note.` : 'That block is not in this note.'}</p>}
        {state === 'ready' && section?.markdown != null && (section.markdown.trim() ? <MarkdownView markdown={section.markdown} context={follow} resolveSrc={resolveSrc} title={section.title} /> : <p className="gr-embed__note">Empty note.</p>)}
      </div>
    </div>
  )
}
