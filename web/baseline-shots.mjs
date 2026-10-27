// Baseline screenshots of the busiest console pages (ux-plan step 0).
// Needs `npx vite build` first. Serves dist/ itself (SPA fallback) and answers the mock API with
// the same rules as mock-api.ts, so no dev server (slow to compile) is needed. Run: node baseline-shots.mjs <outdir>
// Uses a globally installed puppeteer: NODE_PATH=$(npm root -g)
import { createRequire } from 'node:module'
import { mkdirSync, readFileSync, existsSync, statSync } from 'node:fs'
import { createServer } from 'node:http'
import { extname, join } from 'node:path'
const require = createRequire(process.env.NODE_PATH ? process.env.NODE_PATH + '/x.js' : import.meta.url)
const puppeteer = require('puppeteer')

const PAGES = ['', 'vms', 'create', 'logs', 'network', 'storage', 'templates', 'agents', 'sessions', 'merion',
  'schedules', 'backups', 'settings', 'alerts', 'analytics', 'audit', 'system', 'security-dashboard',
  'migrations', 'access-control']
const SIZES = { desktop: [1440, 900], phone: [390, 844] }
const out = process.argv[2]
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.json': 'application/json', '.woff2': 'font/woff2', '.png': 'image/png' }
const server = createServer((req, res) => {
  let file = join('dist', new URL(req.url, 'http://x').pathname)
  if (!existsSync(file) || statSync(file).isDirectory()) file = join('dist', 'index.html')
  res.setHeader('Content-Type', TYPES[extname(file)] ?? 'application/octet-stream')
  res.end(readFileSync(file))
}).listen(3111)
mkdirSync(out, { recursive: true })
const browser = await puppeteer.launch()
for (const theme of ['light', 'dark']) {
  for (const [size, [width, height]] of Object.entries(SIZES)) {
    const page = await browser.newPage()
    await page.setViewport({ width, height })
    await page.setRequestInterception(true)
    page.on('request', (req) => {
      const path = new URL(req.url()).pathname
      if (!path.startsWith('/api/')) return req.continue()
      const p = path.slice(4).replace(/\/+$/, '')
      let body = p.endsWith('s') ? [] : {}
      if (p === '/auth/me') body = { id: 'mock-admin', username: 'admin', role: 'admin', tenant: null }
      req.respond({ status: 200, contentType: 'application/json', body: JSON.stringify(body) })
    })
    await page.evaluateOnNewDocument((t) => {
      localStorage.setItem('zyvor_fabric_token', 'mock-token')
      localStorage.setItem('zyvor-fabricd-theme', t)
    }, theme)
    for (const name of PAGES) {
      await page.goto(`http://localhost:3111/app/${name}`, { waitUntil: 'load', timeout: 30000 })
      await new Promise((r) => setTimeout(r, 1800))
      const sideways = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)
      await page.screenshot({ path: `${out}/${name || 'dashboard'}-${theme}-${size}.png` })
      if (sideways) console.log('SIDEWAYS', size, theme, name || 'dashboard')
    }
    await page.close()
  }
}
await browser.close()
server.close()
