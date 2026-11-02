/**
 * Excalidraw is large, so it loads the first time a drawing is shown.
 * Fonts come from the app itself (`/excalidraw/fonts/…`, copied there by
 * the host's build) rather than a CDN, so drawings work offline.
 */
type ExcalidrawModule = typeof import('@excalidraw/excalidraw')

declare global {
  interface Window {
    EXCALIDRAW_ASSET_PATH?: string | string[]
  }
}

let loading: Promise<ExcalidrawModule> | null = null

export function loadExcalidraw(): Promise<ExcalidrawModule> {
  if (!loading) {
    if (typeof window !== 'undefined' && window.EXCALIDRAW_ASSET_PATH === undefined) window.EXCALIDRAW_ASSET_PATH = '/excalidraw/'
    loading = Promise.all([import('@excalidraw/excalidraw'), import('./excalidrawCss')]).then(([m]) => m)
    loading.catch(() => {
      loading = null
    })
  }
  return loading
}

export function isDarkTheme(): boolean {
  const attr = document.documentElement.getAttribute('data-theme')
  if (attr) return attr === 'dark'
  return typeof window.matchMedia === 'function' && window.matchMedia('(prefers-color-scheme: dark)').matches
}

/** Sent on `window` after a drawing is saved, so its embeds redraw. */
export const DRAWING_SAVED = 'graphite:drawing-saved'

export function announceDrawingSaved(src: string) {
  window.dispatchEvent(new CustomEvent(DRAWING_SAVED, { detail: src }))
}

/** Whether two srcs name the same file, one percent-encoded and one not. */
export function sameSrc(a: string, b: string): boolean {
  const plain = (s: string) => {
    try {
      return decodeURIComponent(s).replace(/^\.\//, '')
    } catch {
      return s
    }
  }
  return plain(a) === plain(b)
}
