import { Code2, Workflow } from 'lucide-react'
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight'
import { NodeViewContent, NodeViewWrapper, ReactNodeViewRenderer, type NodeViewProps } from '@tiptap/react'
import { common, createLowlight } from 'lowlight'
import { useEffect, useState } from 'react'
import { MermaidPreview } from './mermaid'
import type { BlockDef } from './types'

const lowlight = createLowlight(common)

/** Languages offered in the picker; anything else typed in a fence is kept as-is. */
export const CODE_LANGUAGES = [
  '',
  'bash',
  'c',
  'cpp',
  'css',
  'diff',
  'go',
  'html',
  'ini',
  'java',
  'javascript',
  'json',
  'kotlin',
  'makefile',
  'markdown',
  'mermaid',
  'python',
  'rust',
  'shell',
  'sql',
  'swift',
  'toml',
  'typescript',
  'xml',
  'yaml',
]

/** Code block with an in-place language picker (C12). */
function CodeBlockView({ node, updateAttributes, editor, getPos }: NodeViewProps) {
  const language: string = node.attrs.language ?? ''
  const options = CODE_LANGUAGES.includes(language) ? CODE_LANGUAGES : [language, ...CODE_LANGUAGES]
  const isMermaid = language === 'mermaid'
  const inside = useCursorInside(editor, getPos, isMermaid)
  // A diagram shows its picture; the code appears while the cursor is in it, or when it is empty or broken.
  const [broken, setBroken] = useState(false)
  const [opening, setOpening] = useState(false)
  const showCode = !isMermaid || inside || broken || opening || !node.textContent.trim()
  return (
    <NodeViewWrapper className={`gr-code-block ${isMermaid ? 'gr-code-block--diagram' : ''} ${showCode ? '' : 'is-code-hidden'}`}>
      <select
        className="gr-code-block__lang"
        contentEditable={false}
        value={language}
        disabled={!editor.isEditable}
        onChange={(e) => updateAttributes({ language: e.target.value || null })}
        aria-label="Code language"
      >
        {options.map((l) => (
          <option key={l} value={l}>
            {l || 'plain text'}
          </option>
        ))}
      </select>
      {/* The picture sits above the code: text after the editable code confuses the cursor at its end. */}
      {isMermaid && (
        <MermaidPreview
          code={node.textContent}
          onBroken={setBroken}
          onEdit={
            editor.isEditable
              ? () => {
                  // Show the code first: a selection put into hidden text lands somewhere else.
                  setOpening(true)
                  window.requestAnimationFrame(() => {
                    const pos = getPos()
                    if (typeof pos === 'number') editor.chain().focus().setTextSelection(pos + node.nodeSize - 1).run()
                    setOpening(false)
                  })
                }
              : undefined
          }
        />
      )}
      <pre>
        <NodeViewContent as={'code' as never} />
      </pre>
    </NodeViewWrapper>
  )
}

/** Whether the selection sits inside the node at `getPos()` (only tracked when `enabled`). */
function useCursorInside(editor: NodeViewProps['editor'], getPos: NodeViewProps['getPos'], enabled: boolean): boolean {
  const [inside, setInside] = useState(false)
  useEffect(() => {
    if (!enabled) return
    const check = () => {
      const pos = getPos()
      if (typeof pos !== 'number') return
      // Read the size from the current document: this runs before React sees the edited node.
      const size = editor.state.doc.nodeAt(pos)?.nodeSize ?? 0
      const { from, to } = editor.state.selection
      setInside(editor.isFocused && from > pos && to < pos + size)
    }
    check()
    editor.on('selectionUpdate', check)
    editor.on('focus', check)
    editor.on('blur', check)
    return () => {
      editor.off('selectionUpdate', check)
      editor.off('focus', check)
      editor.off('blur', check)
    }
  }, [editor, getPos, enabled])
  return inside
}

const CodeBlock = CodeBlockLowlight.extend({
  addNodeView() {
    return ReactNodeViewRenderer(CodeBlockView)
  },
}).configure({ lowlight, defaultLanguage: null })

export const codeBlock: BlockDef = {
  id: 'codeBlock',
  label: 'Code block',
  description: 'Monospaced block with syntax highlighting',
  icon: Code2,
  group: 'text',
  keywords: ['code', 'snippet', 'pre', 'fence'],
  shortcut: 'Ctrl+Alt+C',
  extensions: [CodeBlock],
  insert: (editor) => editor.chain().focus().toggleCodeBlock().run(),
  isActive: (editor) => editor.isActive('codeBlock'),
}

export const mermaidDiagram: BlockDef = {
  id: 'mermaid',
  label: 'Diagram from text',
  description: 'A diagram written as text (Mermaid): flowcharts, sequences, timelines, Gantt',
  icon: Workflow,
  group: 'media',
  keywords: ['mermaid', 'diagram', 'flowchart', 'chart', 'sequence', 'gantt', 'timeline', 'graph'],
  insert: (editor) =>
    editor
      .chain()
      .focus()
      .insertContent({ type: 'codeBlock', attrs: { language: 'mermaid' }, content: [{ type: 'text', text: 'flowchart LR\n  Idea --> Plan --> Build' }] })
      .run(),
  toolbar: false,
}
