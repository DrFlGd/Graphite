import { Node, mergeAttributes, nodeInputRule } from '@tiptap/core'

export interface WikiLinkAttrs {
  target: string
  heading: string | null
  display: string | null
  /** Written as `![[…]]`: an embed of a file or note rather than a plain link. */
  embed?: boolean
}

const WIKI_RE = /^(!?)\[\[([^[\]\n]+?)\]\]/

/** Split the inside of `[[…]]` into target, heading and display text. */
export function parseWikiInner(inner: string): WikiLinkAttrs {
  const [targetPart, display] = splitOnce(inner, '|')
  const [target, heading] = splitOnce(targetPart, '#')
  return { target: target.trim(), heading: heading?.trim() ?? null, display: display?.trim() ?? null }
}

export function wikiLinkToMarkdown(attrs: WikiLinkAttrs): string {
  let inner = attrs.target
  if (attrs.heading) inner += `#${attrs.heading}`
  if (attrs.display) inner += `|${attrs.display}`
  return `${attrs.embed ? '!' : ''}[[${inner}]]`
}

function splitOnce(s: string, sep: string): [string, string | undefined] {
  const i = s.indexOf(sep)
  return i < 0 ? [s, undefined] : [s.slice(0, i), s.slice(i + 1)]
}

/**
 * `[[Note]]` links as an atomic inline node. The same syntax the core's link
 * extractor understands (see `core/src/links.rs`), so what the editor shows
 * as a chip is exactly what the index counts as a link.
 */
export const WikiLink = Node.create({
  name: 'wikiLink',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  markdownTokenName: 'wikiLink',

  addAttributes() {
    return {
      target: { default: '' },
      heading: { default: null },
      display: { default: null },
      embed: { default: false },
    }
  },

  parseHTML() {
    return [
      {
        tag: 'span[data-wiki-link]',
        getAttrs: (el) => {
          const e = el as HTMLElement
          return { target: e.getAttribute('data-target') ?? '', heading: e.getAttribute('data-heading'), display: e.getAttribute('data-display'), embed: e.hasAttribute('data-embed') }
        },
      },
    ]
  },

  renderHTML({ node, HTMLAttributes }) {
    const attrs = node.attrs as WikiLinkAttrs
    const text = attrs.display ?? (attrs.heading ? `${attrs.target} › ${attrs.heading}` : attrs.target)
    return [
      'span',
      mergeAttributes(HTMLAttributes, {
        'data-wiki-link': '',
        'data-target': attrs.target,
        'data-heading': attrs.heading ?? undefined,
        'data-display': attrs.display ?? undefined,
        'data-embed': attrs.embed ? '' : undefined,
        class: attrs.embed ? 'gr-wiki-link gr-wiki-link--embed' : 'gr-wiki-link',
        role: 'link',
        tabindex: 0,
      }),
      text,
    ]
  },

  renderText({ node }) {
    return wikiLinkToMarkdown(node.attrs as WikiLinkAttrs)
  },

  // Typing `[[Title]]` by hand (ignoring the autocomplete) becomes a chip the
  // moment the closing `]]` lands, so it is saved as a link, not escaped text.
  addInputRules() {
    return [
      nodeInputRule({
        // No capture group: nodeInputRule would otherwise keep the brackets as text around the node.
        find: /!?\[\[[^[\]\n]+?\]\]$/,
        type: this.type,
        getAttributes: (match) => {
          const embed = match[0].startsWith('!')
          return { ...parseWikiInner(match[0].slice(embed ? 3 : 2, -2)), embed }
        },
      }),
    ]
  },

  markdownTokenizer: {
    name: 'wikiLink',
    level: 'inline',
    start: (src) => {
      const i = src.indexOf('[[')
      return i > 0 && src[i - 1] === '!' ? i - 1 : i
    },
    tokenize: (src) => {
      const m = WIKI_RE.exec(src)
      if (!m) return undefined
      return { type: 'wikiLink', raw: m[0], ...parseWikiInner(m[2]), embed: m[1] === '!' }
    },
  },

  parseMarkdown: (token, helpers) =>
    helpers.createNode('wikiLink', { target: token.target, heading: token.heading ?? null, display: token.display ?? null, embed: !!token.embed }),

  renderMarkdown: (node) => wikiLinkToMarkdown(node.attrs as WikiLinkAttrs),
})
