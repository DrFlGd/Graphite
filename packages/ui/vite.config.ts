import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Dev harness only (see dev/); the package itself is consumed as source.
export default defineConfig({
  plugins: [react()],
  server: { port: 1430, strictPort: true, open: '/dev/index.html' },
})
