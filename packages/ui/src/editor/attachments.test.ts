import { describe, expect, it } from 'vitest'
import { Editor } from '@tiptap/core'
import { buildMarkdownExtensions } from './extensions'
import { insertAttachments } from './attachments'

function editorWith(markdown: string): Editor {
  return new Editor({ extensions: buildMarkdownExtensions(), content: markdown, contentType: 'markdown' })
}

describe('insertAttachments', () => {
  it('inserts images and media as embeds and other files as links', () => {
    const editor = editorWith('Hello')
    insertAttachments(editor, [
      { src: 'attachments/shot.png', name: 'shot.png' },
      { src: 'attachments/spec.pdf', name: 'spec.pdf' },
      { src: 'attachments/data.csv', name: 'data.csv' },
    ])
    const md = editor.getMarkdown()
    expect(md).toContain('![shot.png](attachments/shot.png)')
    expect(md).toContain('![spec.pdf](attachments/spec.pdf)')
    expect(md).toContain('[data.csv](attachments/data.csv)')
    editor.destroy()
  })

  it('adds after a selected image instead of replacing it', () => {
    const editor = editorWith('Hello')
    insertAttachments(editor, [{ src: 'attachments/a.png', name: 'a.png' }])
    insertAttachments(editor, [{ src: 'attachments/b.pdf', name: 'b.pdf' }])
    const md = editor.getMarkdown()
    expect(md).toContain('![a.png](attachments/a.png)')
    expect(md).toContain('![b.pdf](attachments/b.pdf)')
    editor.destroy()
  })

  it('percent-encodes spaces and parentheses in paths', () => {
    const editor = editorWith('Hello')
    insertAttachments(editor, [
      { src: 'attachments/Garden Robot/plan (v2).png', name: 'plan' },
      { src: 'attachments/Garden Robot/notes.txt', name: 'notes' },
    ])
    const md = editor.getMarkdown()
    expect(md).toContain('![plan](attachments/Garden%20Robot/plan%20%28v2%29.png)')
    expect(md).toContain('[notes](attachments/Garden%20Robot/notes.txt)')
    editor.destroy()
  })

  it('does nothing for an empty list', () => {
    const editor = editorWith('Hello')
    expect(insertAttachments(editor, [])).toBe(false)
    expect(editor.getMarkdown()).toBe('Hello')
    editor.destroy()
  })
})
