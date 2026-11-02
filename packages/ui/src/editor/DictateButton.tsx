import type { Editor } from '@tiptap/core'
import { Loader2, Mic, Square } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { spacedTranscript, startRecording, type DictationHost, type Recording } from './dictation'

type State = 'idle' | 'starting' | 'recording' | 'writing'

const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`

/**
 * Dictate (B11): press to record, press again to stop, and the words are
 * written in at the cursor. Escape while recording drops the recording.
 */
export function DictateButton({ editor, host }: { editor: Editor; host: DictationHost }) {
  const [state, setState] = useState<State>('idle')
  const [seconds, setSeconds] = useState(0)
  const [level, setLevel] = useState(0)
  const rec = useRef<Recording | null>(null)

  const fail = (e: unknown) => {
    const msg = e instanceof Error ? e.message : String(e)
    host.onError?.(/denied|not allowed|permission/i.test(msg) ? 'Graphite was not allowed to use the microphone.' : msg)
  }

  const start = async () => {
    setState('starting')
    try {
      if (!(await host.prepare())) {
        setState('idle')
        return
      }
      rec.current = await startRecording(setLevel)
      setSeconds(0)
      setState('recording')
    } catch (e) {
      rec.current = null
      setState('idle')
      fail(e)
    }
  }

  const stop = async () => {
    const r = rec.current
    rec.current = null
    if (!r) return
    setState('writing')
    try {
      const text = await host.transcribe(await r.stop())
      const { from } = editor.state.selection
      const before = editor.state.doc.textBetween(Math.max(0, from - 1), from, '\n', '\n')
      const insert = spacedTranscript(before, text)
      if (insert) editor.chain().focus().insertContent({ type: 'text', text: insert }).run()
      else host.onError?.('No words were heard.')
    } catch (e) {
      fail(e)
    } finally {
      setState('idle')
    }
  }

  useEffect(() => {
    if (state !== 'recording') return
    const timer = window.setInterval(() => setSeconds((s) => s + 1), 1000)
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      rec.current?.cancel()
      rec.current = null
      setState('idle')
    }
    window.addEventListener('keydown', onKey, true)
    return () => {
      window.clearInterval(timer)
      window.removeEventListener('keydown', onKey, true)
    }
  }, [state])

  // Let go of the microphone if the note closes mid-recording.
  useEffect(() => () => rec.current?.cancel(), [])

  const recording = state === 'recording'
  const label = recording ? 'Stop and write it in' : state === 'writing' ? 'Writing what you said…' : 'Dictate'
  return (
    <button
      type="button"
      className={`gr-tool gr-dictate gr-dictate--${state}`}
      title={recording ? `${label} (${clock(seconds)} so far; Esc drops it)` : label}
      aria-label={label}
      aria-pressed={recording}
      disabled={state === 'starting' || state === 'writing'}
      onMouseDown={(e) => e.preventDefault()}
      // The ring around the button follows the microphone level.
      style={recording ? { boxShadow: `0 0 0 ${1 + Math.round(level * 5)}px rgba(214, 69, 69, 0.35)` } : undefined}
      onClick={() => void (recording ? stop() : start())}
    >
      {state === 'writing' || state === 'starting' ? <Loader2 size={16} className="gr-spin" /> : recording ? <Square size={13} fill="currentColor" /> : <Mic size={16} />}
    </button>
  )
}
