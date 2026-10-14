// Typed wrappers over the Tauri commands in `src-tauri/src/lib.rs`.
// The UI never touches the file system; everything goes through here.

import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface VaultInfo {
  root: string
  name: string
  note_count: number
}

export interface NoteSummary {
  id: string
  path: string
  title: string
  type: string
  tags: string[]
  updated: string | null
}

export interface Note extends NoteSummary {
  aliases: string[]
  created: string | null
  properties: Record<string, unknown>
  body: string
}

export interface SearchHit {
  id: string
  path: string
  title: string
  snippet: string
}

export interface Backlink {
  from: NoteSummary
  relation: string | null
  kind: 'wiki' | 'embed'
  position: number
}

export interface Outgoing {
  target: string
  id: string | null
}

export const api = {
  openVault: (path: string) => invoke<VaultInfo>('open_vault', { path }),
  vaultInfo: () => invoke<VaultInfo | null>('vault_info'),
  listNotes: () => invoke<NoteSummary[]>('list_notes'),
  readNote: (id: string) => invoke<Note>('read_note', { id }),
  saveNote: (note: Note) => invoke<Note>('save_note', { note }),
  createNote: (title: string, noteType?: string, folder?: string) => invoke<Note>('create_note', { title, noteType, folder }),
  deleteNote: (id: string) => invoke<void>('delete_note', { id }),
  search: (query: string, limit = 30) => invoke<SearchHit[]>('search', { query, limit }),
  backlinks: (id: string) => invoke<Backlink[]>('backlinks', { id }),
  unlinkedMentions: (id: string) => invoke<NoteSummary[]>('unlinked_mentions', { id }),
  outgoingLinks: (id: string) => invoke<Outgoing[]>('outgoing_links', { id }),
  followLink: (target: string) => invoke<string>('follow_link', { target }),
  completeTitle: (prefix: string) => invoke<NoteSummary[]>('complete_title', { prefix }),
  rebuildIndex: () => invoke<number>('rebuild_index'),
  attachFile: (src: string, folder?: string) => invoke<string>('attach_file', { src, folder }),
  /** Subscribe to notes changed on disk by something other than this app. */
  onVaultChanged: (cb: (ids: string[]) => void): Promise<UnlistenFn> =>
    listen<{ ids: string[] }>('vault://changed', (e) => cb(e.payload.ids)),
}

/** Turn a vault-relative src into a URL the webview may load. */
export function assetUrl(vaultRoot: string, src: string): string {
  if (/^(https?:|data:|blob:|asset:)/i.test(src)) return src
  const sep = vaultRoot.includes('\\') ? '\\' : '/'
  const rel = src.replace(/^\.?\//, '').split('/').join(sep)
  return convertFileSrc(`${vaultRoot}${sep}${rel}`)
}
