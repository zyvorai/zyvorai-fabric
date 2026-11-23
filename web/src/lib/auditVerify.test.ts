// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect } from 'vitest'
import { describeVerify } from './auditVerify'
import { auditExportQuery, type AuditVerifyReport } from '../api/audit'

const base: AuditVerifyReport = {
  status: 'ok', ok: true, chain_enabled: true, total: 5, chained: 5, legacy_unchained: 0,
  malformed: [], checked_at: '2026-03-01T00:00:00Z',
}

describe('describeVerify', () => {
  it('reports an intact chain', () => {
    const s = describeVerify(base)
    expect(s.tone).toBe('ok')
    expect(s.headline).toBe('Chain intact: 5 chained events verified')
  })

  it('mentions events that predate the chain', () => {
    expect(describeVerify({ ...base, legacy_unchained: 2 }).detail).toContain('2 older events predate the chain')
  })

  it('says nothing to verify when no chain exists', () => {
    const s = describeVerify({ ...base, status: 'not_chained', chained: 0, total: 1 })
    expect(s.tone).toBe('neutral')
    expect(s.detail).toContain('1 event stored without hashes')
  })

  it('names the first broken link', () => {
    const s = describeVerify({
      ...base, status: 'broken', ok: false,
      first_break: { reason: 'sequence_gap', seq: 4, id: 'e4', detail: 'events 3..3 are missing (removed)' },
    })
    expect(s.tone).toBe('broken')
    expect(s.headline).toBe('Chain broken at event 4')
    expect(s.detail).toContain('One or more events were removed')
    expect(s.detail).toContain('events 3..3 are missing')
  })

  it('copes with an unknown reason and no sequence', () => {
    const s = describeVerify({ ...base, status: 'broken', ok: false, first_break: { reason: 'new_thing', detail: '' } })
    expect(s.headline).toBe('Chain broken')
    expect(s.detail).toBe('new_thing')
  })
})

describe('auditExportQuery', () => {
  it('keeps only filled filters, trims, and encodes', () => {
    expect(auditExportQuery({ from: ' 2026-03-01 ', actor: 'a b', action: '' }, 'csv')).toBe(
      'from=2026-03-01&actor=a+b&format=csv',
    )
  })
  it('always carries the format', () => {
    expect(auditExportQuery({}, 'ndjson')).toBe('format=ndjson')
  })
})
