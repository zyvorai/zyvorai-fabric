// Docs-site gate (ux-plan step 8): for a set of pages, in both themes, at desktop and phone width:
// no sideways scroll, no serious/critical axe violation, and Lighthouse accessibility + performance >= 95.
// Run: npm run build && node check-site.mjs [--no-lighthouse | --lighthouse-only]
import { readFileSync, existsSync, statSync } from 'node:fs'
import { createServer } from 'node:http'
import { createRequire } from 'node:module'
import { extname, join } from 'node:path'
import { gzipSync } from 'node:zlib'
import puppeteer from 'puppeteer'

const BASE = '/zyvorai-fabric' // docusaurus baseUrl
const PORT = 3120
const PAGES = ['/start', '/', '/docs/getting-started/Quick-Start', '/docs/tutorials/', '/docs/reference/api', '/merion', '/compare', '/support']
const SIZES = { desktop: [1440, 900], phone: [390, 844] }
const THEMES = ['light', 'dark']
const SCORE_FLOOR = 95
const lighthouseOn = !process.argv.includes('--no-lighthouse')
const axeOn = !process.argv.includes('--lighthouse-only')

const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.json': 'application/json', '.png': 'image/png', '.jpg': 'image/jpeg', '.woff2': 'font/woff2', '.xml': 'application/xml' }
const server = createServer((req, res) => {
  const path = decodeURIComponent(new URL(req.url, 'http://x').pathname).replace(BASE, '') || '/'
  let file = join('build', path)
  if (existsSync(file) && statSync(file).isDirectory()) file = join(file, 'index.html')
  if (!existsSync(file)) file = join('build', '404.html')
  const type = TYPES[extname(file)] ?? 'application/octet-stream'
  res.setHeader('Content-Type', type)
  // GitHub Pages compresses text and caches for ten minutes; measure the same, not a bare file server
  res.setHeader('Cache-Control', 'max-age=600')
  const body = readFileSync(file)
  if (/text|javascript|json|svg|xml/.test(type) && /gzip/.test(req.headers['accept-encoding'] ?? '')) {
    res.setHeader('Content-Encoding', 'gzip')
    res.end(gzipSync(body))
  } else res.end(body)
}).listen(PORT)

// Docusaurus namespaces its theme key (e.g. theme-371); read it from the built page so light mode really is light
const THEME_KEY = /localStorage\.getItem\("(theme[^"]*)"\)/.exec(readFileSync('build/index.html', 'utf8'))?.[1] ?? 'theme'
const axeSource = readFileSync(createRequire(import.meta.url).resolve('axe-core/axe.min.js'), 'utf8')
const browser = await puppeteer.launch({ args: process.env.CI ? ['--no-sandbox'] : [] })
const failures = []

for (const theme of axeOn ? THEMES : []) {
  for (const [size, [width, height]] of Object.entries(SIZES)) {
    const page = await browser.newPage()
    await page.setViewport({ width, height })
    await page.evaluateOnNewDocument((k, t) => localStorage.setItem(k, t), THEME_KEY, theme)
    for (const path of PAGES) {
      await page.goto(`http://localhost:${PORT}${BASE}${path}`, { waitUntil: 'networkidle0', timeout: 60000 })
      const where = `${path} ${theme} ${size}`
      const actual = await page.evaluate(() => document.documentElement.getAttribute('data-theme'))
      if (actual !== theme) failures.push(`${where}: page rendered in ${actual} theme, not ${theme}`)
      if (await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)) failures.push(`${where}: scrolls sideways`)
      await page.addStyleTag({ content: '*,*::before,*::after{animation:none!important;transition:none!important}' })
      await page.evaluate(axeSource)
      const { violations } = await page.evaluate(() =>
        // eslint-disable-next-line no-undef
        axe.run(document, { runOnly: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'], resultTypes: ['violations'] }),
      )
      for (const v of violations.filter((x) => x.impact === 'serious' || x.impact === 'critical'))
        for (const n of v.nodes.slice(0, 3)) {
          const d = v.id === 'color-contrast' ? n.any[0]?.data : null
          const ratio = d ? ` {${d.fgColor} on ${d.bgColor} = ${d.contrastRatio}}` : ''
          failures.push(`${where}: axe ${v.id}: ${n.target.join(' ')}${ratio}`)
        }
    }
    await page.close()
  }
}

if (lighthouseOn) {
  // Lighthouse scores swing with machine load (the same page scored 80 and 96), so each page gets a fresh browser
  // and the median of three runs.
  const { default: lighthouse } = await import('lighthouse')
  // the stock desktop config also switches off the mobile network and CPU throttling
  const { default: desktopConfig } = await import('lighthouse/core/config/desktop-config.js')
  const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)]
  const lhBrowser = await puppeteer.launch({ args: process.env.CI ? ['--no-sandbox'] : [] })
  for (const path of ['/start', '/docs/getting-started/Quick-Start']) {
    for (const [size, form] of [['desktop', 'desktop'], ['phone', 'mobile']]) {
      const a11y = []
      const perf = []
      for (let i = 0; i < 3; i++) {
        const lhr = (await lighthouse(
          `http://localhost:${PORT}${BASE}${path}`,
          { port: Number(new URL(lhBrowser.wsEndpoint()).port), onlyCategories: ['accessibility', 'performance'], logLevel: 'error' },
          form === 'desktop' ? desktopConfig : undefined,
        )).lhr
        a11y.push(Math.round(lhr.categories.accessibility.score * 100))
        perf.push(Math.round(lhr.categories.performance.score * 100))
      }
      console.log(`lighthouse ${path} ${size}: accessibility ${median(a11y)}, performance ${median(perf)} (runs: ${perf.join(', ')})`)
      if (median(a11y) < SCORE_FLOOR) failures.push(`${path} ${size}: lighthouse accessibility ${median(a11y)} < ${SCORE_FLOOR}`)
      if (median(perf) < SCORE_FLOOR) failures.push(`${path} ${size}: lighthouse performance ${median(perf)} < ${SCORE_FLOOR}`)
    }
  }
  await lhBrowser.close()
}

await browser.close()
server.close()
for (const f of failures) console.log(`FAIL ${f}`)
console.log(failures.length ? `\n${failures.length} site check failures` : '\nsite checks: all clear')
process.exit(failures.length ? 1 : 0)
