/** One tag in the tag pane (C17): `garden` holds `garden/beds`, and so on. */
export interface TagNode {
  /** The last part, as written (`beds`). */
  name: string
  /** The whole tag (`garden/beds`). */
  path: string
  /** Notes with this tag or one under it. */
  count: number
  children: TagNode[]
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

/** Every tag used in `notes`, nested on `/`, compared without case. */
export function tagTree(notes: { tags: string[] }[]): TagNode[] {
  const roots: TagNode[] = []
  const byPath = new Map<string, { node: TagNode; seen: Set<number> }>()
  notes.forEach((n, i) => {
    for (const tag of n.tags) {
      const parts = tag.split('/').filter(Boolean)
      let level = roots
      for (let d = 0; d < parts.length; d++) {
        const path = parts.slice(0, d + 1).join('/')
        const key = path.toLowerCase()
        let entry = byPath.get(key)
        if (!entry) {
          entry = { node: { name: parts[d], path, count: 0, children: [] }, seen: new Set() }
          byPath.set(key, entry)
          level.push(entry.node)
        }
        if (!entry.seen.has(i)) {
          entry.seen.add(i)
          entry.node.count++
        }
        level = entry.node.children
      }
    }
  })
  const sort = (nodes: TagNode[]) => {
    nodes.sort((a, b) => collator.compare(a.name, b.name))
    for (const n of nodes) sort(n.children)
  }
  sort(roots)
  return roots
}

/** Whether a note has `tag` or a tag under it (`garden` matches `garden/beds`). */
export function hasTag(note: { tags: string[] }, tag: string): boolean {
  const t = tag.toLowerCase().replace(/^#/, '')
  return note.tags.some((x) => {
    const l = x.toLowerCase()
    return l === t || l.startsWith(`${t}/`)
  })
}
