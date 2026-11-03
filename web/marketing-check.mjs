// Marketing pages (signed out): axe in both themes at desktop and phone width, plus a sideways-scroll check. These pages were not
// in any gate before, and the check found a brand link whose label did not match its text, a button with low contrast on the
// inverted band, and a scrollable box a keyboard could not reach.
// Run: npx vite build && node marketing-check.mjs
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { serveDist, launch, SIZES } from './page-harness.mjs'

const ROUTES = ['/', '/product', '/platform', '/security', '/merion']
const axeSource = readFileSync(createRequire(import.meta.url).resolve('axe-core/axe.min.js'), 'utf8')
const server = serveDist(3113)
const browser = await launch()
let bad = 0
for (const route of ROUTES) {
  for (const theme of ['light', 'dark']) {
    for (const [size, [w, h]] of Object.entries(SIZES)) {
      const page = await browser.newPage()
      await page.setViewport({ width: w, height: h })
      await page.setRequestInterception(true)
      page.on('request', (r) => (new URL(r.url()).pathname.startsWith('/api/')
        ? r.respond({ status: 200, contentType: 'application/json', body: '{}' }) : r.continue()))
      await page.evaluateOnNewDocument((t) => { try { localStorage.setItem('zyvor-fabricd-theme', t) } catch {} }, theme)
      await page.goto(`http://localhost:3113${route}`, { waitUntil: 'load', timeout: 30000 })
      await new Promise((r) => setTimeout(r, 1500))
      await page.evaluate(axeSource)
      const res = await page.evaluate(() => axe.run(document, { runOnly: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'] }))
      const hard = res.violations.filter((v) => v.impact === 'serious' || v.impact === 'critical')
      const sideways = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1)
      if (hard.length || sideways) {
        bad++
        console.log(`FAIL ${route} ${theme}/${size}:`, hard.map((v) => `${v.id} ${v.nodes.slice(0, 2).map((n) => n.target.join(' ')).join(' | ')}`).join(' ; '), sideways ? 'SIDEWAYS SCROLL' : '')
      }
      await page.close()
    }
  }
}
console.log(bad ? `marketing: ${bad} page/theme/size combinations with problems` : `marketing: ${ROUTES.length} pages, 0 serious/critical, no sideways scroll`)
await browser.close(); server.close()
process.exit(bad ? 1 : 0)
