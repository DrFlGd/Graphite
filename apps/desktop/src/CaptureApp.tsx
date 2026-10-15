import { useCallback, useEffect, useRef, useState, type ClipboardEvent } from 'react'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { Paperclip, X } from 'lucide-react'
import { api, fileToBase64 } from './api'

interface Attachment {
  rel: string
  name: string
}

/**
 * The quick-capture window (global shortcut). One box, no questions:
 * Enter saves to the inbox, Shift+Enter is a newline, Esc hides. Files can be
 * dropped or pasted and are copied into `attachments/Inbox/`.
 */
export function CaptureApp() {
  const [text, setText] = useState('')
  const [files, setFiles] = useState<Attachment[]>([])
  const [status, setStatus] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const box = useRef<HTMLTextAreaElement>(null)
  const lastSaved = useRef<string | null>(null)

  const focus = () => window.setTimeout(() => box.current?.focus(), 30)

  useEffect(() => {
    focus()
    let un1: (() => void) | undefined
    let un2: (() => void) | undefined
    void api.onCaptureShown(() => {
      setStatus(null)
      focus()
    }).then((u) => (un1 = u))
    void getCurrentWebview()
      .onDragDropEvent(async (e) => {
        if (e.payload.type !== 'drop') return
        for (const p of e.payload.paths) {
          try {
            const rel = await api.attachFile(p, 'attachments/Inbox')
            setFiles((f) => [...f, { rel, name: p.split(/[\\/]/).pop() ?? rel }])
          } catch (err) {
            setStatus(String(err))
          }
        }
      })
      .then((u) => (un2 = u))
    return () => {
      un1?.()
      un2?.()
    }
  }, [])

  const reset = () => {
    setText('')
    setFiles([])
  }

  const hide = useCallback(async () => {
    setStatus(null)
    await api.hideCaptureWindow()
  }, [])

  const save = useCallback(async () => {
    if (busy) return
    if (!text.trim() && files.length === 0) return
    setBusy(true)
    try {
      const note = await api.capture(
        text,
        files.map((f) => f.rel),
      )
      lastSaved.current = note.id
      reset()
      setStatus(`Saved to inbox: ${note.title}`)
      window.setTimeout(() => void hide(), 500)
    } catch (e) {
      setStatus(String(e))
    } finally {
      setBusy(false)
    }
  }, [busy, text, files, hide])

  const onPaste = async (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const pasted = Array.from(e.clipboardData.files)
    if (pasted.length === 0) return
    e.preventDefault()
    for (const f of pasted) {
      try {
        const rel = await api.attachData(f.name || `pasted-${Date.now()}.png`, await fileToBase64(f), 'attachments/Inbox')
        setFiles((list) => [...list, { rel, name: f.name || rel }])
      } catch (err) {
        setStatus(String(err))
      }
    }
  }

  return (
    <div className="capture" onKeyDown={(e) => e.key === 'Escape' && void hide()}>
      <textarea
        ref={box}
        className="capture__box"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onPaste={onPaste}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault()
            void save()
          }
        }}
        placeholder="Capture a thought, a link, or drop files… Enter saves, Shift+Enter for a new line, Esc closes"
        aria-label="Quick capture"
        spellCheck
      />
      {files.length > 0 && (
        <ul className="capture__files">
          {files.map((f) => (
            <li key={f.rel}>
              <Paperclip size={12} /> {f.name}
              <button type="button" className="icon-btn" aria-label={`Remove ${f.name}`} onClick={() => setFiles((l) => l.filter((x) => x.rel !== f.rel))}>
                <X size={12} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="capture__bar">
        <span className="capture__status">{status ?? 'Saves to notes/Inbox'}</span>
        {lastSaved.current && status?.startsWith('Saved') && (
          <button type="button" className="btn" onClick={() => void api.openInMain(lastSaved.current!)}>
            Open
          </button>
        )}
        <button type="button" className="btn btn--primary" disabled={busy || (!text.trim() && files.length === 0)} onClick={() => void save()}>
          Save
        </button>
      </div>
    </div>
  )
}
