/** Search snippets from the index mark matches with these control characters (core `SNIPPET_START`/`SNIPPET_END`). */
const START = '\u0002'
const END = '\u0003'

export interface SnippetPart {
  text: string
  match: boolean
}

/** Markdown syntax that reads as noise in a one-line preview. */
function stripMarkdown(s: string): string {
  return s
    .replace(/!?\[\[([^\]|]*\|)?([^\]]*)\]\]/g, '$2') // [[target|label]], ![[file]]
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1') // [text](url), ![alt](src)
    .replace(/^\s{0,3}#{1,6}\s+/gm, '') // headings
    .replace(/^\s*>\s?/gm, '') // quotes
    .replace(/^\s*(?:[-*+]|\d+[.)])\s+(?:\[[ xX/-]\]\s+)?/gm, '') // list and task markers
    .replace(/(\*\*|__|~~|`)/g, '') // bold, strike, code
    .replace(/(^|\W)[*_](\S[^*_]*)[*_](?=\W|$)/g, '$1$2') // italics
    .replace(/\s+/g, ' ')
}

/** A snippet as plain text with its matched terms flagged. */
export function snippetParts(snippet: string): SnippetPart[] {
  // Strip the whole line first so syntax around a match goes too.
  const clean = stripMarkdown(snippet)
  const out: SnippetPart[] = []
  let at = 0
  for (const m of clean.matchAll(new RegExp(`${START}([^${END}]*)${END}`, 'g'))) {
    if (m.index > at) out.push({ text: clean.slice(at, m.index), match: false })
    out.push({ text: m[1], match: true })
    at = m.index + m[0].length
  }
  if (at < clean.length) out.push({ text: clean.slice(at), match: false })
  return out.filter((p) => p.text !== '')
}

/** The snippet as one plain line, without match marks. */
export function snippetText(snippet: string): string {
  return snippetParts(snippet)
    .map((p) => p.text)
    .join('')
}
