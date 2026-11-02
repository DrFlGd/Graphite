import { useEffect, useRef, useState, type ComponentType } from 'react'
import { announceDrawingSaved, isDarkTheme, loadExcalidraw } from './excalidraw'

const SAVE_DELAY_MS = 700

type ExcalidrawModule = Awaited<ReturnType<typeof loadExcalidraw>>

/** What changes when the drawing does: its elements' versions and its images. */
const sceneKey = (version: number, files: Record<string, unknown> | null | undefined) => `${version}:${Object.keys(files ?? {}).length}`

export interface DrawingEditorProps {
  /** The drawing's vault path, announced to its embeds after each save. */
  src: string
  /** The file's JSON. */
  load: () => Promise<string>
  /** Write the file's JSON. */
  save: (json: string) => Promise<void>
  onError?: (message: string) => void
  /** Called with 'saving' / 'saved' as edits are written. */
  onSaveState?: (state: 'saved' | 'saving' | 'dirty') => void
}

/**
 * The Excalidraw editor on one `.excalidraw` file. Edits save on their own
 * shortly after the drawing changes (moving the view or selecting does not
 * count), and once more when the editor closes.
 */
export function DrawingEditor({ src, load, save, onError, onSaveState }: DrawingEditorProps) {
  const [mod, setMod] = useState<ExcalidrawModule | null>(null)
  const [initial, setInitial] = useState<Record<string, unknown> | null>(null)
  const [failed, setFailed] = useState<string | null>(null)
  const pending = useRef<{ timer: number | null; json: string | null; version: string }>({ timer: null, json: null, version: '' })
  const cb = useRef({ save, onError, onSaveState })
  cb.current = { save, onError, onSaveState }

  useEffect(() => {
    let live = true
    Promise.all([loadExcalidraw(), load()])
      .then(([m, json]) => {
        if (!live) return
        const data = JSON.parse(json)
        const restored = m.restore(data, null, null)
        pending.current.version = sceneKey(m.getSceneVersion(restored.elements), restored.files)
        setMod(m)
        setInitial({ elements: restored.elements, appState: { ...restored.appState, theme: isDarkTheme() ? 'dark' : 'light' }, files: restored.files, scrollToContent: true })
      })
      .catch((e) => live && setFailed(String(e)))
    return () => {
      live = false
    }
    // The file is loaded once per drawing; later saves come from this editor.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [src])

  const flush = async () => {
    const p = pending.current
    if (p.timer !== null) window.clearTimeout(p.timer)
    p.timer = null
    const json = p.json
    p.json = null
    if (json === null) return
    cb.current.onSaveState?.('saving')
    try {
      await cb.current.save(json)
      announceDrawingSaved(src)
      cb.current.onSaveState?.(pending.current.json ? 'dirty' : 'saved')
    } catch (e) {
      cb.current.onError?.(String(e))
      cb.current.onSaveState?.('dirty')
    }
  }

  // Save what is pending when the drawing closes.
  useEffect(
    () => () => {
      void flush()
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [src],
  )

  if (failed) return <p className="gr-drawing-editor__error">This drawing could not be opened: {failed}</p>
  if (!mod || !initial) return <p className="gr-drawing-editor__loading">Loading the drawing editor…</p>
  const Excalidraw = mod.Excalidraw as unknown as ComponentType<Record<string, unknown>>
  return (
    <div className="gr-drawing-editor">
      <Excalidraw
        initialData={initial}
        theme={isDarkTheme() ? 'dark' : 'light'}
        UIOptions={{ canvasActions: { loadScene: false, saveToActiveFile: false, toggleTheme: false } }}
        onChange={(elements: readonly { version: number }[], appState: Record<string, unknown>, files: Record<string, unknown>) => {
          const version = mod.getSceneVersion(elements as never)
          const p = pending.current
          // Only real edits (elements or their images) count, not panning or selecting.
          const key = sceneKey(version, files)
          if (key === p.version) return
          p.version = key
          p.json = mod.serializeAsJSON(elements as never, appState as never, files as never, 'local')
          cb.current.onSaveState?.('dirty')
          if (p.timer !== null) window.clearTimeout(p.timer)
          p.timer = window.setTimeout(() => void flush(), SAVE_DELAY_MS)
        }}
      />
    </div>
  )
}

export default DrawingEditor
