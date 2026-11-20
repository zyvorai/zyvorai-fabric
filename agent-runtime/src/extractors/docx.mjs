// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

main((buf) => {
  const entries = zipEntries(buf)
  const doc = zipRead(buf, entries, 'word/document.xml')
  if (!doc) throw new Error('word/document.xml not found: is this a .docx file?')
  let s = doc.toString('utf8')
  s = s.replace(/<w:tab\s*\/>/g, '\t').replace(/<w:br\b[^>]*\/>/g, '\n')
  s = s.replace(/<\/w:tc>/g, ' | ').replace(/<\/w:p>/g, '\n')
  s = stripTags(s)
  emit(tidy(decodeEntities(s)))
})
