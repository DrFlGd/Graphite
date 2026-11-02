import { useCallback, useState } from 'react'
import { Copy, FolderOpen } from 'lucide-react'
import { DrawingEditor, encodeSrc } from '@graphite/ui'
import { api } from '../api'

/** One `.excalidraw` file in the drawing editor (E11), saving as you draw. */
export function DrawingView({ path, onError }: { path: string; onError: (e: string) => void }) {
  const [state, setState] = useState<'saved' | 'saving' | 'dirty'>('saved')
  const name = (path.split('/').pop() ?? path).replace(/\.excalidraw$/i, '')
  const load = useCallback(() => api.readDrawing(path), [path])
  const save = useCallback((json: string) => api.writeDrawing(path, json), [path])
  return (
    <div className="drawing">
      <header className="drawing__header">
        <h2 title={path}>{name}</h2>
        <span className="drawing__path">{path}</span>
        <span className={`note__save note__save--${state}`}>{state === 'saved' ? 'Saved' : state === 'saving' ? 'Saving…' : 'Unsaved'}</span>
        <button type="button" className="btn" onClick={() => void navigator.clipboard?.writeText(`![${name}](${encodeSrc(path)})`)} title="Copy an embed to paste into a note">
          <Copy size={14} /> Copy embed
        </button>
        <button type="button" className="btn" onClick={() => void api.revealAsset(path).catch((e) => onError(String(e)))} title="Show in the file manager">
          <FolderOpen size={14} /> Reveal
        </button>
      </header>
      <div className="drawing__canvas">
        <DrawingEditor src={path} load={load} save={save} onError={onError} onSaveState={setState} />
      </div>
    </div>
  )
}
