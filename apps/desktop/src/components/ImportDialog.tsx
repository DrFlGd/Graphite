import { useEffect, useRef, useState } from 'react'
import { api, SOURCE_KIND_LABEL, type NoteSummary, type SourceKind, type Tools } from '../api'
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
  /** The import is running in the background. */
  onStarted: () => void
}

const KINDS: SourceKind[] = ['github', 'page', 'video', 'pdf', 'file', 'git']

const KIND_HINT: Record<SourceKind, string> = {
  github: 'Downloads the repository and uses its README as the note.',
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
export function ImportDialog({ request, notes, onClose, onStarted }: Props) {
  const [url, setUrl] = useState(request.url ?? '')
  const [guess, setGuess] = useState<SourceKind | null>(null)
  const [kind, setKind] = useState<SourceKind | ''>('')
  const [project, setProject] = useState(request.project ?? '')
  const [title, setTitle] = useState('')
  const [tools, setTools] = useState<Tools | null>(null)
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<string | null>(null)
  const [videoMissing, setVideoMissing] = useState<string[]>([])
  const input = useRef<HTMLInputElement>(null)

  useEffect(() => {
    input.current?.focus()
    void api.tools().then(setTools).catch(() => {})
    void api.missingVideoTools().then(setVideoMissing).catch(() => {})
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
  const missingTool = tools ? missingToolFor(effective, url.trim(), tools) : null
  const projects = notes.filter((n) => n.type === 'project')

  // A video import first downloads whatever video tools are missing.
  const needsTools = effective === 'video' && videoMissing.length > 0

  // The import runs in the background; only a request that cannot start
  // (not a URL) keeps the window open with the reason.
  const go = async () => {
    if (!valid || busy) return
    setBusy(true)
    setFailure(null)
    try {
      await api.startImport({ url: url.trim(), kind: kind || undefined, project: project.trim() || undefined, title: title.trim() || undefined, withTools: needsTools, captureId: request.captureId })
      onStarted()
    } catch (e) {
      setFailure(String(e))
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
              {missingTool === 'git' && <strong> git is not installed. It is needed for repositories outside GitHub.</strong>}
              {needsTools && <strong> {videoToolsNote(videoMissing)}</strong>}
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
            <span className="qv__muted">{request.captureId ? 'The inbox capture is removed once the import succeeds.' : 'Runs in the background; files go under attachments/sources/.'}</span>
            <button type="button" className="btn" onClick={onClose} disabled={busy}>
              Cancel
            </button>
            <button type="submit" className="btn btn--primary" disabled={!valid || busy}>
              Import
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

const VIDEO_TOOLS: Record<string, { name: string; mb: number }> = {
  'yt-dlp': { name: 'yt-dlp', mb: 40 },
  deno: { name: 'Deno', mb: 45 },
  ffmpeg: { name: 'ffmpeg', mb: 200 },
}

/** What a video import downloads first, in a line. */
export function videoToolsNote(missing: string[]): string {
  const tools = missing.map((m) => VIDEO_TOOLS[m]).filter(Boolean)
  if (tools.length === 0) return ''
  const names = tools.map((t) => t.name)
  const list = names.length === 1 ? names[0] : `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`
  const mb = tools.reduce((sum, t) => sum + t.mb, 0)
  return `Video import needs ${list}; the import downloads ${tools.length === 1 ? 'it' : 'them'} first (about ${mb} MB).`
}

/** The program an import of this kind and URL needs but cannot find, if any. */
export function missingToolFor(kind: SourceKind | null, url: string, tools: Tools): 'git' | 'yt-dlp' | null {
  if (kind === 'video' && !tools.yt_dlp) return 'yt-dlp'
  if (tools.git) return null
  if (kind === 'git') return 'git'
  // GitHub repositories download without git; other hosts still need it.
  if (kind === 'github' && !isGithubUrl(url)) return 'git'
  return null
}

function isGithubUrl(url: string): boolean {
  try {
    return new URL(url).hostname.replace(/^www\./, '') === 'github.com'
  } catch {
    return false
  }
}
