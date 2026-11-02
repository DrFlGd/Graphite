import { MarkdownManager } from '@tiptap/markdown'
import { matchInlineMath } from './blocks/math'

/**
 * Tiptap escapes the Markdown characters it knows about in plain text; this
 * adds the syntax Graphite's own blocks introduced, so text that only looks
 * like it stays text: a `$` that would open a formula is written `\$`.
 */
type Escaper = { escapeMarkdownSyntax(text: string): string }
const proto = MarkdownManager.prototype as unknown as Escaper & { __graphiteEscapes?: boolean }
if (!proto.__graphiteEscapes) {
  const base = proto.escapeMarkdownSyntax
  proto.escapeMarkdownSyntax = function (text: string): string {
    const out = base.call(this, text)
    if (!out.includes('$')) return out
    let s = ''
    for (let i = 0; i < out.length; i++) {
      const c = out[i]
      if (c === '\\') {
        s += c + (out[i + 1] ?? '')
        i++
        continue
      }
      if (c === '$' && matchInlineMath(out.slice(i))) s += '\\$'
      else s += c
    }
    return s
  }
  proto.__graphiteEscapes = true
}
