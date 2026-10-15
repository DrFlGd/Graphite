import { useEffect, useRef, useState } from 'react'
import { api, SOURCE_KIND_LABEL, type ImportReport, type NoteSummary, type SourceKind, type Tools } from '../api'
import { SourceKindIcon } from './sources'

export interface ImportRequest {
  /** Prefilled URL (from the palette, a capture or the clipboard). */
  url?: string
  /** Project title to link the source to. */
  project?: string
  /** An inbox capture this import replaces: deleted once the import succeeds. */
  captureId?: string
}

interface Props {
  request: ImportRequest
  notes: NoteSummary[]
  onClose: () => void
  onDone: (report: ImportReport) => void
  onError: (e: string) => void
}

const KINDS: SourceKind[] = ['github', 'page', 'video', 'pdf', 'file', 'git']

const KIND_HINT: Record<SourceKind, string> = {
  github: 'Clones the repository and uses its README as the note.',
  git: 'Clones the repository and uses its README as the note.',
  page: 'Saves the page as one HTML file, its readable text as the note, and a screenshot.',
  video: 'Downloads the video and its thumbnail with yt-dlp.',
  pdf: 'Downloads the PDF; its text becomes searchable.',
  file: 'Downloads the file into the vault.',
}

/**
 * "Import from URL": paste a link, see which importer will handle it,
 * optionally pick another one, a project and a title, then fetch.
 */
export function ImportDialog({ request, notes, onClose, onDone, onError }: Props) {
  const [url, setUrl] = useState(request.url ?? '')
  const [guess, setGuess] = useState<SourceKind | null>(null)
  const [kind, setKind] = useState<SourceKind | ''>('')
  const [project, setProject] = useState(request.project ?? '')
  const [title, setTitle] = useState('')
  const [tools, setTools] = useState<Tools | null>(null)
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<string | null>(null)
  const input = useRef<HTMLInputElement>(null)

  useEffect(() => {
    input.current?.focus()
    void api.tools().then(setTools).catch(() => {})
  }, [])

  // Which importer the URL would get, as the user types.
  useEffect(() => {
    const u = url.trim()
    if (!u) {
      setGuess(null)
      return
    }
    let cancelled = false
    const t = window.setTimeout(() => {
      void api
        .classifyUrl(u)
        .then((k) => {
          if (!cancelled) setGuess(k)
        })
        .catch(() => {})
    }, 120)
    return () => {
      cancelled = true
      window.clearTimeout(t)
    }
  }, [url])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !busy) onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose, busy])

  const effective: SourceKind | null = kind || guess
  const valid = /^(https?:\/\/|git@)\S+$/.test(url.trim())
  const missingTool = effective === 'video' && tools && !tools.yt_dlp ? 'yt-dlp' : (effective === 'github' || effective === 'git') && tools && !tools.git ? 'git' : null
  const projects = notes.filter((n) => n.type === 'project')

  const go = async () => {
    if (!valid || busy) return
    setBusy(true)
    setFailure(null)
    try {
      const report = await api.importUrl(url.trim(), { kind: kind || undefined, project: project.trim() || undefined, title: title.trim() || undefined })
      if (request.captureId) {
        await api.deleteNote(request.captureId).catch(() => {})
      }
      onDone(report)
    } catch (e) {
      setFailure(String(e))
      onError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="modal" onMouseDown={busy ? undefined : onClose}>
      <div className="modal__card picker import" onMouseDown={(e) => e.stopPropagation()} role="dialog" aria-label="Import from URL">
        <form
          onSubmit={(e) => {
            e.preventDefault()
            void go()
          }}
        >
          <input ref={input} className="modal__input" value={url} onChange={(e) => setUrl(e.target.value)} placeholder="Paste a link: a GitHub repo, an article, a YouTube video, a PDF…" aria-label="URL" disabled={busy} spellCheck={false} />
          <div className="import__body">
            <div className="import__kinds" role="radiogroup" aria-label="Importer">
              {KINDS.map((k) => (
                <button
                  key={k}
                  type="button"
                  role="radio"
                  aria-checked={effective === k}
                  className={`chip ${effective === k ? 'is-active' : ''} ${kind === '' && guess === k ? 'is-guess' : ''}`}
                  onClick={() => setKind(kind === k ? '' : k)}
                  disabled={busy}
                  title={kind === '' && guess === k ? 'Detected from the URL' : KIND_HINT[k]}
                >
                  <SourceKindIcon kind={k} size={12} /> {SOURCE_KIND_LABEL[k]}
                </button>
              ))}
            </div>
            <p className="import__hint">
              {effective ? KIND_HINT[effective] : 'The importer is picked from the link; click one to override.'}
              {missingTool && <strong> {missingTool} is not installed, so this import will fail.</strong>}
            </p>
            <div className="import__fields">
              <label>
                Project
                <input list="import-projects" value={project} onChange={(e) => setProject(e.target.value)} placeholder="Link to a project (optional)" aria-label="Project" disabled={busy} />
                <datalist id="import-projects">
                  {projects.map((p) => (
                    <option key={p.id} value={p.title} />
                  ))}
                </datalist>
              </label>
              <label>
                Title
                <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Taken from the page, repo or document" aria-label="Title" disabled={busy} />
              </label>
            </div>
            {failure && <p className="import__failure">{failure}</p>}
          </div>
          <div className="picker__footer">
            <span className="qv__muted">{busy ? 'Fetching… this can take a while for a repository or a video.' : request.captureId ? 'The inbox capture is removed once the import succeeds.' : 'Files go under attachments/sources/, the note under notes/Sources/.'}</span>
            <button type="button" className="btn" onClick={onClose} disabled={busy}>
              Cancel
            </button>
            <button type="submit" className="btn btn--primary" disabled={!valid || busy}>
              {busy ? 'Importing…' : 'Import'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
