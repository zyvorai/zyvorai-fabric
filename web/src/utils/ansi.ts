// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

/** Terminal.app-ish ANSI colors on a near-black background. */
const FG: Record<number, string> = {
  30: '#8e8e93', // black → dim gray (readable on black)
  31: '#ff453a', // red
  32: '#32d74b', // green
  33: '#ffd60a', // yellow
  34: '#0a84ff', // blue
  35: '#bf5af2', // magenta
  36: '#64d2ff', // cyan
  37: '#f5f5f7', // white
  39: '#f5f5f7', // default
  90: '#636366',
  91: '#ff6961',
  92: '#30db5b',
  93: '#ffd426',
  94: '#409cff',
  95: '#da8fff',
  96: '#70d7ff',
  97: '#ffffff',
}

export type AnsiSpan = {
  text: string
  color?: string
  bold?: boolean
  dim?: boolean
}

/**
 * Apply CR / CSI erase-in-line so spinner frames collapse to their final text.
 */
export function collapseTerminalControls(raw: string): string {
  let out = ''
  let i = 0
  while (i < raw.length) {
    const ch = raw[i]
    if (ch === '\r') {
      // Overwrite from start of current "visual" line (after last \n).
      const nl = out.lastIndexOf('\n')
      out = nl >= 0 ? out.slice(0, nl + 1) : ''
      i += 1
      continue
    }
    if (ch === '\x1b' && raw[i + 1] === '[') {
      let j = i + 2
      while (j < raw.length && !/[A-Za-z]/.test(raw[j])) j += 1
      const cmd = raw[j]
      // Erase in line (K) / erase display fragments — drop the CSI, keep buffer.
      if (cmd === 'K' || cmd === 'J') {
        if (cmd === 'K') {
          // CSI 0K / K: clear from cursor to end → truncate current line at cursor (= end of out after CR handling)
          // Cursor is at end of out in our model, so no-op.
        }
        i = j + 1
        continue
      }
      // Leave SGR and other CSI for the color parser; copy through for now.
      out += raw.slice(i, j + 1)
      i = j + 1
      continue
    }
    out += ch
    i += 1
  }
  return out
}

/** True for systemd wait-online spinner frames that drown boot logs. */
export function isSpinnerNoise(text: string): boolean {
  // ESC CSI sequences — control char is intentional
  // eslint-disable-next-line no-control-regex
  const plain = text.replace(/\x1b\[[0-9;?]*[A-Za-z]/g, '').replace(/\r/g, '').trim()
  if (!plain) return true
  return /systemd-networkd-wait-online/i.test(plain) && /running \(/i.test(plain)
}

/**
 * Parse SGR sequences into styled spans. Unknown CSI/OSC are stripped.
 */
export function ansiToSpans(input: string): AnsiSpan[] {
  const prepared = collapseTerminalControls(input)
  const spans: AnsiSpan[] = []
  let color: string | undefined = FG[37]
  let bold = false
  let dim = false
  let buf = ''

  const flush = () => {
    if (!buf) return
    spans.push({ text: buf, color, bold: bold || undefined, dim: dim || undefined })
    buf = ''
  }

  let i = 0
  while (i < prepared.length) {
    if (prepared[i] === '\x1b') {
      if (prepared[i + 1] === '[') {
        let j = i + 2
        while (j < prepared.length && !/[A-Za-z]/.test(prepared[j])) j += 1
        const cmd = prepared[j]
        const params = prepared.slice(i + 2, j)
        if (cmd === 'm') {
          flush()
          const parts = params === '' ? ['0'] : params.split(';')
          for (const p of parts) {
            const n = Number(p)
            if (!Number.isFinite(n) || p === '') {
              color = FG[37]
              bold = false
              dim = false
              continue
            }
            if (n === 0) {
              color = FG[37]
              bold = false
              dim = false
            } else if (n === 1) bold = true
            else if (n === 2) dim = true
            else if (n === 22) {
              bold = false
              dim = false
            } else if (n === 39) color = FG[37]
            else if (FG[n]) color = FG[n]
          }
          i = j + 1
          continue
        }
        // Drop other CSI (cursor, etc.)
        i = j + 1
        continue
      }
      if (prepared[i + 1] === ']') {
        // OSC … BEL or ST
        let j = i + 2
        while (j < prepared.length && prepared[j] !== '\x07' && !(prepared[j] === '\x1b' && prepared[j + 1] === '\\')) j += 1
        i = prepared[j] === '\x07' ? j + 1 : j + 2
        continue
      }
      // Lone ESC — skip
      i += 1
      continue
    }
    buf += prepared[i]
    i += 1
  }
  flush()
  return spans
}

export function stripAnsi(input: string): string {
  return ansiToSpans(input).map((s) => s.text).join('')
}
