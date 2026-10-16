import { useEffect, useMemo, useState } from 'react'
import { Archive, Box, Braces, Code2, File, FileText, Image as ImageIcon, Music, PenTool, Plus, Video } from 'lucide-react'
import { pickAndAttach } from '../filePicker'
import { api, assetUrl, ASSET_KINDS, type Asset, type AssetKind } from '../api'
import { formatDuration, formatSize } from './AssetView'

interface Props {
  vaultRoot: string
  /** Bumped when files or their previews change. */
  version: number
  /** Restrict to one folder (a project's attachments), vault-relative with no trailing slash. */
  folder?: string
  /** Compact grid for embedding in the project hub. */
  compact?: boolean
  onOpen: (path: string, newTab?: boolean) => void
  onChanged: () => void
  onError: (e: string) => void
}

export const KIND_LABEL: Record<AssetKind, string> = {
  image: 'Images',
  vector: 'Vector',
  video: 'Video',
  audio: 'Audio',
  pdf: 'PDF',
  model: '3D models',
  code: 'Code',
  data: 'Data',
  document: 'Documents',
  archive: 'Archives',
  other: 'Other',
}

export function KindIcon({ kind, size = 14 }: { kind: AssetKind; size?: number }) {
  switch (kind) {
    case 'image':
      return <ImageIcon size={size} />
    case 'vector':
      return <PenTool size={size} />
    case 'video':
      return <Video size={size} />
    case 'audio':
      return <Music size={size} />
    case 'pdf':
      return <FileText size={size} />
    case 'model':
      return <Box size={size} />
    case 'code':
      return <Code2 size={size} />
    case 'data':
      return <Braces size={size} />
    case 'archive':
      return <Archive size={size} />
    default:
      return <File size={size} />
  }
}

/**
 * Every file in the vault (or one folder of it) as a filterable gallery:
 * thumbnails where there are any, kind and size otherwise. Click opens the
 * file's own view; "Add files" copies files from anywhere into the vault.
 */
export function FilesView({ vaultRoot, version, folder, compact, onOpen, onChanged, onError }: Props) {
  const [assets, setAssets] = useState<Asset[]>([])
  const [kind, setKind] = useState<AssetKind | null>(null)
  const [filter, setFilter] = useState('')
  /** Files whose preview failed to load; they show their kind icon instead of a broken image. */
  const [broken, setBroken] = useState<Set<string>>(() => new Set())
  const markBroken = (path: string) => setBroken((b) => (b.has(path) ? b : new Set(b).add(path)))
  useEffect(() => setBroken(new Set()), [version])

  useEffect(() => {
    let cancelled = false
    api
      .assets(undefined, folder ? `${folder}/` : undefined)
      .then((a) => {
        if (!cancelled) setAssets(folder ? a.filter((x) => x.path.startsWith(`${folder}/`)) : a)
      })
      .catch((e) => onError(String(e)))
    return () => {
      cancelled = true
    }
  }, [version, folder, onError])

  const counts = useMemo(() => {
    const c = new Map<AssetKind, number>()
    for (const a of assets) c.set(a.kind, (c.get(a.kind) ?? 0) + 1)
    return c
  }, [assets])

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase()
    return assets.filter((a) => (!kind || a.kind === kind) && (!q || a.path.toLowerCase().includes(q)))
  }, [assets, kind, filter])

  const addFiles = async () => {
    try {
      const added = await pickAndAttach({ multiple: true, title: folder ? `Add files to ${folder}` : 'Add files to the vault', folder })
      if (added.length === 0) return
      await api.deriveAssets()
      onChanged()
    } catch (e) {
      onError(String(e))
    }
  }

  return (
    <div className={`files ${compact ? 'files--compact' : ''}`}>
      <div className="files__bar">
        {!compact && <h2>Files</h2>}
        <div className="files__chips">
          <button type="button" className={`chip ${kind === null ? 'is-active' : ''}`} onClick={() => setKind(null)}>
            All <span className="chip__n">{assets.length}</span>
          </button>
          {ASSET_KINDS.filter((k) => counts.has(k)).map((k) => (
            <button key={k} type="button" className={`chip ${kind === k ? 'is-active' : ''}`} onClick={() => setKind(kind === k ? null : k)}>
              <KindIcon kind={k} size={12} /> {KIND_LABEL[k]} <span className="chip__n">{counts.get(k)}</span>
            </button>
          ))}
        </div>
        <input className="files__filter" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Filter by name or folder" aria-label="Filter files" />
        <button type="button" className="btn" onClick={() => void addFiles()} title="Copy files from anywhere on disk into the vault">
          <Plus size={14} /> Add files
        </button>
      </div>
      {shown.length === 0 && (
        <p className="hub__empty">
          {assets.length === 0 ? (folder ? `No files in ${folder}/ yet. Add some, or drop them into a note.` : 'No files in the vault yet. Add some, or drop them into a note; anything under attachments/ shows up here.') : 'Nothing matches.'}
        </p>
      )}
      <div className="files__grid">
        {shown.map((a) => (
          <button key={a.path} type="button" className="file-card" onClick={(e) => onOpen(a.path, e.ctrlKey || e.metaKey)} title={a.path}>
            <div className="file-card__thumb">
              {broken.has(a.path) ? (
                <span className="file-card__icon">
                  <KindIcon kind={a.kind} size={28} />
                  <span className="file-card__ext">{a.ext || a.kind}</span>
                </span>
              ) : a.thumbnail ? (
                <img src={`${assetUrl(vaultRoot, a.thumbnail)}?v=${version}`} alt="" loading="lazy" onError={() => markBroken(a.path)} />
              ) : a.kind === 'image' || (a.kind === 'vector' && a.ext === 'svg') ? (
                <img src={`${assetUrl(vaultRoot, a.path)}?v=${version}`} alt="" loading="lazy" className={a.kind === 'vector' ? 'is-contain' : ''} onError={() => markBroken(a.path)} />
              ) : (
                <span className="file-card__icon">
                  <KindIcon kind={a.kind} size={28} />
                  <span className="file-card__ext">{a.ext || a.kind}</span>
                </span>
              )}
              {a.duration !== null && <span className="file-card__badge">{formatDuration(a.duration)}</span>}
              {a.pages !== null && a.pages > 1 && <span className="file-card__badge">{a.pages} pp</span>}
            </div>
            <div className="file-card__name">{a.name}</div>
            <div className="file-card__meta">
              {folder ? a.path.slice(folder.length + 1, -a.name.length).replace(/\/$/, '') || formatSize(a.size) : a.path.slice(0, -a.name.length).replace(/\/$/, '') || formatSize(a.size)}
            </div>
          </button>
        ))}
      </div>
    </div>
  )
}
