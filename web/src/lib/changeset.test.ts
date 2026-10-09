// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest'
import { changeCount, summarizeChangeset } from './changeset'

describe('summarizeChangeset', () => {
  it('reads a changeset approval as the runtime writes it', () => {
    const s = summarizeChangeset({
      changeset_id: 'cs-1',
      command: 'echo hello > /tmp/specdir/a.txt',
      exit_code: 0,
      changes: { added: ['/tmp/specdir/a.txt'], deleted: [], modified: ['/tmp/b'], unchanged: 3 },
      side_effects: { egress: 'blocked', non_replayable: ['sent mail'] },
      unstaged: ['/tmp/big'],
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
    expect(s).toMatchObject({ exitCode: null, added: [], egress: null, command: '' })
  })
})
