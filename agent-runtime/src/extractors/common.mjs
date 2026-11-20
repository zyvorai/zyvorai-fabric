// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

// Runs inside the sealed cell on an untrusted upload. Output is plain text on stdout.
import fs from 'node:fs'
import zlib from 'node:zlib'

const OUT_CAP = 300000
const inputPath = process.argv[2]

function emit(text) {
  process.stdout.write(String(text).slice(0, OUT_CAP))
}

function decodeEntities(s) {
  const named = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ' }
  return s.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (m, e) => {
    if (e[0] === '#') {
      const n = e[1].toLowerCase() === 'x' ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10)
      return Number.isFinite(n) && n > 0 && n < 0x110000 ? String.fromCodePoint(n) : ''
    }
    return named[e.toLowerCase()] ?? m
  })
}

/**
 * Remove every tag. A single pass is not enough: "<<b>script>" loses "<b>" and leaves "<script>", so repeat
 * until nothing changes. (The host also defangs "<" and ">" in what it renders; this is the guest's own part.)
 */
function stripTags(s, replacement = '') {
  let previous
  do {
    previous = s
    s = s.replace(/<[^>]*>/g, replacement)
  } while (s !== previous)
  return s
}

function tidy(s) {
  return s
    .replace(/[ \t\f\v]+/g, ' ')
    .replace(/ *\n */g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

// Minimal ZIP reader: stored and deflate entries, no ZIP64, no encryption.
const MAX_ENTRY = 20 * 1024 * 1024

function zipEntries(buf) {
  let eocd = -1
  for (let i = buf.length - 22; i >= Math.max(0, buf.length - 22 - 65535); i--) {
    if (buf.readUInt32LE(i) === 0x06054b50) { eocd = i; break }
  }
  if (eocd < 0) throw new Error('not a zip file')
  const count = buf.readUInt16LE(eocd + 10)
  let p = buf.readUInt32LE(eocd + 16)
  const out = new Map()
  for (let n = 0; n < count; n++) {
    if (buf.readUInt32LE(p) !== 0x02014b50) throw new Error('corrupt zip directory')
    const flags = buf.readUInt16LE(p + 8)
    const method = buf.readUInt16LE(p + 10)
    const csize = buf.readUInt32LE(p + 20)
    const usize = buf.readUInt32LE(p + 24)
    const nlen = buf.readUInt16LE(p + 28)
    const xlen = buf.readUInt16LE(p + 30)
    const clen = buf.readUInt16LE(p + 32)
    const local = buf.readUInt32LE(p + 42)
    const name = buf.toString('utf8', p + 46, p + 46 + nlen)
    out.set(name, { flags, method, csize, usize, local })
    p += 46 + nlen + xlen + clen
  }
  return out
}

function zipRead(buf, entries, name) {
  const e = entries.get(name)
  if (!e) return null
  if (e.flags & 1) throw new Error('encrypted entries are not supported')
  if (e.usize > MAX_ENTRY) throw new Error(`${name} is larger than ${MAX_ENTRY} bytes unpacked`)
  if (buf.readUInt32LE(e.local) !== 0x04034b50) throw new Error('corrupt zip entry')
  const start = e.local + 30 + buf.readUInt16LE(e.local + 26) + buf.readUInt16LE(e.local + 28)
  const data = buf.subarray(start, start + e.csize)
  if (e.method === 0) return data
  if (e.method === 8) return zlib.inflateRawSync(data, { maxOutputLength: MAX_ENTRY })
  throw new Error(`unsupported zip compression method ${e.method}`)
}

function main(fn) {
  try {
    fn(fs.readFileSync(inputPath))
  } catch (err) {
    process.stderr.write(`extract failed: ${err.message}\n`)
    process.exit(2)
  }
}
