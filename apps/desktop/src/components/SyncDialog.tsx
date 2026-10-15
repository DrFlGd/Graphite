import { useEffect, useState } from 'react'
import { Check, Copy } from 'lucide-react'
import { api, type SyncStatus, type VaultConfig } from '../api'
import { ago } from './sync'

interface Props {
  status: SyncStatus | null
  onStatus: (s: SyncStatus) => void
  onClose: () => void
  onError: (e: string) => void
}

/**
 * The sync panel: who else shares this vault, the relay this device talks
 * to (connect with an invite, or make a key and show the invite), and the
 * large-file limit.
 */
export function SyncDialog({ status, onStatus, onClose, onError }: Props) {
  const [url, setUrl] = useState('')
  const [invite, setInvite] = useState('')
  const [shown, setShown] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [busy, setBusy] = useState<'create' | 'connect' | 'leave' | null>(null)
  const [failure, setFailure] = useState<string | null>(null)
  const [config, setConfig] = useState<VaultConfig | null>(null)

  useEffect(() => {
    void api.getConfig().then(setConfig).catch((e) => onError(String(e)))
  }, [onError])

  const relay = status?.relay ?? null

  const run = async (what: 'create' | 'connect' | 'leave') => {
    setBusy(what)
    setFailure(null)
    try {
      if (what === 'create') {
        const inv = await api.relayCreate(url.trim())
        setShown(inv)
      } else if (what === 'connect') {
        await api.relayConnect(invite.trim())
        setInvite('')
      } else {
        await api.relayDisconnect()
        setShown(null)
      }
      onStatus(await api.syncStatus())
    } catch (e) {
      setFailure(String(e))
    } finally {
      setBusy(null)
    }
  }

  const showInvite = async () => {
    try {
      setShown((await api.relayInvite()) ?? null)
    } catch (e) {
      onError(String(e))
    }
  }

  const copy = async () => {
    if (!shown) return
    try {
      await navigator.clipboard.writeText(shown)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch (e) {
      onError(String(e))
    }
  }

  const saveLimit = async (mb: number) => {
    if (!config || !Number.isFinite(mb) || mb < 0) return
    try {
      setConfig(await api.setConfig({ ...config, large_file_mb: Math.round(mb) }))
    } catch (e) {
      onError(String(e))
    }
  }

  return (
    <div className="modal" onMouseDown={busy ? undefined : onClose}>
      <div className="modal__card picker syncd" onMouseDown={(e) => e.stopPropagation()} role="dialog" aria-label="Sync">
        <div className="syncd__body">
          <section>
            <h3>Devices</h3>
            {!status?.enabled && <p className="qv__muted">Sync is off. It turns on with the first pass: press Sync in the sidebar, share the folder, or connect a relay below.</p>}
            {status?.enabled && (
              <ul className="syncd__devices">
                {status.devices.map((d) => (
                  <li key={d.id}>
                    <span>
                      {d.name}
                      {d.this_device ? ' (this device)' : ''}
                    </span>
                    <span className="qv__muted">
                      {d.notes} notes · {ago(d.last_sync)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section>
            <h3>Relay</h3>
            {relay ? (
              <>
                <p>
                  Connected to <strong>{relay.url}</strong>. Last exchange {ago(relay.last)}; {relay.files} files sent or received.
                </p>
                {relay.kept_local.length > 0 && (
                  <p className="qv__muted">
                    Kept on this device (over the limit): {relay.kept_local.slice(0, 5).join(', ')}
                    {relay.kept_local.length > 5 ? ` and ${relay.kept_local.length - 5} more` : ''}
                  </p>
                )}
                <div className="syncd__row">
                  <button type="button" className="btn" onClick={() => void showInvite()}>
                    Show invite
                  </button>
                  <button type="button" className="btn" onClick={() => void run('leave')} disabled={busy !== null}>
                    {busy === 'leave' ? 'Leaving…' : 'Leave relay'}
                  </button>
                </div>
              </>
            ) : (
              <>
                <p className="qv__muted">A relay syncs devices that do not share a folder, such as a phone. Everything is encrypted before it leaves this device; the relay only stores what it cannot read.</p>
                <label className="syncd__field">
                  <span>Paste an invite from another device</span>
                  <div className="syncd__row">
                    <input value={invite} onChange={(e) => setInvite(e.target.value)} placeholder="https://relay.example.com#key" spellCheck={false} disabled={busy !== null} />
                    <button type="button" className="btn btn--primary" onClick={() => void run('connect')} disabled={busy !== null || !invite.trim()}>
                      {busy === 'connect' ? 'Connecting…' : 'Connect'}
                    </button>
                  </div>
                </label>
                <label className="syncd__field">
                  <span>Or start a new vault on a relay</span>
                  <div className="syncd__row">
                    <input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://relay.example.com" spellCheck={false} disabled={busy !== null} />
                    <button type="button" className="btn" onClick={() => void run('create')} disabled={busy !== null || !url.trim()}>
                      {busy === 'create' ? 'Creating…' : 'Create key'}
                    </button>
                  </div>
                </label>
              </>
            )}
            {shown && (
              <div className="syncd__invite">
                <p className="qv__muted">Paste this on your other devices. It is the key to the vault: anyone who has it can read everything.</p>
                <div className="syncd__row">
                  <code>{shown}</code>
                  <button type="button" className="btn" onClick={() => void copy()} aria-label="Copy invite">
                    {copied ? <Check size={14} /> : <Copy size={14} />}
                  </button>
                </div>
              </div>
            )}
            {failure && <p className="import__failure">{failure}</p>}
          </section>

          <section>
            <h3>Large files</h3>
            <label className="syncd__field syncd__field--inline">
              <span>Files over</span>
              <input type="number" min={0} step={1} value={config?.large_file_mb ?? ''} onChange={(e) => void saveLimit(Number(e.target.value))} disabled={!config} aria-label="Large-file limit in megabytes" />
              <span>MB are linked where they are instead of copied in, and stay on this device instead of going through the relay.</span>
            </label>
          </section>
        </div>
        <div className="picker__footer">
          <button type="button" className="btn" onClick={onClose} disabled={busy !== null}>
            Close
          </button>
        </div>
      </div>
    </div>
  )
}
