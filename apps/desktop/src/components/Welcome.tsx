import { FolderOpen } from 'lucide-react'

export function Welcome({ onPick, error }: { onPick: () => void; error: string | null }) {
  return (
    <div className="welcome">
      <div className="welcome__card">
        <h1>Graphite</h1>
        <p>A vault is just a folder of Markdown notes and attachments. Pick one to open it, or pick an empty folder to start a new vault.</p>
        <button type="button" className="btn btn--primary" onClick={onPick}>
          <FolderOpen size={16} /> Open vault folder
        </button>
        {error && <p className="welcome__error">{error}</p>}
      </div>
    </div>
  )
}
