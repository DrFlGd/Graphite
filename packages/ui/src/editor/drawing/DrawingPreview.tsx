import { useEffect, useRef, useState } from 'react'
import { DRAWING_SAVED, isDarkTheme, loadExcalidraw, sameSrc } from './excalidraw'

/**
 * A drawing as a picture, for embeds in notes. `read` returns the file's
 * JSON; the picture is redrawn whenever the drawing is saved.
 */
export function DrawingPreview({ src, read }: { src: string; read: (src: string) => Promise<string> }) {
  const box = useRef<HTMLDivElement>(null)
  const [state, setState] = useState<'loading' | 'ready' | 'empty' | 'broken'>('loading')
  const [version, setVersion] = useState(0)

  useEffect(() => {
    const onSaved = (e: Event) => {
      if (sameSrc((e as CustomEvent<string>).detail, src)) setVersion((v) => v + 1)
    }
    window.addEventListener(DRAWING_SAVED, onSaved)
    return () => window.removeEventListener(DRAWING_SAVED, onSaved)
  }, [src])

  useEffect(() => {
    let live = true
    ;(async () => {
      try {
        const [json, ex] = await Promise.all([read(src), loadExcalidraw()])
        const data = JSON.parse(json)
        const elements = ex.getNonDeletedElements(data.elements ?? [])
        if (!live) return
        if (elements.length === 0) {
          setState('empty')
          return
        }
        const svg = await ex.exportToSvg({
          elements,
          appState: { ...data.appState, exportBackground: false, exportWithDarkMode: isDarkTheme(), exportPadding: 12 },
          files: data.files ?? {},
        })
        if (!live || !box.current) return
        svg.classList.add('gr-drawing__svg')
        box.current.replaceChildren(svg)
        setState('ready')
      } catch {
        if (live) setState('broken')
      }
    })()
    return () => {
      live = false
    }
  }, [src, read, version])

  return (
    <div className={`gr-drawing gr-drawing--${state}`}>
      <div ref={box} className="gr-drawing__picture" />
      {state === 'loading' && <p className="gr-drawing__note">Loading drawing…</p>}
      {state === 'empty' && <p className="gr-drawing__note">Empty drawing. Open it to draw.</p>}
      {state === 'broken' && <p className="gr-drawing__note">This drawing could not be read.</p>}
    </div>
  )
}
