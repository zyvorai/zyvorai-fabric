// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

main((buf) => {
  let s = buf.toString('utf8')
  s = s.replace(/<!--[\s\S]*?-->/g, ' ')
  s = s.replace(/<(script|style|noscript|template|svg)\b[\s\S]*?<\/\1\s*>/gi, ' ')
  s = s.replace(/<h([1-6])\b[^>]*>/gi, (_m, n) => '\n\n' + '#'.repeat(Number(n)) + ' ')
  s = s.replace(/<\/(p|div|section|article|header|footer|li|ul|ol|table|h[1-6]|blockquote|pre)\s*>/gi, '\n')
  s = s.replace(/<(br|hr)\b[^>]*>/gi, '\n')
  s = s.replace(/<\/t[dh]\s*>/gi, ' | ')
  s = s.replace(/<tr\b[^>]*>/gi, '\n')
  s = stripTags(s, ' ')
  emit(tidy(decodeEntities(s)))
})
