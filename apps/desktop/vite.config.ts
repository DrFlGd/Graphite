import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import { createRequire } from 'node:module'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * Serve Excalidraw's fonts from the app (`/excalidraw/fonts/…`) and ship
 * them in the build, so drawings render offline and within the CSP. The
 * Chinese/Japanese/Korean fallback (Xiaolai, 13 MB) is left out; those
 * characters use the system font.
 */
function excalidrawFonts(): Plugin {
  const require = createRequire(import.meta.url)
  // Resolves to dist/prod/index.js; the fonts sit beside it.
  const entry = require.resolve('@excalidraw/excalidraw', { paths: [path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../packages/ui')] })
  const root = path.join(path.dirname(entry), 'fonts')
  const skip = new Set(['Xiaolai'])
  const files = (): string[] =>
    fs
      .readdirSync(root)
      .filter((family) => !skip.has(family))
      .flatMap((family) => fs.readdirSync(path.join(root, family)).map((f) => `${family}/${f}`))
  return {
    name: 'graphite-excalidraw-fonts',
    configureServer(server) {
      server.middlewares.use('/excalidraw/fonts/', (req, res, next) => {
        const rel = decodeURIComponent((req.url ?? '').split('?')[0]).replace(/^\/+/, '')
        if (rel.includes('..') || skip.has(rel.split('/')[0])) return next()
        const file = path.join(root, rel)
        if (!fs.existsSync(file)) return next()
        res.setHeader('Content-Type', file.endsWith('.woff2') ? 'font/woff2' : 'application/octet-stream')
        fs.createReadStream(file).pipe(res)
      })
    },
    generateBundle() {
      for (const rel of files()) this.emitFile({ type: 'asset', fileName: `excalidraw/fonts/${rel}`, source: fs.readFileSync(path.join(root, rel)) })
    },
  }
}

const host = process.env.TAURI_DEV_HOST

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), excalidrawFonts()],
  // Excalidraw reads this at load time.
  define: { 'process.env.IS_PREACT': JSON.stringify('false') },
  // Vite options tailored for Tauri development.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: {
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    minify: !process.env.TAURI_ENV_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
})
