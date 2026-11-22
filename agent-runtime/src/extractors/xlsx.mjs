// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

// First sheet only, as plain CSV, so csv_columns and table read it like any CSV file.
const MAX_SHEETS = 1
const MAX_ROWS = 5000

function colIndex(ref) {
  const letters = /^[A-Z]+/.exec(ref)?.[0] ?? 'A'
  let n = 0
  for (const ch of letters) n = n * 26 + (ch.charCodeAt(0) - 64)
  return n - 1
}

function csvField(v) {
  return /[",\n\r]/.test(v) ? '"' + v.replace(/"/g, '""') + '"' : v
}

main((buf) => {
  const entries = zipEntries(buf)
  const read = (name) => zipRead(buf, entries, name)?.toString('utf8')
  const workbook = read('xl/workbook.xml')
  if (!workbook) throw new Error('xl/workbook.xml not found: is this a .xlsx file?')

  const shared = []
  const sst = read('xl/sharedStrings.xml')
  if (sst) {
    for (const si of sst.match(/<si\b[\s\S]*?<\/si>/g) ?? []) {
      shared.push(decodeEntities((si.match(/<t\b[^>]*>([\s\S]*?)<\/t>/g) ?? []).map((t) => stripTags(t)).join('')))
    }
  }

  const names = [...workbook.matchAll(/<sheet\b[^>]*\bname="([^"]*)"/g)].map((m) => decodeEntities(m[1]))
  const out = []
  for (let i = 0; i < Math.min(names.length || 1, MAX_SHEETS); i++) {
    const xml = read(`xl/worksheets/sheet${i + 1}.xml`)
    if (!xml) continue
    let rows = 0
    for (const row of xml.match(/<row\b[\s\S]*?<\/row>/g) ?? []) {
      if (rows++ >= MAX_ROWS) break
      const cells = []
      for (const c of row.match(/<c\b[^>]*?(?:\/>|>[\s\S]*?<\/c>)/g) ?? []) {
        const ref = /\br="([A-Z]+\d+)"/.exec(c)?.[1] ?? 'A1'
        const type = /\bt="([^"]*)"/.exec(c)?.[1]
        const v = /<v>([\s\S]*?)<\/v>/.exec(c)?.[1]
        let text = ''
        if (type === 's' && v !== undefined) text = shared[Number(v)] ?? ''
        else if (type === 'inlineStr') text = decodeEntities((c.match(/<t\b[^>]*>([\s\S]*?)<\/t>/g) ?? []).map((t) => stripTags(t)).join(''))
        else if (v !== undefined) text = decodeEntities(v)
        cells[colIndex(ref)] = text
      }
      out.push(Array.from(cells, (x) => csvField(x ?? '')).join(','))
    }
  }
  emit(out.join('\n'))
})
