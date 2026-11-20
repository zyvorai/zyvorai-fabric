// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

function decodeWord(s) {
  return s.replace(/=\?([\w-]+)\?([bBqQ])\?([^?]*)\?=/g, (_m, _cs, enc, text) => {
    try {
      if (enc.toLowerCase() === 'b') return Buffer.from(text, 'base64').toString('utf8')
      return Buffer.from(text.replace(/_/g, ' ').replace(/=([0-9A-F]{2})/gi, (_x, h) => String.fromCharCode(parseInt(h, 16))), 'latin1').toString('utf8')
    } catch { return text }
  })
}

function parseHeaders(block) {
  const headers = {}
  for (const line of block.replace(/\r\n/g, '\n').replace(/\n[ \t]+/g, ' ').split('\n')) {
    const i = line.indexOf(':')
    if (i > 0) headers[line.slice(0, i).toLowerCase()] = line.slice(i + 1).trim()
  }
  return headers
}

function decodeBody(body, encoding) {
  const enc = (encoding ?? '').toLowerCase()
  if (enc === 'base64') return Buffer.from(body.replace(/\s+/g, ''), 'base64').toString('utf8')
  if (enc === 'quoted-printable') {
    return Buffer.from(
      body.replace(/=\r?\n/g, '').replace(/=([0-9A-F]{2})/gi, (_m, h) => String.fromCharCode(parseInt(h, 16))),
      'latin1',
    ).toString('utf8')
  }
  return body
}

function stripHtml(s) {
  return decodeEntities(stripTags(s.replace(/<(script|style)\b[\s\S]*?<\/\1\s*>/gi, ' ').replace(/<br\s*\/?>|<\/p>|<\/div>/gi, '\n'), ' '))
}

function split(raw) {
  const text = raw.replace(/\r\n/g, '\n')
  const i = text.indexOf('\n\n')
  return i < 0 ? [text, ''] : [text.slice(0, i), text.slice(i + 2)]
}

function textOf(raw, depth = 0) {
  const [head, body] = split(raw)
  const h = parseHeaders(head)
  const type = h['content-type'] ?? 'text/plain'
  const boundary = /boundary="?([^";\s]+)"?/i.exec(type)?.[1]
  if (/^multipart\//i.test(type) && boundary && depth < 4) {
    const parts = body.split('--' + boundary).slice(1).filter((p) => !p.startsWith('--'))
    const texts = parts.map((p) => textOf(p.replace(/^\n/, ''), depth + 1))
    const plain = texts.find((t) => t.kind === 'plain')
    return plain ?? texts.find((t) => t.kind === 'html') ?? { kind: 'none', text: '' }
  }
  if (/^message\/rfc822/i.test(type)) return { kind: 'none', text: '' }
  const decoded = decodeBody(body, h['content-transfer-encoding'])
  if (/^text\/html/i.test(type)) return { kind: 'html', text: stripHtml(decoded) }
  if (/^text\//i.test(type)) return { kind: 'plain', text: decoded }
  return { kind: 'none', text: '' }
}

main((buf) => {
  const raw = buf.toString('utf8').replace(/\r\n/g, '\n')
  const messages = raw.startsWith('From ') ? raw.split(/^From .*\n/m).slice(1) : [raw]
  const out = []
  for (const msg of messages.slice(0, 500)) {
    const [head] = split(msg)
    const h = parseHeaders(head)
    const body = tidy(textOf(msg).text)
    out.push(
      [
        `From: ${decodeWord(h.from ?? '')}`,
        `Date: ${h.date ?? ''}`,
        `Subject: ${decodeWord(h.subject ?? '')}`,
        '',
        body,
        '',
        '---',
      ].join('\n'),
    )
  }
  emit(out.join('\n'))
})
