// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Default target predates top-level await, which @novnc/novnc's ESM
  // build uses -- es2022 is baseline-supported by every browser this
  // app already requires (native ES modules, CSS nesting, etc).
  oxc: {
    target: 'es2022',
  },
  optimizeDeps: {
    esbuildOptions: {
      target: 'es2022',
    },
  },
  build: {
    target: 'es2022',
  },
  test: {
    coverage: {
      provider: 'v8',
      reporter: ['text', 'lcov', 'html'],
      reportsDirectory: './coverage',
    },
  },
  server: {
    port: 3000,
    // Keep marketing copy is shared from ../docs/merion/marketing.json.
    fs: { allow: ['..'] },
    proxy: {
      '/api': {
        target: 'http://localhost:9095',
        changeOrigin: true,
      },
      '/ws': {
        target: 'ws://localhost:9095',
        ws: true,
      },
    },
  },
})
