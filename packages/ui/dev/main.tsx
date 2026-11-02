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

> [!tip] Water first
> Check the ==pump== before the motors.

> [!warning]- Mains voltage
> Unplug it before opening the case.

Wheel speed is $v = \\omega r$, so at 2 rev/s it moves about 0.4 m/s.[^1]

$$
\\int_0^{T} P(t)\\,dt \\le E_{battery}
$$

\`\`\`mermaid
flowchart LR
  Sensor --> Controller --> Pump
\`\`\`

Power details: [[Battery#Sizing]] and the whole [[Battery]] note.

![[Battery#^cells]]

A paragraph to link to. ^para1

[^1]: Measured on the bench, 2026-10-12.
`

const VAULT: Record<string, string> = {
  battery: '# Battery\n\nA 12 V pack.\n\n## Sizing\n\nAbout $E = V \\cdot Ah$.\n\n> [!tip] Keep it cool\n> Below 40 °C.\n\n- 4 cells in series ^cells\n  - 18650\n',
}

function section(md: string, anchor: string | null): string | null {
  if (!anchor) return md
  const lines = md.split('\n')
  if (anchor.startsWith('^')) {
    const i = lines.findIndex((l) => l.trimEnd().endsWith(' ' + anchor))
    if (i < 0) return null
    let end = i + 1
    while (end < lines.length && /^\s+\S/.test(lines[end])) end++
    return lines.slice(i, end).join('\n').replace(' ' + anchor, '')
  }
  const i = lines.findIndex((l) => /^#+\s/.test(l) && l.replace(/^#+\s*/, '').toLowerCase() === anchor.toLowerCase())
  if (i < 0) return null
  const level = /^#+/.exec(lines[i])![0].length
  let end = i + 1
  while (end < lines.length && !(/^#+\s/.test(lines[end]) && /^#+/.exec(lines[end])![0].length <= level)) end++
  return lines.slice(i, end).join('\n')
}

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
          searchNotes={async (q) => NOTES.filter((n) => n.toLowerCase().startsWith(q.toLowerCase())).map((title) => ({ id: title, title, subtitle: `notes/${title}.md` }))}
          blockContext={{
            pickImage: async () => 'attachments/example.png',
            loadSection: async (target, anchor) => {
              const md = VAULT[target.toLowerCase()]
              return md === undefined ? null : { title: target, markdown: section(md, anchor) }
            },
            followLink: (t, a) => setFollowed(a ? `${t}#${a}` : t),
          }}
          onFollowLink={(t, a) => setFollowed(a ? `${t}#${a}` : t)}
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
