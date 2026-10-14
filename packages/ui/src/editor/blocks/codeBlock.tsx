import { Code2 } from 'lucide-react'
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight'
import { NodeViewContent, NodeViewWrapper, ReactNodeViewRenderer, type NodeViewProps } from '@tiptap/react'
import { common, createLowlight } from 'lowlight'
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
function CodeBlockView({ node, updateAttributes, editor }: NodeViewProps) {
  const language: string = node.attrs.language ?? ''
  const options = CODE_LANGUAGES.includes(language) ? CODE_LANGUAGES : [language, ...CODE_LANGUAGES]
  return (
    <NodeViewWrapper className="gr-code-block">
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
      <pre>
        <NodeViewContent as={'code' as never} />
      </pre>
    </NodeViewWrapper>
  )
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
