import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from 'react'
import { Copy, ExternalLink, FilePlus2, FolderOpen, RefreshCw } from 'lucide-react'
import { encodeSrc } from '@graphite/ui'
import { api, assetUrl, type Asset, type Backlink, type Tools } from '../api'
const ModelViewer = lazy(() => import('./ModelViewer'))
/** Formats the three.js viewer can load. */
export const VIEWER_EXTS = ['glb', 'gltf', 'obj', 'stl', 'ply', '3mf']

interface Props {
  path: string
  vaultRoot: string
  /** Page to show first, for `file.pdf#page=3` links. */
  page?: number
  /** Bumped when the file or its previews changed. */
  version: number
  onOpenNote: (id: string, newTab?: boolean) => void
  /** A `type: asset` note was created for this file. */
  onNoteCreated: (id: string) => void
  onError: (e: string) => void
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`
}

export function formatDuration(s: number): string {
  const m = Math.floor(s / 60)
  const sec = Math.round(s % 60)
  const h = Math.floor(m / 60)
  return h > 0 ? `${h}:${String(m % 60).padStart(2, '0')}:${String(sec).padStart(2, '0')}` : `${m}:${String(sec).padStart(2, '0')}`
}

const TEXT_EXTS = ['csv', 'tsv', 'json', 'jsonl', 'yaml', 'yml', 'xml', 'txt', 'tex', 'org', 'rst', 'adoc']

/**
 * One file as a first-class node: the preview that fits its kind (image,
 * video, audio, PDF pages, 3D model, code, CSV table), its metadata, and
 * the notes that reference it. Files are never modified from here; Open
 * and Reveal hand them to the OS.
 */
export function AssetView({ path, vaultRoot, page, version, onOpenNote, onNoteCreated, onError }: Props) {
  const [asset, setAsset] = useState<Asset | null>(null)
  const [refs, setRefs] = useState<Backlink[]>([])
  const [text, setText] = useState<string | null>(null)
  const [tools, setTools] = useState<Tools | null>(null)
  const [missing, setMissing] = useState(false)

  useEffect(() => {
    let cancelled = false
    api
      .asset(path)
      .then((a) => {
        if (cancelled) return
        setAsset(a)
        setMissing(false)
        return Promise.all([api.assetReferences(path), (a.kind === 'code' || (a.kind === 'data' && TEXT_EXTS.includes(a.ext)) || (a.kind === 'document' && TEXT_EXTS.includes(a.ext))) && a.text ? api.assetText(path) : Promise.resolve(null)]).then(([r, t]) => {
          if (cancelled) return
          setRefs(r)
          setText(t)
        })
      })
      .catch((e) => {
        if (cancelled) return
        if (String(e).includes('not found')) setMissing(true)
        else onError(String(e))
      })
    return () => {
      cancelled = true
    }
  }, [path, version, onError])

  useEffect(() => {
    void api.tools().then(setTools).catch(() => {})
  }, [])

  const url = useMemo(() => assetUrl(vaultRoot, path), [vaultRoot, path])
  // Cache-bust after an external edit so the webview reloads the bytes.
  const liveUrl = useMemo(() => `${url}${url.includes('?') ? '&' : '?'}v=${version}`, [url, version])

  const act = useCallback(
    (f: () => Promise<unknown>) => {
      f().catch((e) => onError(String(e)))
    },
    [onError],
  )

  const copyLink = () => {
    const name = asset?.name ?? path.split('/').pop() ?? path
    const src = encodeSrc(path)
    const md = asset && ['image', 'vector', 'video', 'audio', 'pdf', 'model'].includes(asset.kind) ? `![${name}](${src})` : `[${name}](${src})`
    void navigator.clipboard?.writeText(md)
  }

  const createNote = () =>
    act(async () => {
      if (!asset) return
      const stem = asset.name.replace(/\.[^.]+$/, '')
      const note = await api.createNote(stem, 'asset')
      const src = encodeSrc(asset.path)
      const embed = ['code', 'data', 'document', 'archive', 'other'].includes(asset.kind) ? `[${asset.name}](${src})` : `![${asset.name}](${src})`
      await api.saveNote({ ...note, properties: { ...note.properties, file: asset.path }, body: `${embed}\n\n` })
      onNoteCreated(note.id)
    })

  if (missing) return <div className="pane__empty">This file is no longer in the vault.</div>
  if (!asset) return <div className="app__empty">Loading…</div>

  const dims = asset.width && asset.height ? `${asset.width} × ${asset.height}` : null
  const meta = asset.meta as Record<string, unknown>
  const dimensions = Array.isArray(meta.dimensions) ? (meta.dimensions as number[]) : null
  const fmtMm = (n: number) => String(Math.round(n * 100) / 100)

  return (
    <div className="asset">
      <header className="asset__header">
        <div className="asset__title">
          <h2 title={asset.path}>{asset.name}</h2>
          <span className="hub__type">{asset.kind}</span>
          <span className="asset__path">{asset.path}</span>
        </div>
        <div className="asset__actions">
          <button type="button" className="btn" onClick={() => act(() => api.openAsset(path))} title="Open in the default app for this file type">
            <ExternalLink size={14} /> Open
          </button>
          <button type="button" className="btn" onClick={() => act(() => api.revealAsset(path))} title="Show in the file manager">
            <FolderOpen size={14} /> Reveal
          </button>
          <button type="button" className="btn" onClick={copyLink} title="Copy a Markdown link to paste into a note">
            <Copy size={14} /> Copy link
          </button>
          <button type="button" className="btn" onClick={createNote} title="New note about this file">
            <FilePlus2 size={14} /> New note
          </button>
          <button type="button" className="icon-btn" onClick={() => act(() => api.deriveAsset(path))} title="Regenerate preview and metadata">
            <RefreshCw size={14} />
          </button>
        </div>
      </header>

      <div className="asset__body">
        <div className="asset__preview">
          <Preview asset={asset} url={liveUrl} vaultRoot={vaultRoot} page={page} version={version} text={text} tools={tools} onError={onError} />
        </div>
        <aside className="asset__details">
          <h3>Details</h3>
          <dl className="asset__dl">
            <dt>Size</dt>
            <dd>{formatSize(asset.size)}</dd>
            <dt>Modified</dt>
            <dd>{new Date(asset.modified).toLocaleString()}</dd>
            {dims && (
              <>
                <dt>Dimensions</dt>
                <dd>{dims}</dd>
              </>
            )}
            {asset.duration !== null && (
              <>
                <dt>Duration</dt>
                <dd>{formatDuration(asset.duration)}</dd>
              </>
            )}
            {asset.pages !== null && (
              <>
                <dt>Pages</dt>
                <dd>{asset.pages}</dd>
              </>
            )}
            {dimensions && (
              <>
                <dt>Bounds</dt>
                <dd>
                  {fmtMm(dimensions[0])} × {fmtMm(dimensions[1])} × {fmtMm(dimensions[2])}
                </dd>
              </>
            )}
            {Object.entries(meta)
              .filter(([k]) => !['bounds', 'dimensions'].includes(k))
              .map(([k, v]) => (
                <MetaRow key={k} k={k} v={v} />
              ))}
            {asset.hash && (
              <>
                <dt>SHA-256</dt>
                <dd className="asset__hash" title={asset.hash}>
                  {asset.hash.slice(0, 16)}…
                </dd>
              </>
            )}
            {asset.derived === 'pending' && (
              <>
                <dt>Preview</dt>
                <dd>Generating…</dd>
              </>
            )}
          </dl>

          <h3>Referenced by {refs.length > 0 && <span className="qv__muted">{refs.length}</span>}</h3>
          {refs.length === 0 && <p className="hub__empty">No note links to this file yet. Use Copy link, or drag the file into a note.</p>}
          {refs.map((r, i) => (
            <button key={`${r.from.id}-${i}`} type="button" className="context__item" onClick={(e) => onOpenNote(r.from.id, e.ctrlKey || e.metaKey)}>
              <span>{r.from.title}</span>
              <span className="context__rel">{r.relation ?? (r.kind === 'embed' ? 'embed' : 'link')}</span>
            </button>
          ))}
        </aside>
      </div>
    </div>
  )
}

function MetaRow({ k, v }: { k: string; v: unknown }) {
  const label = k.replace(/_/g, ' ')
  const value = Array.isArray(v) ? v.join(', ') : typeof v === 'object' && v !== null ? JSON.stringify(v) : String(v)
  return (
    <>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </>
  )
}

interface PreviewProps {
  asset: Asset
  url: string
  vaultRoot: string
  page?: number
  version: number
  text: string | null
  tools: Tools | null
  onError: (e: string) => void
}

function Preview({ asset, url, vaultRoot, page, version, text, tools, onError }: PreviewProps) {
  switch (asset.kind) {
    case 'image':
    case 'vector':
      if (asset.kind === 'vector' && asset.ext !== 'svg') return <NoPreview asset={asset} hint={`${asset.ext.toUpperCase()} files need a conversion step to preview; open them in their own app.`} />
      return (
        <div className="asset__media">
          <img src={url} alt={asset.name} className="asset__img" />
        </div>
      )
    case 'video':
      return (
        <div className="asset__media">
          <video src={url} controls className="asset__video" />
        </div>
      )
    case 'audio':
      return (
        <div className="asset__media asset__media--audio">
          {asset.thumbnail && <img src={assetUrl(vaultRoot, asset.thumbnail)} alt="" className="asset__img" />}
          <audio src={url} controls className="asset__audio" />
        </div>
      )
    case 'pdf':
      return <PdfPreview asset={asset} vaultRoot={vaultRoot} page={page} version={version} tools={tools} onError={onError} />
    case 'model':
      if (VIEWER_EXTS.includes(asset.ext)) {
        return (
          <Suspense fallback={<div className="app__empty">Loading viewer…</div>}>
            <ModelViewer url={url} ext={asset.ext} version={version} />
          </Suspense>
        )
      }
      return <NoPreview asset={asset} hint={`${asset.ext.toUpperCase()} needs a converter (coming later); open it in your CAD app, or export a GLB/STL next to it for a preview here.`} />
    case 'code':
      return text !== null ? <pre className="asset__code">{text}</pre> : <NoPreview asset={asset} />
    case 'data':
      if ((asset.ext === 'csv' || asset.ext === 'tsv') && text !== null) return <CsvTable text={text} sep={asset.ext === 'tsv' ? '\t' : ','} />
      return text !== null ? <pre className="asset__code">{text}</pre> : <NoPreview asset={asset} />
    case 'document':
      return text !== null ? <pre className="asset__code asset__code--prose">{text}</pre> : <NoPreview asset={asset} />
    default:
      return <NoPreview asset={asset} />
  }
}

function NoPreview({ asset, hint }: { asset: Asset; hint?: string }) {
  return (
    <div className="asset__none">
      <div className="asset__none-ext">.{asset.ext || 'file'}</div>
      <p>{hint ?? 'No inline preview for this kind of file. Open it in its own app; Graphite keeps the link and metadata.'}</p>
    </div>
  )
}

function PdfPreview({ asset, vaultRoot, page, version, tools, onError }: { asset: Asset; vaultRoot: string; page?: number; version: number; tools: Tools | null; onError: (e: string) => void }) {
  const total = asset.pages ?? 1
  const [current, setCurrent] = useState(Math.min(Math.max(page ?? 1, 1), total))
  const [src, setSrc] = useState<string | null>(null)
  const [text, setText] = useState<string | null>(null)
  const [showText, setShowText] = useState(false)
  const canRender = tools?.pdftoppm ?? true

  useEffect(() => {
    setCurrent(Math.min(Math.max(page ?? 1, 1), total))
  }, [page, total])

  useEffect(() => {
    if (!canRender) return
    let cancelled = false
    setSrc(null)
    api
      .assetPage(asset.path, current)
      .then((rel) => {
        if (!cancelled) setSrc(`${assetUrl(vaultRoot, rel)}?v=${version}`)
      })
      .catch((e) => {
        if (!cancelled) onError(String(e))
      })
    return () => {
      cancelled = true
    }
  }, [asset.path, current, canRender, vaultRoot, version, onError])

  useEffect(() => {
    if (!showText || text !== null) return
    api
      .assetText(asset.path)
      .then((t) => setText(t ?? ''))
      .catch((e) => onError(String(e)))
  }, [showText, text, asset.path, onError])

  const go = (n: number) => setCurrent(Math.min(Math.max(n, 1), total))

  return (
    <div className="pdf">
      <div className="pdf__bar">
        <button type="button" className="btn" onClick={() => go(current - 1)} disabled={current <= 1}>
          ‹
        </button>
        <span>
          Page{' '}
          <input className="pdf__page" type="number" min={1} max={total} value={current} onChange={(e) => go(Number(e.target.value) || 1)} aria-label="Page" /> of {total}
        </span>
        <button type="button" className="btn" onClick={() => go(current + 1)} disabled={current >= total}>
          ›
        </button>
        {asset.text && (
          <button type="button" className={`btn ${showText ? 'is-active' : ''}`} onClick={() => setShowText((s) => !s)}>
            {showText ? 'Hide text' : 'Text'}
          </button>
        )}
      </div>
      {!canRender && <p className="hub__empty">Install poppler (pdftoppm) to see pages here. The text below is still searchable.</p>}
      {canRender && (src ? <img className="pdf__page-img" src={src} alt={`Page ${current}`} /> : <div className="app__empty">Rendering page {current}…</div>)}
      {(showText || !canRender) && text !== null && <pre className="asset__code asset__code--prose">{text || 'No text could be extracted from this PDF (it may be scanned; OCR comes in a later milestone).'}</pre>}
      {!canRender && text === null && <TextLoader path={asset.path} onText={setText} onError={onError} />}
    </div>
  )
}

function TextLoader({ path, onText, onError }: { path: string; onText: (t: string) => void; onError: (e: string) => void }) {
  useEffect(() => {
    api
      .assetText(path)
      .then((t) => onText(t ?? ''))
      .catch((e) => onError(String(e)))
  }, [path, onText, onError])
  return null
}

function CsvTable({ text, sep }: { text: string; sep: string }) {
  const rows = useMemo(() => {
    const lines = text.split(/\r?\n/).filter((l) => l.trim() !== '')
    const limited = lines.slice(0, 501)
    return { cells: limited.map((l) => splitCsv(l, sep)), truncated: lines.length > 501, total: lines.length }
  }, [text, sep])
  if (rows.cells.length === 0) return <NoPreviewEmpty />
  const [head, ...body] = rows.cells
  return (
    <div className="asset__table-wrap">
      <table className="asset__table">
        <thead>
          <tr>
            {head.map((c, i) => (
              <th key={i}>{c}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {body.map((r, i) => (
            <tr key={i}>
              {r.map((c, j) => (
                <td key={j}>{c}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {rows.truncated && <p className="hub__empty">Showing the first 500 of {rows.total - 1} rows.</p>}
    </div>
  )
}

function NoPreviewEmpty() {
  return <p className="hub__empty">Empty file.</p>
}

/** Minimal CSV field splitting with quotes. */
export function splitCsv(line: string, sep = ','): string[] {
  const out: string[] = []
  let cur = ''
  let quoted = false
  for (let i = 0; i < line.length; i++) {
    const ch = line[i]
    if (quoted) {
      if (ch === '"' && line[i + 1] === '"') {
        cur += '"'
        i++
      } else if (ch === '"') quoted = false
      else cur += ch
    } else if (ch === '"') quoted = true
    else if (ch === sep) {
      out.push(cur)
      cur = ''
    } else cur += ch
  }
  out.push(cur)
  return out
}
