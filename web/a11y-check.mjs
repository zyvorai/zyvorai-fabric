// Accessibility gate (ux-plan step 7): axe-core on the baseline pages in both themes at desktop and phone
// width, plus a visible-focus check (Tab twice, the focused control must show an outline or ring).
// Fails on any serious or critical violation (contrast, unlabelled controls, ...) or a missing focus ring.
// Run: npx vite build && node a11y-check.mjs [--verbose]
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { PAGES, SIZES, serveDist, launch, newConsolePage, openPage } from './page-harness.mjs'

const axeSource = readFileSync(createRequire(import.meta.url).resolve('axe-core/axe.min.js'), 'utf8')
const verbose = process.argv.includes('--verbose')
const server = serveDist()
const browser = await launch()
const focusFailures = []
const found = new Map() // rule id -> { impact, help, nodes: Set<"page theme size: selector"> }
for (const theme of ['light', 'dark']) {
  for (const [size, dims] of Object.entries(SIZES)) {
    const page = await newConsolePage(browser, theme, dims)
    for (const name of PAGES) {
      await openPage(page, name)
      // keyboard focus must be visible: the skip link is first, so the second Tab lands on a real control
      await page.keyboard.press('Tab')
      await page.keyboard.press('Tab')
      const ring = await page.evaluate(() => {
        const el = document.activeElement
        if (!el || el === document.body) return 'nothing focused'
        const c = getComputedStyle(el)
        const visible = (c.outlineStyle !== 'none' && parseFloat(c.outlineWidth) > 0) || c.boxShadow !== 'none'
        return visible ? null : `no focus ring on <${el.tagName.toLowerCase()}> ${String(el.className).slice(0, 60)}`
      })
      if (ring) focusFailures.push(`${name || 'dashboard'} ${theme} ${size}: ${ring}`)
      // settle every animation first: axe reads the colours of elements that are mid-fade
      await page.addStyleTag({ content: '*,*::before,*::after{animation:none!important;transition:none!important}' })
      await page.evaluate(axeSource)
      const { violations } = await page.evaluate(() =>
        // eslint-disable-next-line no-undef
        axe.run(document, { runOnly: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'], resultTypes: ['violations'] }),
      )
      for (const v of violations) {
        const entry = found.get(v.id) ?? { impact: v.impact, help: v.help, nodes: [] }
        for (const n of v.nodes) {
          const d = n.any[0]?.data
          const ratio = v.id === 'color-contrast' && d ? ` {${d.fgColor} on ${d.bgColor} = ${d.contrastRatio}}` : ''
          entry.nodes.push(`${name || 'dashboard'} ${theme} ${size}: ${n.target.join(' ')}${ratio}`)
        }
        found.set(v.id, entry)
      }
    }
    await page.close()
  }
}
await browser.close()
server.close()

const theme_of = (w) => ''
let blocking = 0
for (const [id, v] of [...found].sort((a, b) => b[1].nodes.length - a[1].nodes.length)) {
  const gate = v.impact === 'serious' || v.impact === 'critical'
  if (gate) blocking += v.nodes.length
  console.log(`${gate ? 'FAIL' : 'warn'} ${v.impact} ${id}: ${v.help} (${v.nodes.length})`)
  // --verbose lists every distinct element (with the pages it appears on); default shows four examples
  const distinct = new Map()
  for (const n of v.nodes) {
    const [where, sel] = n.split(': ')
    distinct.set(sel, [...(distinct.get(sel) ?? []), where + (theme_of(where))])
  }
  for (const [sel, wheres] of verbose ? distinct : [...distinct].slice(0, 4))
    console.log(`    ${sel}  [${wheres.length}: ${[...new Set(wheres.map((w) => w.split(' ')[0]))].slice(0, 3).join(', ')}]`)
}
for (const f of focusFailures) console.log(`FAIL focus ${f}`)
blocking += focusFailures.length
console.log(blocking ? `\n${blocking} serious/critical or focus violations` : '\naxe: no serious or critical violations')
process.exit(blocking ? 1 : 0)
