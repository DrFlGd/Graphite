import { describe, expect, it } from 'vitest'
import { Editor } from '@tiptap/core'
import { buildMarkdownExtensions } from './extensions'
import { insertAttachments } from './attachments'

function editorWith(markdown: string): Editor {
  return new Editor({ extensions: buildMarkdownExtensions(), content: markdown, contentType: 'markdown' })
}

describe('insertAttachments', () => {
  it('inserts images as image blocks and other files as links', () => {
    const editor = editorWith('Hello')
    insertAttachments(editor, [
      { src: 'attachments/shot.png', name: 'shot.png' },
      { src: 'attachments/spec.pdf', name: 'spec.pdf' },
    ])
    const md = editor.getMarkdown()
    expect(md).toContain('![shot.png](attachments/shot.png)')
    expect(md).toContain('[spec.pdf](attachments/spec.pdf)')
    editor.destroy()
  })

  it('adds after a selected image instead of replacing it', () => {
    const editor = editorWith('Hello')
    insertAttachments(editor, [{ src: 'attachments/a.png', name: 'a.png' }])
    insertAttachments(editor, [{ src: 'attachments/b.pdf', name: 'b.pdf' }])
    const md = editor.getMarkdown()
    expect(md).toContain('![a.png](attachments/a.png)')
    expect(md).toContain('[b.pdf](attachments/b.pdf)')
    editor.destroy()
  })

  it('does nothing for an empty list', () => {
    const editor = editorWith('Hello')
    expect(insertAttachments(editor, [])).toBe(false)
    expect(editor.getMarkdown()).toBe('Hello')
    editor.destroy()
  })
})
