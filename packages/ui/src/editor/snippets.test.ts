import { describe, expect, it } from 'vitest'
import { Editor } from '@tiptap/core'
import { buildMarkdownExtensions } from './extensions'
import { insertMarkdown, matchSnippets, selectionMarkdown } from './snippets'

function editorWith(markdown: string): Editor {
  return new Editor({ extensions: buildMarkdownExtensions(), content: markdown, contentType: 'markdown' })
}

describe('snippets', () => {
  it('puts a one-line snippet into the line and several blocks in as blocks', () => {
    const editor = editorWith('Signed: ')
    editor.commands.setTextSelection(editor.state.doc.content.size - 1)
    insertMarkdown(editor, '**Jerred**\n')
    expect(editor.getMarkdown().trim()).toBe('Signed: **Jerred**')

    const empty = editorWith('First\n\n')
    empty.commands.setTextSelection(empty.state.doc.content.size - 1)
    insertMarkdown(empty, '## Attendees\n\n- [ ] Agenda sent')
    const md = empty.getMarkdown()
    expect(md).toContain('## Attendees')
    expect(md).toContain('- [ ] Agenda sent')
    editor.destroy()
    empty.destroy()
  })

  it('reads the selection back as Markdown', () => {
    const editor = editorWith('# Notes\n\nSome **bold** text\n\n- one\n- two')
    expect(selectionMarkdown(editor)).toBe('')
    editor.commands.selectAll()
    const md = selectionMarkdown(editor)
    expect(md).toContain('Some **bold** text')
    expect(md).toContain('- one')
    // Part of one paragraph: the text with its marks.
    let from = 0
    editor.state.doc.descendants((n, pos) => {
      if (n.isText && n.text === 'bold') from = pos
    })
    editor.commands.setTextSelection({ from, to: from + 4 })
    expect(selectionMarkdown(editor)).toBe('**bold**')
    editor.destroy()
  })

  it('matches snippets by name first, then by description', () => {
    const list = [
      { id: '1', name: 'Meeting header', description: 'Date and attendees' },
      { id: '2', name: 'Sign-off', description: 'Thanks and name' },
      { id: '3', name: 'Attendees', description: '' },
    ]
    expect(matchSnippets(list, 'att').map((s) => s.id)).toEqual(['3', '1'])
    expect(matchSnippets(list, 'snip')).toHaveLength(3)
    expect(matchSnippets(list, 'zzz')).toEqual([])
  })
})
