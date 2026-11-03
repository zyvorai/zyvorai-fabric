// Shared by baseline-shots.mjs and a11y-check.mjs: serves dist/ (SPA fallback), answers the mock API with
// the same rules as mock-api.ts, and opens console pages signed in, in a chosen theme and viewport.
// Needs `npx vite build` first; no dev server (slow to compile) is involved.
import puppeteer from 'puppeteer'
import { readFileSync, existsSync, statSync } from 'node:fs'
import { createServer } from 'node:http'
import { extname, join } from 'node:path'

export const PAGES = ['', 'vms', 'create', 'logs', 'network', 'storage', 'templates', 'agents', 'sessions', 'merion',
  'schedules', 'backups', 'settings', 'alerts', 'analytics', 'audit', 'system', 'security-dashboard',
  'migrations', 'access-control']
export const SIZES = { desktop: [1440, 900], phone: [390, 844] }

const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.json': 'application/json', '.woff2': 'font/woff2', '.png': 'image/png' }

export function serveDist(port = 3111) {
  return createServer((req, res) => {
    let file = join('dist', new URL(req.url, 'http://x').pathname)
    if (!existsSync(file) || statSync(file).isDirectory()) file = join('dist', 'index.html')
    res.setHeader('Content-Type', TYPES[extname(file)] ?? 'application/octet-stream')
    res.end(readFileSync(file))
  }).listen(port)
}

export function launch() {
  // PUPPETEER_EXECUTABLE_PATH is honoured by puppeteer itself, so a machine with its own Chrome needs no download.
  return puppeteer.launch({ args: process.env.CI ? ['--no-sandbox'] : [] })
}

/** A page with the mock API, a stored sign-in and the theme set before the app loads. */
export async function newConsolePage(browser, theme, [width, height]) {
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
  return page
}

export async function openPage(page, name, port = 3111) {
  await page.goto(`http://localhost:${port}/app/${name}`, { waitUntil: 'load', timeout: 30000 })
  // the stagger/page-in animations take about a second to settle
  await new Promise((r) => setTimeout(r, 1800))
}
