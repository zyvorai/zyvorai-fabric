// Sign-in page (unauthenticated): screenshots in both themes and sizes, plus an axe pass.
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { serveDist, launch, SIZES } from './page-harness.mjs'
const out = process.argv[2]
const axeSource = readFileSync(createRequire(import.meta.url).resolve('axe-core/axe.min.js'), 'utf8')
const server = serveDist(3112)
const browser = await launch()
let bad = 0
for (const theme of ['light', 'dark']) for (const [size, [w, h]] of Object.entries(SIZES)) {
  const page = await browser.newPage()
  await page.setViewport({ width: w, height: h })
  await page.setRequestInterception(true)
  page.on('request', (r) => (new URL(r.url()).pathname.startsWith('/api/')
    ? r.respond({ status: 200, contentType: 'application/json', body: '{}' }) : r.continue()))
  await page.evaluateOnNewDocument((t) => { try { localStorage.setItem('zyvor-fabricd-theme', t) } catch {} }, theme)
  await page.goto('http://localhost:3112/sign-in', { waitUntil: 'load', timeout: 30000 })
  await new Promise((r) => setTimeout(r, 1500))
  await page.evaluate(axeSource)
  const res = await page.evaluate(() => axe.run(document, { runOnly: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'] }))
  const hard = res.violations.filter((v) => v.impact === 'serious' || v.impact === 'critical')
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1)
  console.log(`${theme}/${size}: ${hard.length} serious/critical${overflow ? ', SIDEWAYS SCROLL' : ''}`)
  for (const v of hard) { bad++; console.log('  ', v.id, v.nodes.slice(0, 3).map((n) => n.target.join(' ')).join(' | ')) }
  if (overflow) bad++
  if (out) await page.screenshot({ path: `${out}/signin-${theme}-${size}.png` })
  await page.close()
}
await browser.close(); server.close()
process.exit(bad ? 1 : 0)
