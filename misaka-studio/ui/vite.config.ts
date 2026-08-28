import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// The runtime the dev server proxies to. `misaka-studiod` defaults to 1338; override with
// MISAKA_STUDIO_PORT when running two at once.
//
// `process` is read through `globalThis` because this file is typechecked with the browser lib
// set the app uses — pulling in @types/node to read one environment variable would put Node's
// globals in scope for every UI file, where they do not exist at runtime.
const port = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env?.MISAKA_STUDIO_PORT ?? 1338
const runtime = `http://127.0.0.1:${port}`

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Relative asset URLs: the same bundle is served by the runtime at `/` and loaded by the Tauri
  // shell from the filesystem, and an absolute `/assets/...` path resolves in only one of those.
  base: './',
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      // Both API surfaces, so the dev UI is a client of exactly the API a packaged build uses —
      // no dev-only endpoints, no CORS config that only exists in development.
      '/api': { target: runtime, changeOrigin: true },
      '/v1': { target: runtime, changeOrigin: true },
    },
  },
  build: {
    outDir: 'dist',
    // Sourcemaps ship: this is a local desktop app, the bundle is on the user's own disk, and a
    // stack trace someone can read is worth more than the kilobytes.
    sourcemap: true,
  },
})
