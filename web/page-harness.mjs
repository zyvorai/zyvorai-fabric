// Shared by baseline-shots.mjs and a11y-check.mjs: serves dist/ (SPA fallback), answers the mock API with
// the same rules as mock-api.ts, and opens console pages signed in, in a chosen theme and viewport.
// Needs `npx vite build` first; no dev server (slow to compile) is involved.
import puppeteer from 'puppeteer'
import { readFileSync, existsSync, statSync } from 'node:fs'
import { createServer } from 'node:http'
import { extname, join } from 'node:path'
import { mockBody, SEEDED_SESSION_ID } from './mock-data.mjs'

/** Pages the mock feeds with data (see mock-data.mjs): they render populated, so tables, cards and charts are checked too. */
export const SEEDED_PAGES = ['', 'vms', 'create', 'logs', 'network', 'storage', 'templates', 'agents', 'sessions', 'merion',
  'schedules', 'settings', 'alerts', 'security-dashboard', 'access-control',
  'backups', 'analytics', 'audit', 'system', 'migrations',
  'certificates', 'compliance', 'drs', 'federation', 'lifecycle', 'migration-report', 'replication', 'site-recovery', `merion/${SEEDED_SESSION_ID}`, 'merion/history', 'vms/web-01', 'vms/staging-api', 'vms/ci-runner', 'ui-kit']
/** Pages that render fine against the mock's empty answers but have no seed data yet. They are checked as empty states (structure,
    labels, contrast of what is drawn), not populated. Seed one in mock-data.mjs and move it to SEEDED_PAGES to check it with data. */
export const EMPTY_STATE_PAGES = [
  'ai', 'autoscale', 'backup-scheduler', 'batch-import', 'batch-migration', 'bulk-operations', 'capacity-planning', 'container-groups',
  'containers', 'content-library', 'cost-estimator', 'datacenters', 'debug', 'disk-converter', 'disk-images', 'distributed-storage',
  'download-disk', 'edge-dataplane', 'encryption', 'event-stream', 'explain', 'fault-tolerance', 'favorites', 'iso-images', 'job-monitor',
  'kernel', 'live-metrics', 'manifest-builder', 'migration-history', 'migration-readiness', 'migration-templates', 'migration-wizard',
  'network-security', 'network-topology', 'notification-center', 'notifications', 'pipeline', 'playground', 'plugins', 'processes',
  'profiles', 'quotas', 'resource-optimizer', 'resource-pools', 'service-map', 'snapshot-manager', 'snapshots', 'storage-manager',
  'storage-pools', 'system-health', 'timeline', 'upload-disk', 'vm-browser', 'vm-compare', 'vm-healthcheck', 'vm-pools', 'vm-wizard',
  'webhooks', 'zones',
]
export const PAGES = [...SEEDED_PAGES, ...EMPTY_STATE_PAGES]
/** Pages whose mock data is not seeded yet: with the mock's empty `{}` they throw and show the error boundary, so axe has
    nothing real to check, and `openPage` fails them. Seed each one (see mock-data.mjs) and move it into PAGES. */
export const UNSEEDED_PAGES = []
/** Pages whose tabs hide whole screens: the gate clicks each tab (a button with exactly this text) and checks again, because axe only
    sees the tab that is open when the page loads. A name with no matching button on the page is skipped. */
export const PAGE_TABS = {
  network: ['Bridges', 'Bonds', 'VLANs', 'Macvtap', 'Tap', 'Interfaces', 'Link Files', 'Port Forwards', 'VXLAN', 'SR-IOV', 'Floating IPs', 'Status'],
  system: ['CPU Topology', 'NUMA Topology', 'Memory & Hugepages', 'Optimization'],
  certificates: ['Dashboard', 'CAs', 'Certificates', 'Requests', 'Attestation', 'Security Baselines'],
  drs: ['balance', 'recommendations', 'rules', 'placement'],
  lifecycle: ['baselines', 'Compliance Scans', 'remediation', 'Rolling Updates'],
  replication: ['dashboard', 'sites', 'Replications', 'RPO Violations'],
  'site-recovery': ['DR Dashboard', 'Recovery Plans', 'Execution History'],
  ai: ['Models', 'Deployments', 'Endpoints', 'API keys', 'Nodes', 'Host GPUs'],
  'content-library': ['Libraries', 'Item Browser', 'Guest Customization', 'Host Profiles'],
  'distributed-storage': ['pools', 'policies', 'migrations', 'Datastore Clusters'],
  'edge-dataplane': ['Health', 'Services', 'Groups', 'CNP', 'Endpoints', 'Identities', 'Observe', 'Packet flow', 'Ipcache'],
  encryption: ['Key Providers', 'Policies', 'Encrypted VMs'],
  'fault-tolerance': ['Protected VMs', 'Compatibility Check', 'events', 'metrics'],
  'network-security': ['Policies', 'Firewall', 'Services', 'QoS', 'DNS', 'VPN', 'Mirror', 'NAT', 'Monitor'],
  notifications: ['Channels', 'Rules', 'History'],
  'merion/history': ['Runs', 'Triggers', 'Audit', 'Approvals'],
  'vms/web-01': ['Overview', 'Metrics', 'Disks', 'Network', 'Dataplane', 'Snapshots', 'Hotplug', 'Devices', 'Cloud-init', 'Rescue', 'Advanced', 'Logs'],
  'vms/staging-api': ['Overview', 'Metrics', 'Disks', 'Network', 'Dataplane', 'Snapshots', 'Hotplug', 'Devices', 'Cloud-init', 'Rescue', 'Advanced', 'Logs'],
  'vms/ci-runner': ['Overview', 'Metrics', 'Disks', 'Network', 'Dataplane', 'Snapshots', 'Hotplug', 'Devices', 'Cloud-init', 'Rescue', 'Advanced', 'Logs'],
}
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
    const body = mockBody(p) ?? (p.endsWith('s') ? [] : {})
    req.respond({ status: 200, contentType: 'application/json', body: JSON.stringify(body) })
  })
  await page.evaluateOnNewDocument((t) => {
    localStorage.setItem('zyvor_fabric_token', 'mock-token')
    localStorage.setItem('zyvor-fabricd-theme', t)
  }, theme)
  return page
}

export async function openPage(page, name, port = 3111, settleMs = 1800) {
  await page.goto(`http://localhost:${port}/app/${name}`, { waitUntil: 'load', timeout: 30000 })
  // the stagger/page-in animations take about a second to settle
  await new Promise((r) => setTimeout(r, settleMs))
  // A crashed page shows the error boundary, which has no violations of its own: without this the gate "passes" it.
  const crashed = await page.evaluate(() => document.body.innerText.includes('Section error') || document.body.innerText.includes('Something went wrong'))
  if (crashed) throw new Error(`/app/${name} rendered an error boundary instead of the page (the mock data probably misses an endpoint)`)
}
