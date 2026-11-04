import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Dev: /api/* is proxied to the Rust backend, so the SPA never hard-codes a host.
// Prod/TMA: set VITE_API_URL to the VPS URL (or serve the build from the Rust binary itself).
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    host: true,
    proxy: {
      '/api': {
        target: 'http://localhost:3000',
        changeOrigin: true,
      },
    },
  },
  build: { outDir: 'dist', sourcemap: false },
})
