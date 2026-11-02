import type { BlockContext, SnippetItem } from '@graphite/ui'
import { api } from './api'

/** How long the snippet list is reused before asking the core again. */
const FRESH_MS = 5000

let cache: { at: number; list: Promise<SnippetItem[]> } | null = null

/** The vault's snippets, for the editor's `/` and Insert menus. */
export function listSnippets(): Promise<SnippetItem[]> {
  if (!cache || Date.now() - cache.at > FRESH_MS) {
    cache = { at: Date.now(), list: api.snippets().catch(() => []) }
  }
  return cache.list
}

/** Forget the cached list, after a snippet is added or the vault changes. */
export function forgetSnippets() {
  cache = null
}

/** Editor callbacks that insert snippets into the note titled `title()`. */
export function snippetContext(title: () => string): Pick<BlockContext, 'listSnippets' | 'renderSnippet'> {
  return {
    listSnippets,
    renderSnippet: (id) => api.renderSnippet(id, title()),
  }
}

/** A name for a new snippet: the first few words of its first sentence. */
export function snippetName(markdown: string): string {
  const line = markdown.split('\n').find((l) => l.trim()) ?? ''
  const plain = line
    .replace(/[#>*_`~=[\]()!|]+/g, ' ')
    .replace(/^\s*(?:[-+]|\d+[.)])\s+/, '')
    .replace(/\s+/g, ' ')
    .trim()
  const sentence = plain.split(/[.!?:;](?:\s|$)/)[0].trim()
  const words = sentence.split(' ').slice(0, 5).join(' ')
  return (words.length > 40 ? words.slice(0, 40).trimEnd() : words) || 'Snippet'
}
