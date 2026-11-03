// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

// Dev-only mock API for `npm run dev:mock`: lets the console render every
// page with no backend, so layout work can be checked and screenshotted.
// The pages in `mock-data.mjs` (VMs, sessions, a cockpit with a pending and a failed changeset, a budget near its cap)
// answer with seed data; any other list (path ending in "s") answers `[]` and anything else `{}`; sign-in and
// /auth/me answer a fixed admin.
import type { Plugin } from 'vite'
// @ts-expect-error -- plain .mjs shared with the screenshot harness; it has no type declarations
import { mockBody, MOCK_USER as USER } from './mock-data.mjs'

export function mockApi(): Plugin {
  return {
    name: 'zyvor-mock-api',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use('/api', (req, res) => {
        const path = ((req as { url?: string }).url ?? '').split('?')[0].replace(/\/+$/, '')
        let body: unknown = mockBody(path) ?? (path.endsWith('s') ? [] : {})
        if (path === '/auth/login') body = { token: 'mock-token', user_id: USER.id, role: USER.role, username: USER.username, tenant: null }
        res.setHeader('Content-Type', 'application/json')
        res.end(JSON.stringify(body))
      })
    },
  }
}
