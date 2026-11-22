#!/usr/bin/env node
// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

/**
 * Regenerate docs/zyvor-fabric-user-feature-guide.{html,pdf} from the
 * current docs/zyvor-fabric-user-feature-guide.md, using the same
 * theme/wrapHtml pipeline as build-user-pdfs.mjs -- kept as a separate
 * script because that one's `books` array is hardcoded to docs/user/*,
 * a different directory this top-level guide isn't part of.
 */
import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync, writeFileSync, rmSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
try {
  const envText = readFileSync(resolve(ROOT, 'scripts/user-docs/product.env'), 'utf8')
  for (const line of envText.split('\n')) {
    const m = line.match(/^([A-Z0-9_]+)=(.*)$/)
    if (m) process.env[m[1]] = m[2].replace(/^['"]|['"]$/g, '')
  }
} catch {}
const PRODUCT = process.env.USER_DOCS_PRODUCT || 'Product'

const MARKED_CANDIDATES = [
  resolve(ROOT, '.docs-tools/node_modules/marked/bin/marked.js'),
  '/opt/homebrew/lib/node_modules/@vibe-kit/grok-cli/node_modules/marked/bin/marked.js',
  '/opt/homebrew/lib/node_modules/openclaw/node_modules/marked/bin/marked.js',
]
const MARKED = MARKED_CANDIDATES.find((p) => existsSync(p))

const THEMES = {
  'Zyvor Fabric': { accent: '#2dd4bf', grad: '#042f2e,#115e59,#0d1b2a', brandHtml: 'Zyvor <span>Fabric</span>' },
}
const theme = THEMES[PRODUCT] || { accent: '#60a5fa', grad: '#0a0a1a,#1e3a5f,#0d1b2a', brandHtml: PRODUCT }
const ACCENT = theme.accent
const GRAD = theme.grad

const CHROME_CANDIDATES = [
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/Applications/Chromium.app/Contents/MacOS/Chromium',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
]

function fail(msg) {
  console.error(`ERROR: ${msg}`)
  process.exit(1)
}

function findChrome() {
  for (const c of CHROME_CANDIDATES) if (existsSync(c)) return c
  return null
}

function mdToHtmlBody(md, tmpBase) {
  const tmpMd = `${tmpBase}.tmp.md`
  const tmpHtml = `${tmpBase}.tmp.html`
  writeFileSync(tmpMd, md)
  execFileSync(process.execPath, [MARKED, '--gfm', '-i', tmpMd, '-o', tmpHtml])
  const html = readFileSync(tmpHtml, 'utf8')
  rmSync(tmpMd)
  rmSync(tmpHtml)
  return html
}

function wrapHtml(title, sub, bodyHtml) {
  const today = new Date().toISOString().slice(0, 10)
  return `<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"/><title>${title} — ${PRODUCT}</title>
<style>
@page{size:A4;margin:15mm 16mm}*{margin:0;padding:0;box-sizing:border-box}
html{-webkit-print-color-adjust:exact;print-color-adjust:exact}
body{font-family:'Segoe UI',system-ui,sans-serif;color:#1f2937;line-height:1.58;font-size:10.5pt}
.cover{height:262mm;margin:-15mm -16mm 0;background:linear-gradient(135deg,${GRAD});color:#fff;display:flex;flex-direction:column;justify-content:center;align-items:center;text-align:center;page-break-after:always;position:relative}
.cover .kicker{letter-spacing:5px;text-transform:uppercase;opacity:.72;margin-bottom:1.4em}
.cover h1{font-size:3.2em;font-weight:800;letter-spacing:-2px}
.cover h1 span{color:${ACCENT}}
.cover .sub{font-size:1.3em;font-weight:300;opacity:.93;margin:1em 0;max-width:28em}
.cover .badge{display:inline-block;background:rgba(96,165,250,.2);border:1px solid ${ACCENT};padding:8px 22px;border-radius:20px;margin-top:2em}
.cover .foot{position:absolute;bottom:34px;font-size:.82em;opacity:.6}
h1,h2{page-break-before:always;margin:0 0 .35em;font-size:1.55em;font-weight:750;color:#12203a;border-bottom:2px solid #e5edfb;padding-bottom:.28em}
h1:first-of-type,h2:first-of-type{page-break-before:avoid}
h3{margin:1.1em 0 .4em;font-size:1.12em;font-weight:700}
p,li{margin:.4em 0}ul,ol{margin:.5em 0 .9em 1.4em}
a{color:#2563eb;text-decoration:none}
code{background:#eef2f7;padding:1px 6px;border-radius:4px;font-size:.86em;font-family:ui-monospace,Menlo,monospace}
pre{background:#0f172a;color:#e2e8f0;padding:12px 15px;border-radius:9px;overflow-x:auto;margin:.8em 0}
table{width:100%;border-collapse:collapse;margin:.8em 0 1.1em;font-size:9.5pt}
th,td{border:1px solid #e5e7eb;padding:6px 8px;text-align:left;vertical-align:top}th{background:#f1f5f9}
hr{border:none;border-top:1px solid #e5e7eb;margin:1.2em 0}
blockquote{border-left:3px solid ${ACCENT};padding:.2em .9em;color:#475569;margin:.8em 0}
</style></head><body>
<section class="cover"><div class="kicker">ZyvorAI Labs · User Documentation</div>
<h1>${theme.brandHtml}</h1>
<div class="sub">${sub}</div>
<div class="badge">${today}</div>
<div class="foot">zyvor.dev · Zyvor Production Licence v2.0</div></section>
<main>${bodyHtml}</main></body></html>`
}

function printPdf(chrome, htmlPath, pdfPath) {
  execFileSync(chrome, ['--headless', '--disable-gpu', '--no-pdf-header-footer', `--print-to-pdf=${pdfPath}`, htmlPath], {
    stdio: 'inherit',
  })
}

if (!MARKED) fail('marked not found at any known path')
const chrome = findChrome()
if (!chrome) fail('Chrome/Chromium required')

const SRC = resolve(ROOT, 'docs/zyvor-fabric-user-feature-guide.md')
const HTML_OUT = resolve(ROOT, 'docs/zyvor-fabric-user-feature-guide.html')
const PDF_OUT = resolve(ROOT, 'docs/zyvor-fabric-user-feature-guide.pdf')

const raw = readFileSync(SRC, 'utf8')
const h1Match = raw.match(/^#\s+([^\n]+)\n/)
const title = h1Match ? h1Match[1].replace(/^Zyvor Fabric\s*—\s*/, '') : 'Feature Guide'
const subMatch = raw.match(/^>\s*\*\*([^*]+)\*\*/m)
const sub = subMatch ? subMatch[1] : title
const body = raw.replace(/^#\s+[^\n]+\n+/, '').replace(/^>\s*\*\*[^*]+\*\*\n+/, '')

const bodyHtml = mdToHtmlBody(body, resolve(ROOT, 'docs/.feature-guide-build'))
writeFileSync(HTML_OUT, wrapHtml(title, sub, bodyHtml))
console.log(`Wrote ${HTML_OUT}`)
console.log('Printing PDF …')
printPdf(chrome, `file://${HTML_OUT}`, PDF_OUT)
console.log(`Wrote ${PDF_OUT}`)
