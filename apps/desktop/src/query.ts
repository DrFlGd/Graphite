// Pure helpers for the query views: editing terms in a query line and
// shaping result rows into trees and groups. Tested in query.test.ts.

import type { ItemRow, QueryResult } from './api'

/** Split a query line into tokens, keeping "quoted" and [[linked]] parts whole. */
export function tokenize(query: string): string[] {
  const out: string[] = []
  let cur = ''
  let quotes = false
  let depth = 0
  for (let i = 0; i < query.length; i++) {
    const c = query[i]
    if (c === '"') {
      quotes = !quotes
      cur += c
    } else if (c === '[' && query[i + 1] === '[' && !quotes) {
      depth++
      cur += '[['
      i++
    } else if (c === ']' && query[i + 1] === ']' && depth > 0 && !quotes) {
      depth--
      cur += ']]'
      i++
    } else if (/\s/.test(c) && !quotes && depth === 0) {
      if (cur) out.push(cur)
      cur = ''
    } else cur += c
  }
  if (cur) out.push(cur)
  return out
}

const isTerm = (tok: string, key: string) => tok.toLowerCase().startsWith(key.toLowerCase() + ':') && !tok.startsWith('[[')

/** The value of `key:value` in the query, if present. */
export function termValue(query: string, key: string): string | null {
  const tok = tokenize(query).find((t) => isTerm(t, key))
  return tok ? tok.slice(key.length + 1) : null
}

/** Return the query with `key:value` set (replacing an existing term) or, with `null`, removed. */
export function withTerm(query: string, key: string, value: string | null): string {
  const toks = tokenize(query).filter((t) => !isTerm(t, key))
  if (value !== null && value !== '') toks.push(`${key}:${value}`)
  return toks.join(' ')
}

/** Which domain a query runs over, mirroring the core's rule. */
export function domainOf(query: string): 'notes' | 'items' {
  const t = termValue(query, 'type')?.toLowerCase()
  return t === 'task' || t === 'gap' || t === 'item' || t === 'items' ? 'items' : 'notes'
}

export interface TreeNode {
  row: ItemRow
  children: TreeNode[]
}

/**
 * Nest items by their `parent` line within each note, keeping the result's
 * order. A child whose parent is not in the result is shown at the top level.
 */
export function buildTree(rows: ItemRow[]): TreeNode[] {
  const nodes = new Map<string, TreeNode>()
  const roots: TreeNode[] = []
  for (const row of rows) nodes.set(`${row.note_id}:${row.line}`, { row, children: [] })
  for (const node of nodes.values()) {
    const parent = node.row.parent === null ? undefined : nodes.get(`${node.row.note_id}:${node.row.parent}`)
    if (parent) parent.children.push(node)
    else roots.push(node)
  }
  return roots
}

export interface GroupSlice<T> {
  key: string
  rows: T[]
}

/** Cut grouped result rows into their groups (the core orders rows group by group). */
export function sliceGroups<T>(rows: T[], result: Pick<QueryResult, 'groups'>): GroupSlice<T>[] {
  if (result.groups.length === 0) return [{ key: '', rows }]
  const out: GroupSlice<T>[] = []
  let at = 0
  for (const g of result.groups) {
    out.push({ key: g.key, rows: rows.slice(at, at + g.count) })
    at += g.count
  }
  return out
}

/** Property keys seen across note rows, in first-seen order. */
export function propertyKeys(rows: { properties: Record<string, unknown> }[]): string[] {
  const keys: string[] = []
  for (const r of rows) for (const k of Object.keys(r.properties)) if (!keys.includes(k)) keys.push(k)
  return keys
}

/** Render a property value for a cell. */
export function cellText(v: unknown): string {
  if (v === null || v === undefined) return ''
  if (typeof v === 'string') return v
  if (Array.isArray(v)) return v.map(cellText).join(', ')
  return JSON.stringify(v)
}

/** Parse what the user typed in a property cell back into a value, keeping numbers and booleans. */
export function cellValue(text: string, previous: unknown): unknown {
  const t = text.trim()
  if (t === '') return null
  if (typeof previous === 'number' && /^-?\d+(\.\d+)?$/.test(t)) return Number(t)
  if (typeof previous === 'boolean' && (t === 'true' || t === 'false')) return t === 'true'
  if (Array.isArray(previous)) return t.split(',').map((s) => s.trim()).filter(Boolean)
  return t
}
