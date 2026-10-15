/** A line of a two-way diff: kept, added (in `b` only) or removed (in `a` only). */
export interface DiffLine {
  kind: 'same' | 'add' | 'del'
  text: string
}

/**
 * Line diff of `a` against `b` (longest common subsequence). Notes are
 * small, so the quadratic table is fine; very long notes fall back to a
 * whole-text replace so the UI never stalls.
 */
export function diffLines(a: string, b: string): DiffLine[] {
  const al = a.split('\n')
  const bl = b.split('\n')
  if (al.length * bl.length > 4_000_000) {
    return [...al.map((text) => ({ kind: 'del' as const, text })), ...bl.map((text) => ({ kind: 'add' as const, text }))]
  }
  const n = al.length
  const m = bl.length
  // lcs[i][j] = length of the LCS of al[i..] and bl[j..]
  const lcs: Uint32Array[] = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1))
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i][j] = al[i] === bl[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1])
    }
  }
  const out: DiffLine[] = []
  let i = 0
  let j = 0
  while (i < n && j < m) {
    if (al[i] === bl[j]) {
      out.push({ kind: 'same', text: al[i] })
      i++
      j++
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      out.push({ kind: 'del', text: al[i] })
      i++
    } else {
      out.push({ kind: 'add', text: bl[j] })
      j++
    }
  }
  for (; i < n; i++) out.push({ kind: 'del', text: al[i] })
  for (; j < m; j++) out.push({ kind: 'add', text: bl[j] })
  return out
}

/** "+3 −1" style summary of a diff. */
export function diffSummary(lines: DiffLine[]): string {
  const add = lines.filter((l) => l.kind === 'add').length
  const del = lines.filter((l) => l.kind === 'del').length
  if (add === 0 && del === 0) return 'no changes'
  return `+${add} −${del}`
}
