// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { describe, expect, it } from 'vitest'
import { ansiToSpans, collapseTerminalControls, isSpinnerNoise, stripAnsi } from './ansi'

describe('ansi', () => {
  it('renders green OK from SGR', () => {
    const spans = ansiToSpans('\x1b[0;32m OK \x1b[0m Started')
    expect(spans.map((s) => s.text).join('')).toBe(' OK  Started')
    expect(spans[0].color).toBe('#32d74b')
  })

  it('collapses CR spinner frames to last paint', () => {
    const raw = 'old\r\x1b[Knew text'
    expect(collapseTerminalControls(raw).includes('old')).toBe(false)
    expect(stripAnsi(raw)).toContain('new text')
  })

  it('detects wait-online spinner noise', () => {
    expect(isSpinnerNoise('Job systemd-networkd-wait-online.service/start running (42s / no limit)')).toBe(true)
    expect(isSpinnerNoise('[  OK  ] Started systemd-journald.service')).toBe(false)
  })
})
