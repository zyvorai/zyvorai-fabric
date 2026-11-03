// Baseline screenshots of the busiest console pages (ux-plan step 0).
// Run: npx vite build && node baseline-shots.mjs <outdir>
import { mkdirSync } from 'node:fs'
import { PAGES, SIZES, serveDist, launch, newConsolePage, openPage } from './page-harness.mjs'

const out = process.argv[2]
mkdirSync(out, { recursive: true })
const server = serveDist()
const browser = await launch()
for (const theme of ['light', 'dark']) {
  for (const [size, dims] of Object.entries(SIZES)) {
    const page = await newConsolePage(browser, theme, dims)
    for (const name of PAGES) {
      await openPage(page, name)
      const sideways = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)
      await page.screenshot({ path: `${out}/${name || 'dashboard'}-${theme}-${size}.png` })
      if (sideways) console.log('SIDEWAYS', size, theme, name || 'dashboard')
    }
    await page.close()
  }
}
await browser.close()
server.close()
