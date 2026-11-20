// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

// Slide text and speaker notes of a PowerPoint .pptx, in presentation order, as plain text.
const MAX_SLIDES = 500

function paragraphs(xml) {
  // Field placeholders (slide numbers, dates) are noise; remove them before reading runs.
  xml = xml.replace(/<a:fld\b[\s\S]*?<\/a:fld>/g, '')
  const out = []
  for (const p of xml.match(/<a:p\b[\s\S]*?<\/a:p>/g) ?? []) {
    const line = p
      .replace(/<a:br\s*\/>/g, '\n')
      .replace(/<a:tab\s*\/>/g, '\t')
      .match(/<a:t\b[^>]*>[\s\S]*?<\/a:t>|\n|\t/g)
    if (!line) continue
    const text = decodeEntities(line.map((t) => (t === '\n' || t === '\t' ? t : stripTags(t))).join('')).trim()
    if (text) out.push(text)
  }
  return out
}

function relTargets(rels) {
  // Relationship Id -> Target, for the parts we follow. Attribute order varies between writers.
  const map = new Map()
  for (const r of rels.match(/<Relationship\b[^>]*>/g) ?? []) {
    const id = /\bId="([^"]*)"/.exec(r)?.[1]
    const target = /\bTarget="([^"]*)"/.exec(r)?.[1]
    const type = /\bType="([^"]*)"/.exec(r)?.[1] ?? ''
    if (id && target) map.set(id, { target, type })
  }
  return map
}

function resolve(base, target) {
  // Targets in these packages are relative ("slides/slide1.xml", "../notesSlides/notesSlide1.xml").
  const parts = base.split('/').slice(0, -1)
  for (const seg of target.split('/')) {
    if (seg === '..') parts.pop()
    else if (seg !== '.' && seg !== '') parts.push(seg)
  }
  return parts.join('/')
}

main((buf) => {
  const entries = zipEntries(buf)
  const read = (name) => zipRead(buf, entries, name)?.toString('utf8')
  const presentation = read('ppt/presentation.xml')
  if (!presentation) throw new Error('ppt/presentation.xml not found: is this a .pptx file?')

  // Presentation order: the slide id list in presentation.xml, resolved through its relationships.
  let slides = []
  const rels = read('ppt/_rels/presentation.xml.rels')
  if (rels) {
    const targets = relTargets(rels)
    for (const m of presentation.matchAll(/<p:sldId\b[^>]*\br:id="([^"]*)"/g)) {
      const t = targets.get(m[1])
      if (t) slides.push(resolve('ppt/presentation.xml', t.target))
    }
  }
  if (slides.length === 0) {
    slides = [...entries.keys()]
      .filter((n) => /^ppt\/slides\/slide\d+\.xml$/.test(n))
      .sort((a, b) => Number(/(\d+)\.xml$/.exec(a)[1]) - Number(/(\d+)\.xml$/.exec(b)[1]))
  }

  const out = [`# Deck: ${slides.length} slide${slides.length === 1 ? '' : 's'}`]
  slides.slice(0, MAX_SLIDES).forEach((name, i) => {
    const xml = read(name)
    if (xml === undefined) return
    out.push('', `## Slide ${i + 1}`)
    out.push(...paragraphs(xml))
    const slideRels = read(name.replace(/slides\/([^/]+)$/, 'slides/_rels/$1.rels'))
    if (slideRels) {
      for (const { target, type } of relTargets(slideRels).values()) {
        if (!type.endsWith('/notesSlide')) continue
        const notes = read(resolve(name, target))
        if (notes) {
          const lines = paragraphs(notes)
          if (lines.length) out.push(`Notes: ${lines.join(' ')}`)
        }
      }
    }
  })
  emit(tidy(out.join('\n')))
})
