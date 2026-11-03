// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest'
import { changeCount, formatRemaining, secondsUntilExpiry, summarizeChangeset } from './changeset'

describe('summarizeChangeset', () => {
  it('reads a changeset approval as the runtime writes it', () => {
    const s = summarizeChangeset({
      changeset_id: 'cs-1',
      command: 'echo hello > /tmp/specdir/a.txt',
      exit_code: 0,
      changes: { added: ['/tmp/specdir/a.txt'], deleted: [], modified: ['/tmp/b'], unchanged: 3 },
      side_effects: { egress: 'blocked', non_replayable: ['sent mail'] },
      unstaged: ['/tmp/big'],
      expires_at: 1000,
    })
    expect(s).toEqual({
      command: 'echo hello > /tmp/specdir/a.txt',
      exitCode: 0,
      added: ['/tmp/specdir/a.txt'],
      modified: ['/tmp/b'],
      deleted: [],
      unstaged: ['/tmp/big'],
      egress: 'blocked',
      nonReplayable: ['sent mail'],
      expiresAt: 1000,
    })
    expect(changeCount(s!)).toBe(2)
  })

  it('is null for anything that is not a changeset', () => {
    expect(summarizeChangeset(null)).toBeNull()
    expect(summarizeChangeset('x')).toBeNull()
    expect(summarizeChangeset({ host: 'example.com' })).toBeNull()
  })

  it('tolerates missing and malformed fields', () => {
    const s = summarizeChangeset({ changeset_id: 'cs', changes: 'oops', side_effects: null, exit_code: '0' })
    expect(s).toMatchObject({ exitCode: null, added: [], egress: null, command: '', expiresAt: null })
  })
})

describe('expiry', () => {
  const at = (expires_at: unknown) => summarizeChangeset({ changeset_id: 'cs', expires_at })!

  it('counts down from unix seconds and floors at zero', () => {
    expect(secondsUntilExpiry(at(1000), 940_000)).toBe(60)
    expect(secondsUntilExpiry(at(1000), 999_500)).toBe(1)
    expect(secondsUntilExpiry(at(1000), 1_500_000)).toBe(0)
  })

  it('is null when no deadline was reported', () => {
    expect(secondsUntilExpiry(at(undefined), 0)).toBeNull()
    expect(secondsUntilExpiry(at(0), 0)).toBeNull()
    expect(secondsUntilExpiry(at('soon'), 0)).toBeNull()
  })

  it('formats the remaining time', () => {
    expect(formatRemaining(125)).toBe('2m 05s left')
    expect(formatRemaining(9)).toBe('9s left')
    expect(formatRemaining(0)).toBe('expired')
  })
})
