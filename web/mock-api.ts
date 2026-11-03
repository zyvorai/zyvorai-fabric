// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

// Dev-only mock API for `npm run dev:mock`: lets the console render every
// page with no backend, so layout work can be checked and screenshotted.
// Lists (paths ending in "s") answer `[]`, everything else `{}`; sign-in
// and /auth/me answer a fixed admin.
import type { Plugin } from 'vite'

const USER = { id: 'mock-admin', username: 'admin', role: 'admin', tenant: null }

export function mockApi(): Plugin {
  return {
    name: 'zyvor-mock-api',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use('/api', (req, res) => {
        const path = ((req as { url?: string }).url ?? '').split('?')[0].replace(/\/+$/, '')
        let body: unknown = path.endsWith('s') ? [] : {}
        if (path === '/auth/me') body = USER
        else if (path === '/auth/login') body = { token: 'mock-token', user_id: USER.id, role: USER.role, username: USER.username, tenant: null }
        res.setHeader('Content-Type', 'application/json')
        res.end(JSON.stringify(body))
      })
    },
  }
}
