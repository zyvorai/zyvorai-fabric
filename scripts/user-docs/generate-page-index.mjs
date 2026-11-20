#!/usr/bin/env node
// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { readFileSync, writeFileSync, existsSync, readdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const OUT = resolve(ROOT, 'docs/user/PAGE_INDEX.md')
const GUIDES = resolve(ROOT, 'docs/user/pages')
const { routes } = JSON.parse(readFileSync(resolve(ROOT, 'scripts/user-docs/routes.json'), 'utf8'))
const purposes = JSON.parse(readFileSync(resolve(ROOT, 'scripts/user-docs/page-purposes.json'), 'utf8'))
const PRODUCT = process.env.USER_DOCS_PRODUCT || 'Zyvor Fabric'

function discoverGuides(dir) {
  const map = new Map()
  if (!existsSync(dir)) return map
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue
    for (const file of readdirSync(join(dir, entry.name))) {
      if (!file.endsWith('.md') || file === 'README.md') continue
      map.set(file.replace(/\.md$/, ''), `pages/${entry.name}/${file}`)
    }
  }
  return map
}

function slug(path) {
  return path.replace(/^\//, '').replace(/\//g, '-').replace(/\?.*/, '') || 'home'
}

// The console router mounts everything under /app (see web/src/App.tsx) --
// routes.json stores paths relative to that mount point, so they need the
// prefix added back for a user-facing URL. A couple of legacy paths
// (like /login) redirect to a different real destination; consoleUrl()
// follows those so the doc shows where the browser actually ends up.
const REDIRECTS = { '/login': '/sign-in' }
function consoleUrl(path) {
  const resolved = REDIRECTS[path] || path
  return resolved === '/' ? '/app' : `/app${resolved}`
}

const MARKETING_ROUTES = [
  { label: 'Home', path: '/', purpose: 'Public marketing home' },
  { label: 'Product', path: '/product', purpose: 'Product story' },
  { label: 'Platform', path: '/platform', purpose: 'Interfaces (Web, CLI, Operator, Terraform)' },
  { label: 'Security', path: '/security', purpose: 'Security story' },
  { label: 'Sign in', path: '/sign-in', purpose: 'Console authentication (`/login` redirects here)' },
]

const guides = discoverGuides(GUIDES)
const byCat = new Map()
for (const r of routes) {
  if (!byCat.has(r.category)) byCat.set(r.category, [])
  byCat.get(r.category).push(r)
}

const lines = [
  `# ${PRODUCT} — Complete page index`,
  '',
  `Marketing: ${MARKETING_ROUTES.map((m) => `\`${m.path}\``).join(', ')}.`,
  '',
  'Console routes under `/app` — every primary navigable ops route.',
  '',
  `_Generated: ${new Date().toISOString().slice(0, 10)} · ${routes.length} routes_`,
  '',
  'Regenerate: `node scripts/user-docs/generate-page-index.mjs`',
  '',
  '## Marketing & auth',
  '',
  '| Page | Route | Purpose |',
  '|------|-------|---------|',
  ...MARKETING_ROUTES.map((m) => `| ${m.label} | \`${m.path}\` | ${m.purpose} |`),
  '',
]

for (const [cat, list] of byCat) {
  lines.push(`## ${cat}`, '', '| Page | Route | Purpose | Guide |', '|------|-------|---------|-------|')
  for (const it of list) {
    const purpose = (purposes[it.path] || '').replace(/\\/g, '\\\\').replace(/\|/g, '\\|')
    const g = guides.get(slug(it.path))
    lines.push(`| ${it.label} | \`${consoleUrl(it.path)}\` | ${purpose} | ${g ? `[Open](${g})` : '—'} |`)
  }
  lines.push('')
}

lines.push('## Related', '', '- [User docs home](README.md)', '- [Page-by-page guides](pages/README.md)', '')
writeFileSync(OUT, lines.join('\n'))
console.log(`Wrote ${OUT}`)
