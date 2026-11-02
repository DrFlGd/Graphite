import { useEffect, useRef, useState } from 'react'

type MermaidApi = typeof import('mermaid').default

let loading: Promise<MermaidApi> | null = null
let theme: string | null = null
let seq = 0

function isDark(): boolean {
  const attr = document.documentElement.getAttribute('data-theme')
  if (attr) return attr === 'dark'
  return typeof window.matchMedia === 'function' && window.matchMedia('(prefers-color-scheme: dark)').matches
}

/** Mermaid is large, so it loads the first time a diagram is shown. */
async function mermaid(): Promise<MermaidApi> {
  loading ??= import('mermaid').then((m) => m.default)
  const api = await loading
  const want = isDark() ? 'dark' : 'default'
  if (theme !== want) {
    api.initialize({ startOnLoad: false, securityLevel: 'strict', theme: want, fontFamily: 'inherit' })
    theme = want
  }
  return api
}

/** Render Mermaid source to SVG markup, or throw with Mermaid's message. */
export async function renderMermaid(code: string): Promise<string> {
  const api = await mermaid()
  const { svg } = await api.render(`gr-mermaid-${++seq}`, code)
  return svg
}

/** The picture for a ```mermaid block; clicking it (when editable) puts the cursor in the code. */
export function MermaidPreview({ code, onBroken, onEdit }: { code: string; onBroken?: (broken: boolean) => void; onEdit?: () => void }) {
  const [svg, setSvg] = useState<string>('')
  const [error, setError] = useState<string | null>(null)
  const broken = useRef(onBroken)
  broken.current = onBroken

  useEffect(() => {
    if (!code.trim()) {
      setSvg('')
      setError(null)
      return
    }
    let cancelled = false
    const t = window.setTimeout(() => {
      renderMermaid(code)
        .then((s) => {
          if (cancelled) return
          setSvg(s)
          setError(null)
          broken.current?.(false)
        })
        .catch((e: unknown) => {
          if (cancelled) return
          setError(e instanceof Error ? e.message : String(e))
          broken.current?.(true)
        })
    }, 250)
    return () => {
      cancelled = true
      window.clearTimeout(t)
    }
  }, [code])

  if (!code.trim()) return null
  return (
    <div className="gr-diagram" contentEditable={false}>
      {error ? (
        <p className="gr-diagram__error">Diagram has an error: {error.split('\n')[0]}</p>
      ) : (
        <div
          className={`gr-diagram__svg ${onEdit ? 'is-clickable' : ''}`}
          title={onEdit ? 'Click to edit the diagram' : undefined}
          onClick={onEdit}
          // Mermaid's own output, rendered with securityLevel "strict" (no scripts, no HTML labels).
          dangerouslySetInnerHTML={{ __html: svg }}
        />
      )}
    </div>
  )
}
