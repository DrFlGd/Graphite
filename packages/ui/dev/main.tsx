// Browser harness for the editor without Tauri: `pnpm --filter @graphite/ui dev`.
// Left: the editor. Right: the Markdown it would save.
import React, { useState } from 'react'
import ReactDOM from 'react-dom/client'
import { NoteEditor } from '../src'
import '../src/styles.css'

const SAMPLE = `# Garden Robot

A small wheeled robot that waters raised beds.

- [x] Decide on wheel size
- [ ] Order motors

See [[Chassis v3]] and [[Motor driver|the driver]].

\`\`\`python
WHEEL_DIAMETER_MM = 65
\`\`\`
`

const NOTES = ['Garden Robot', 'Chassis v3', 'Motor driver', 'Battery', 'Solar still']

function Harness() {
  const [md, setMd] = useState(SAMPLE)
  const [followed, setFollowed] = useState<string | null>(null)
  return (
    <>
      <div className="harness-editor">
        <NoteEditor
          docKey="harness"
          markdown={md}
          onChange={setMd}
          onFollowLink={(t) => setFollowed(t)}
          searchNotes={async (q) => NOTES.filter((n) => n.toLowerCase().startsWith(q.toLowerCase())).map((title) => ({ id: title, title, subtitle: `notes/${title}.md` }))}
          blockContext={{ pickImage: async () => 'attachments/example.png' }}
          autofocus
        />
      </div>
      <pre className="harness-md" data-testid="markdown">
        {followed ? `# followed: ${followed}\n\n` : ''}
        {md}
      </pre>
    </>
  )
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Harness />
  </React.StrictMode>,
)
