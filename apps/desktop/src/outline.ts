/** One heading of a note, for the outline (C16). */
export interface OutlineItem {
  level: number
  text: string
  /** What follows `#` in a link to it: `Parts#Motors` when nested. */
  anchor: string
}

/** Inline Markdown that is not part of a heading's words. */
function plain(s: string): string {
  return s
    .replace(/\s\^[A-Za-z0-9-]+\s*$/, '') // block id
    .replace(/!?\[\[([^\]|#]*)(#[^\]|]*)?(\|([^\]]*))?\]\]/g, (_, t: string, _h, _p, label?: string) => label ?? t) // [[target|label]]
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1') // [text](url)
    .replace(/(\*\*|__|~~|==|`)/g, '') // bold, strike, highlight, code
    .replace(/(^|\W)[*_](\S[^*_]*)[*_](?=\W|$)/g, '$1$2') // italics
    .replace(/\s+/g, ' ')
    .trim()
}

/**
 * The note's headings in order, leaving out lines inside fenced code and a
 * first heading that only repeats the note's `title`.
 */
export function markdownOutline(body: string, title?: string): OutlineItem[] {
  const out: OutlineItem[] = []
  const stack: { level: number; text: string }[] = []
  let fence: string | null = null
  for (const line of body.split('\n')) {
    const t = line.trimStart()
    if (fence) {
      if (t.startsWith(fence)) fence = null
      continue
    }
    if (t.startsWith('```') || t.startsWith('~~~')) {
      fence = t.slice(0, 3)
      continue
    }
    const m = /^ {0,3}(#{1,6})[ \t]+(.*?)[ \t]*#*[ \t]*$/.exec(line)
    if (!m) continue
    const level = m[1].length
    const text = plain(m[2])
    if (!text) continue
    if (out.length === 0 && stack.length === 0 && title !== undefined && text.toLowerCase() === title.trim().toLowerCase()) {
      title = undefined
      continue
    }
    while (stack.length && stack[stack.length - 1].level >= level) stack.pop()
    stack.push({ level, text })
    out.push({ level, text, anchor: stack.map((s) => s.text).join('#') })
  }
  return out
}
