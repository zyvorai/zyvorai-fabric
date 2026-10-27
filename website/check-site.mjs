// Docs-site gate (ux-plan step 8): for a set of pages, in both themes, at desktop and phone width:
// no sideways scroll, no serious/critical axe violation, and Lighthouse accessibility + performance >= 95.
// Run: npm run build && node check-site.mjs [--no-lighthouse]
import { readFileSync, existsSync, statSync } from 'node:fs'
import { createServer } from 'node:http'
import { createRequire } from 'node:module'
import { extname, join } from 'node:path'
import puppeteer from 'puppeteer'

const BASE = '/zyvorai-fabric' // docusaurus baseUrl
const PORT = 3120
const PAGES = ['/start', '/', '/docs/getting-started/Quick-Start', '/docs/tutorials/', '/docs/reference/api', '/merion', '/compare', '/support']
const SIZES = { desktop: [1440, 900], phone: [390, 844] }
const THEMES = ['light', 'dark']
const SCORE_FLOOR = 95
const lighthouseOn = !process.argv.includes('--no-lighthouse')

const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.json': 'application/json', '.png': 'image/png', '.jpg': 'image/jpeg', '.woff2': 'font/woff2', '.xml': 'application/xml' }
const server = createServer((req, res) => {
  const path = decodeURIComponent(new URL(req.url, 'http://x').pathname).replace(BASE, '') || '/'
  let file = join('build', path)
  if (existsSync(file) && statSync(file).isDirectory()) file = join(file, 'index.html')
  if (!existsSync(file)) file = join('build', '404.html')
  res.setHeader('Content-Type', TYPES[extname(file)] ?? 'application/octet-stream')
  res.end(readFileSync(file))
}).listen(PORT)

// Docusaurus namespaces its theme key (e.g. theme-371); read it from the built page so light mode really is light
const THEME_KEY = /localStorage\.getItem\("(theme[^"]*)"\)/.exec(readFileSync('build/index.html', 'utf8'))?.[1] ?? 'theme'
const axeSource = readFileSync(createRequire(import.meta.url).resolve('axe-core/axe.min.js'), 'utf8')
const browser = await puppeteer.launch({ args: process.env.CI ? ['--no-sandbox'] : [] })
const failures = []

for (const theme of THEMES) {
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
  const { default: lighthouse } = await import('lighthouse')
  for (const path of ['/start', '/docs/getting-started/Quick-Start']) {
    for (const [size, form] of [['desktop', 'desktop'], ['phone', 'mobile']]) {
      const lhr = (await lighthouse(
        `http://localhost:${PORT}${BASE}${path}`,
        { port: Number(new URL(browser.wsEndpoint()).port), onlyCategories: ['accessibility', 'performance'], logLevel: 'error' },
        form === 'desktop' ? { extends: 'lighthouse:default', settings: { formFactor: 'desktop', screenEmulation: { disabled: true }, throttlingMethod: 'simulate' } } : undefined,
      )).lhr
      const a11y = Math.round(lhr.categories.accessibility.score * 100)
      const perf = Math.round(lhr.categories.performance.score * 100)
      console.log(`lighthouse ${path} ${size}: accessibility ${a11y}, performance ${perf}`)
      if (a11y < SCORE_FLOOR) failures.push(`${path} ${size}: lighthouse accessibility ${a11y} < ${SCORE_FLOOR}`)
      if (perf < SCORE_FLOOR) failures.push(`${path} ${size}: lighthouse performance ${perf} < ${SCORE_FLOOR}`)
    }
  }
}

await browser.close()
server.close()
for (const f of failures) console.log(`FAIL ${f}`)
console.log(failures.length ? `\n${failures.length} site check failures` : '\nsite checks: all clear')
process.exit(failures.length ? 1 : 0)
