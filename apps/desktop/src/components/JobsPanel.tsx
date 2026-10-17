import { useEffect, useRef, useState } from 'react'
import { Check, ChevronDown, ChevronUp, CircleAlert, Download, X } from 'lucide-react'
import { api, type Job, type JobState } from '../api'

interface Props {
  /** Open the note an import made. */
  onOpen: (noteId: string) => void
  /** A job finished: the note list and file previews need a refresh. */
  onFinished: (job: Job) => void
  onError: (e: string) => void
}

/** How long a finished import stays in the list. Failures stay until dismissed. */
const KEEP_DONE_MS = 15000

/**
 * Imports and tool downloads running in the background, with their
 * progress, in a corner of the window. The rest of the app stays usable.
 */
export function JobsPanel({ onOpen, onFinished, onError }: Props) {
  const [jobs, setJobs] = useState<Job[]>([])
  const [collapsed, setCollapsed] = useState(false)
  const states = useRef(new Map<number, JobState>())

  useEffect(() => {
    void api.jobs().then(setJobs).catch(() => {})
    const un = api.onJobsChanged(setJobs)
    return () => void un.then((f) => f())
  }, [])

  // Tell the app once per finished job, and let finished ones leave by themselves.
  useEffect(() => {
    for (const job of jobs) {
      const before = states.current.get(job.id)
      states.current.set(job.id, job.state)
      if (before !== 'running' || job.state === 'running') continue
      if (job.state === 'done') onFinished(job)
      if (job.state === 'done' || job.state === 'cancelled') {
        window.setTimeout(() => void api.dismissJob(job.id).catch(() => {}), KEEP_DONE_MS)
      }
    }
  }, [jobs, onFinished])

  if (jobs.length === 0) return null
  const running = jobs.filter((j) => j.state === 'running').length
  const failed = jobs.filter((j) => j.state === 'failed').length

  const retry = async (job: Job, withTools: boolean) => {
    try {
      if (job.kind === 'tools') await api.startInstallVideoTools()
      else if (job.kind === 'reimport' && job.sourceId) await api.startReimport(job.sourceId, withTools)
      else if (job.request) await api.startImport({ ...job.request, kind: job.request.kind ?? undefined, withTools })
      await api.dismissJob(job.id)
    } catch (e) {
      onError(String(e))
    }
  }

  return (
    <section className="jobs" aria-label="Imports">
      <button type="button" className="jobs__head" onClick={() => setCollapsed((c) => !c)} aria-expanded={!collapsed}>
        <Download size={14} />
        <span>{running > 0 ? `Importing ${running}…` : failed > 0 ? `${failed} import${failed === 1 ? '' : 's'} failed` : 'Imports finished'}</span>
        {collapsed ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
      </button>
      {!collapsed && (
        <ul className="jobs__list">
          {jobs.map((job) => (
            <JobRow key={job.id} job={job} onOpen={onOpen} onRetry={(withTools) => void retry(job, withTools)} />
          ))}
        </ul>
      )}
    </section>
  )
}

function JobRow({ job, onOpen, onRetry }: { job: Job; onOpen: (id: string) => void; onRetry: (withTools: boolean) => void }) {
  const pct = job.total ? Math.min(100, Math.round((job.done / job.total) * 100)) : null
  const dismiss = () => void api.dismissJob(job.id).catch(() => {})
  return (
    <li className={`jobs__item is-${job.state}`} data-testid="job">
      <div className="jobs__top">
        {job.state === 'done' ? <Check size={14} className="jobs__ok" /> : job.state === 'failed' ? <CircleAlert size={14} className="jobs__bad" /> : null}
        <span className="jobs__label" title={job.label}>
          {shortLabel(job.label)}
        </span>
        {job.state === 'running' ? (
          <button type="button" className="jobs__icon" onClick={() => void api.cancelJob(job.id)} title="Cancel" aria-label="Cancel">
            <X size={14} />
          </button>
        ) : (
          <button type="button" className="jobs__icon" onClick={dismiss} title="Remove from the list" aria-label="Remove from the list">
            <X size={14} />
          </button>
        )}
      </div>
      {job.state === 'running' && (
        <>
          <div className="jobs__stage">
            {job.stage}
            {pct !== null ? ` · ${pct}% of ${megabytes(job.total ?? 0)}` : job.done > 0 ? ` · ${megabytes(job.done)}` : ''}
          </div>
          <div className={`jobs__bar ${pct === null ? 'is-indeterminate' : ''}`} role="progressbar" aria-valuenow={pct ?? undefined} aria-valuemin={0} aria-valuemax={100}>
            <span style={pct === null ? undefined : { width: `${pct}%` }} />
          </div>
        </>
      )}
      {job.state === 'done' && (
        <div className="jobs__stage">
          {job.summary}
          {job.noteId && (
            <button type="button" className="jobs__link" onClick={() => onOpen(job.noteId!)}>
              Open
            </button>
          )}
        </div>
      )}
      {job.state === 'cancelled' && <div className="jobs__stage">Cancelled</div>}
      {job.state === 'failed' && (
        <>
          <div className="jobs__error" title={job.error ?? ''}>
            {job.error}
          </div>
          <div className="jobs__actions">
            <button type="button" className="jobs__link" onClick={() => onRetry(false)}>
              Retry
            </button>
            {job.source === 'video' && (
              <button type="button" className="jobs__link" onClick={() => onRetry(true)} title="A newer yt-dlp usually fixes errors after a site changes">
                Update yt-dlp and retry
              </button>
            )}
          </div>
        </>
      )}
    </li>
  )
}

/** A URL without its scheme and `www.`, cut to fit the panel. */
export function shortLabel(label: string): string {
  const s = label.replace(/^https?:\/\/(www\.)?/, '').replace(/\/$/, '')
  return s.length > 48 ? `${s.slice(0, 47)}…` : s
}

export function megabytes(bytes: number): string {
  const mb = bytes / 1_000_000
  return `${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB`
}
