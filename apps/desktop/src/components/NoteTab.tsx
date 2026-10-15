import { useCallback, useEffect, useRef, useState } from 'react'
import { insertAttachments, type AttachmentItem, type Editor } from '@graphite/ui'
import { api, fileToBase64, type Note } from '../api'
import { NotePane } from './NotePane'

const SAVE_DELAY_MS = 400

interface Props {
  noteId: string
  /** Whether this tab is the one the user is looking at in the active pane. */
  active: boolean
  /** Ids of notes that changed on disk (from the watcher); reload if ours is among them and we are clean. */
  changed: { ids: string[]; seq: number }
  /** A note another tab just saved; adopt it if it is ours and we are clean. */
  saved: Note | null
  /** Front-matter edits from the context panel, applied through the normal save path. */
  patch: { id: string; patch: Partial<Note>; seq: number } | null
  onActiveNote: (note: Note | null) => void
  onEditorReady: (editor: Editor | null) => void
  onSaved: (note: Note) => void
  onFollowLink: (target: string) => void
  onOpenFile: (src: string) => void
  onOpenUrl: (href: string) => void
  onDelete: (note: Note) => void
  onError: (e: string) => void
  resolveSrc?: (src: string) => string
  pickImage: () => Promise<string | null>
  pickFile: () => Promise<{ src: string; name: string } | null>
}

/**
 * One open note: loads it, autosaves edits (debounced, flushed on close),
 * reloads when the file changes underneath it, and attaches pasted files.
 */
export function NoteTab({ noteId, active, changed, saved, patch, onActiveNote, onEditorReady, onSaved, onFollowLink, onOpenFile, onOpenUrl, onDelete, onError, resolveSrc, pickImage, pickFile }: Props) {
  const [note, setNote] = useState<Note | null>(null)
  const [saveState, setSaveState] = useState<'saved' | 'dirty' | 'saving'>('saved')
  const pending = useRef<{ timer: number | null; note: Note | null }>({ timer: null, note: null })
  const editor = useRef<Editor | null>(null)
  const callbacks = useRef({ onActiveNote, onEditorReady, onSaved, onError })
  callbacks.current = { onActiveNote, onEditorReady, onSaved, onError }

  useEffect(() => {
    let cancelled = false
    api
      .readNote(noteId)
      .then((n) => {
        if (!cancelled) setNote(n)
      })
      .catch((e) => callbacks.current.onError(String(e)))
    return () => {
      cancelled = true
    }
  }, [noteId])

  const flush = useCallback(async () => {
    const p = pending.current
    if (p.timer) {
      window.clearTimeout(p.timer)
      p.timer = null
    }
    const n = p.note
    p.note = null
    if (!n) return
    setSaveState('saving')
    try {
      const out = await api.saveNote(n)
      setNote((c) => (c && c.id === out.id && !pending.current.note ? { ...c, updated: out.updated, created: out.created, path: out.path } : c))
      setSaveState(pending.current.note ? 'dirty' : 'saved')
      callbacks.current.onSaved(out)
    } catch (e) {
      callbacks.current.onError(String(e))
      setSaveState('dirty')
    }
  }, [])

  // Save whatever is pending when the tab closes.
  useEffect(() => {
    return () => {
      const p = pending.current
      if (p.timer) window.clearTimeout(p.timer)
      if (p.note) void api.saveNote(p.note).catch(() => {})
      p.note = null
    }
  }, [])

  const change = (n: Note) => {
    setNote(n)
    pending.current.note = n
    setSaveState('dirty')
    if (pending.current.timer) window.clearTimeout(pending.current.timer)
    pending.current.timer = window.setTimeout(() => void flush(), SAVE_DELAY_MS)
  }

  // External change on disk: reload unless we have unsaved edits.
  useEffect(() => {
    if (!changed.ids.includes(noteId) || pending.current.note) return
    void api
      .readNote(noteId)
      .then(setNote)
      .catch(() => {})
  }, [changed, noteId])

  // The same note saved from another tab or pane.
  useEffect(() => {
    if (saved && saved.id === noteId && !pending.current.note) setNote((c) => (c && c.updated !== saved.updated ? saved : c))
  }, [saved, noteId])

  // Property edits from the context panel.
  const appliedPatch = useRef(0)
  useEffect(() => {
    if (!patch || patch.id !== noteId || patch.seq === appliedPatch.current) return
    appliedPatch.current = patch.seq
    setNote((c) => {
      if (!c) return c
      const n = { ...c, ...patch.patch }
      pending.current.note = n
      setSaveState('dirty')
      if (pending.current.timer) window.clearTimeout(pending.current.timer)
      pending.current.timer = window.setTimeout(() => void flush(), SAVE_DELAY_MS)
      return n
    })
  }, [patch, noteId, flush])

  // Tell the app what is on screen.
  useEffect(() => {
    if (active) callbacks.current.onActiveNote(note)
  }, [active, note])
  useEffect(() => {
    if (active) callbacks.current.onEditorReady(editor.current)
  }, [active, note?.id])

  // Ctrl/Cmd+S saves now.
  useEffect(() => {
    if (!active) return
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === 's') {
        e.preventDefault()
        void flush()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [active, flush])

  const onFiles = async (files: File[], pos?: number) => {
    const items: AttachmentItem[] = []
    for (const f of files) {
      const ext = f.type.split('/')[1] || 'bin'
      const name = !f.name || f.name === 'image.png' ? `pasted-${new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19)}.${ext}` : f.name
      try {
        const src = await api.attachData(name, await fileToBase64(f))
        items.push({ src, name })
      } catch (err) {
        onError(String(err))
      }
    }
    if (editor.current) insertAttachments(editor.current, items, pos)
  }

  if (!note) return <div className="app__empty">Loading…</div>
  return (
    <NotePane
      note={note}
      saveState={saveState}
      onChange={change}
      onFollowLink={onFollowLink}
      onOpenFile={onOpenFile}
      onOpenUrl={onOpenUrl}
      onDelete={() => onDelete(note)}
      resolveSrc={resolveSrc}
      pickImage={pickImage}
      pickFile={pickFile}
      onEditorReady={(ed) => {
        editor.current = ed
        if (active) onEditorReady(ed)
      }}
      onFiles={(files, pos) => void onFiles(files, pos)}
      onReplace={(n) => {
        pending.current.note = null
        setNote(n)
        setSaveState('saved')
        onSaved(n)
      }}
      onError={onError}
    />
  )
}
