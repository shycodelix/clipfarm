import { resolve } from 'node:path'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

/**
 * Renderer-only dev server. Electron is the shipping target, but the whole UI is
 * an ordinary web app, so `npm run dev:web` is the fastest way to iterate on it in
 * a browser. In this mode the renderer falls back to the demo bridge (see
 * src/renderer/src/lib/bridge.ts).
 */
export default defineConfig({
  root: resolve('src/renderer'),
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@shared': resolve('src/shared'),
      '@': resolve('src/renderer/src')
    }
  },
  server: {
    host: '0.0.0.0',
    port: Number(process.env.PORT ?? 3000),
    strictPort: false
  },
  build: {
    outDir: resolve('out/renderer'),
    emptyOutDir: true
  }
})
