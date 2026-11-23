// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect } from 'vitest'
import {
  DEFAULT_FORM,
  describeNextRun,
  formToInput,
  hasKeep,
  policyToForm,
  toggleKeep,
  validateForm,
} from './snapshotPolicy'
import type { SnapshotPolicy } from '../api/snapshotPolicy'

describe('validateForm', () => {
  it('accepts the defaults', () => {
    expect(validateForm(DEFAULT_FORM)).toEqual([])
  })
  it('requires a retention rule', () => {
    const errs = validateForm({ ...DEFAULT_FORM, keepLast: '', maxAgeHours: '' })
    expect(errs.join(' ')).toContain('retention')
  })
  it('rejects zero, fractional and junk numbers', () => {
    expect(validateForm({ ...DEFAULT_FORM, keepLast: '0' }).join(' ')).toContain('Keep last')
    expect(validateForm({ ...DEFAULT_FORM, intervalMinutes: '1.5' }).join(' ')).toContain('Interval')
    expect(validateForm({ ...DEFAULT_FORM, maxAgeHours: 'x' }).join(' ')).toContain('Max age')
  })
  it('checks cron field count and prefix', () => {
    const cron = { ...DEFAULT_FORM, scheduleKind: 'cron' as const }
    expect(validateForm({ ...cron, cron: '* * * *' }).join(' ')).toContain('5 fields')
    expect(validateForm({ ...cron, cron: '0 3 * * *' })).toEqual([])
    expect(validateForm({ ...DEFAULT_FORM, namePrefix: 'bad prefix' }).join(' ')).toContain('Name prefix')
    expect(validateForm({ ...DEFAULT_FORM, namePrefix: '' }).join(' ')).toContain('Name prefix')
  })
})

describe('formToInput / policyToForm', () => {
  it('converts minutes and hours to seconds, exclusive schedule', () => {
    const i = formToInput({ ...DEFAULT_FORM, intervalMinutes: '30', keepLast: '', maxAgeHours: '24' })
    expect(i.interval_secs).toBe(1800)
    expect(i.cron).toBeNull()
    expect(i.keep_last).toBeNull()
    expect(i.max_age_secs).toBe(86400)
    const c = formToInput({ ...DEFAULT_FORM, scheduleKind: 'cron', cron: ' 0 3 * * * ' })
    expect(c.interval_secs).toBeNull()
    expect(c.cron).toBe('0 3 * * *')
  })
  it('round-trips a stored policy', () => {
    const p: SnapshotPolicy = {
      vm_name: 'vm',
      updated_at: '2026-03-01T00:00:00Z',
      enabled: false,
      cron: '0 3 * * *',
      keep_last: 5,
      max_age_secs: 7200,
      name_prefix: 'nightly',
      snapshot_type: 'Full',
    }
    const f = policyToForm(p)
    expect(f.scheduleKind).toBe('cron')
    expect(f.maxAgeHours).toBe('2')
    const back = formToInput(f)
    expect(back).toMatchObject({ enabled: false, cron: '0 3 * * *', keep_last: 5, max_age_secs: 7200, name_prefix: 'nightly', snapshot_type: 'Full' })
    expect(policyToForm(null)).toEqual(DEFAULT_FORM)
  })
})

describe('describeNextRun', () => {
  const now = new Date('2026-03-01T10:00:00Z')
  it('formats future, due and disabled', () => {
    expect(describeNextRun('2026-03-01T10:45:00Z', true, now)).toBe('2026-03-01 10:45 UTC (in 45 min)')
    expect(describeNextRun('2026-03-01T15:00:00Z', true, now)).toContain('in 5 h')
    expect(describeNextRun('2026-03-01T09:00:00Z', true, now)).toContain('Due now')
    expect(describeNextRun(null, true, now)).toBe('Not scheduled')
    expect(describeNextRun('2026-03-01T15:00:00Z', false, now)).toContain('disabled')
  })
})

describe('keep tag helpers', () => {
  it('toggles keep and keeps other tags', () => {
    expect(hasKeep(['a', 'keep'])).toBe(true)
    expect(hasKeep(undefined)).toBe(false)
    expect(toggleKeep(['a'])).toEqual(['a', 'keep'])
    expect(toggleKeep(undefined)).toEqual(['keep'])
    expect(toggleKeep(['keep', 'a'])).toEqual(['a'])
  })
})
