// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect } from 'vitest'
import { computeAuditInsights, insightsHeadline, type AuditEntry } from './auditInsights'

let n = 0
// 2026-03-02 is a Monday.
const ev = (ts: string, user: string, action: string, over: Partial<AuditEntry> = {}): AuditEntry => ({
  id: `e${++n}`,
  timestamp: ts,
  user,
  action,
  resource_type: 'vm',
  resource_name: 'vm-1',
  status: 'success',
  ip_address: '10.0.0.1',
  ...over,
})
const at = (hhmm: string, day = '2026-03-02') => `${day}T${hhmm}:00Z`

/** A calm log: a few events by known actors during working hours across several days. */
const calmLog = (): AuditEntry[] => [
  ev(at('09:00', '2026-02-27'), 'alice', 'vm.start'),
  ev(at('10:00', '2026-02-27'), 'bob', 'vm.stop', { ip_address: '10.0.0.2' }),
  ev(at('09:30', '2026-03-02'), 'alice', 'vm.start'),
  ev(at('11:00', '2026-03-02'), 'bob', 'vm.create', { ip_address: '10.0.0.2' }),
  ev(at('14:00', '2026-03-02'), 'alice', 'login'),
]

describe('computeAuditInsights', () => {
  it('yields nothing for normal activity', () => {
    expect(computeAuditInsights(calmLog())).toEqual([])
    expect(insightsHeadline([])).toBe('No unusual activity found')
  })

  it('yields nothing for empty or unparseable input', () => {
    expect(computeAuditInsights([])).toEqual([])
    expect(computeAuditInsights([ev('not-a-date', 'a', 'vm.start')])).toEqual([])
  })

  it('detects a burst by one actor and not one just under the threshold', () => {
    const burst = Array.from({ length: 20 }, (_, i) => ev(`2026-03-02T10:0${Math.floor(i / 10)}:${String((i % 10) * 5).padStart(2, '0')}Z`, 'mallory', 'vm.list'))
    const found = computeAuditInsights([...calmLog(), ...burst]).filter((i) => i.kind === 'burst')
    expect(found).toHaveLength(1)
    expect(found[0].actor).toBe('mallory')
    expect(found[0].count).toBe(20)
    const almost = burst.slice(0, 19)
    expect(computeAuditInsights([...calmLog(), ...almost]).filter((i) => i.kind === 'burst')).toEqual([])
  })

  it('does not call a slow trickle a burst', () => {
    const slow = Array.from({ length: 30 }, (_, i) => ev(`2026-03-02T${String(9 + Math.floor(i / 6)).padStart(2, '0')}:${String((i % 6) * 10).padStart(2, '0')}:00Z`, 'carol', 'vm.list'))
    expect(computeAuditInsights(slow).filter((i) => i.kind === 'burst')).toEqual([])
  })

  it('respects explicit burst thresholds', () => {
    const three = [0, 1, 2].map((m) => ev(at(`10:0${m}`), 'dave', 'vm.list'))
    const r = computeAuditInsights(three, { burstThreshold: 3, burstWindowMinutes: 5 })
    expect(r.some((i) => i.kind === 'burst')).toBe(true)
  })

  it('detects a failed-auth streak and escalates a long one', () => {
    const fails = (k: number) => Array.from({ length: k }, (_, i) => ev(at(`10:0${i}`), 'eve', 'login', { status: 'failed' }))
    const warn = computeAuditInsights([...calmLog(), ...fails(5)]).filter((i) => i.kind === 'auth_failures')
    expect(warn).toHaveLength(1)
    expect(warn[0].severity).toBe('warning')
    const crit = computeAuditInsights(fails(10)).filter((i) => i.kind === 'auth_failures')
    expect(crit[0].severity).toBe('critical')
    expect(insightsHeadline(crit)).toBe('1 critical')
  })

  it('does not flag short or interrupted failure streaks', () => {
    const four = Array.from({ length: 4 }, (_, i) => ev(at(`10:0${i}`), 'eve', 'login', { status: 'failed' }))
    expect(computeAuditInsights(four)).toEqual([])
    const broken = [
      ...Array.from({ length: 3 }, (_, i) => ev(at(`10:0${i}`), 'eve', 'login', { status: 'failed' })),
      ev(at('10:05'), 'eve', 'login'),
      ...Array.from({ length: 3 }, (_, i) => ev(at(`10:1${i}`), 'eve', 'login', { status: 'failed' })),
    ]
    expect(computeAuditInsights(broken).filter((i) => i.kind === 'auth_failures')).toEqual([])
  })

  it('does not count failed non-auth actions as a sign-in streak', () => {
    const failedStarts = Array.from({ length: 8 }, (_, i) => ev(at(`10:0${i}`), 'frank', 'vm.start', { status: 'failed' }))
    expect(computeAuditInsights(failedStarts).filter((i) => i.kind === 'auth_failures')).toEqual([])
  })

  it('flags privileged actions at night and on weekends, not ordinary ones', () => {
    const night = ev(at('02:30'), 'grace', 'user.delete')
    const weekend = ev(at('11:00', '2026-03-01'), 'grace', 'role.update') // Sunday
    const ordinaryNight = ev(at('03:00'), 'heidi', 'vm.start')
    const found = computeAuditInsights([...calmLog(), night, weekend, ordinaryNight]).filter((i) => i.kind === 'off_hours_privileged')
    expect(found).toHaveLength(1)
    expect(found[0].actor).toBe('grace')
    expect(found[0].count).toBe(2)
  })

  it('honours the configured timezone offset and can be disabled', () => {
    // 02:30 UTC is 08:00 in UTC+5:30, inside an 8-20 working day.
    const e = [ev(at('02:30'), 'ivan', 'user.delete')]
    expect(computeAuditInsights(e, { workingHours: { startHour: 8, endHour: 20, workDays: [1, 2, 3, 4, 5], utcOffsetMinutes: 330 } })).toEqual([])
    expect(computeAuditInsights(e, { workingHours: null })).toEqual([])
    expect(computeAuditInsights(e).some((i) => i.kind === 'off_hours_privileged')).toBe(true)
  })

  it('flags a first-seen actor and IP only when older history exists', () => {
    const log = [...calmLog(), ev(at('15:00'), 'newbie', 'vm.start', { ip_address: '203.0.113.9' })]
    const found = computeAuditInsights(log)
    expect(found.map((i) => i.kind).sort()).toEqual(['new_actor', 'new_ip'])
    expect(found.every((i) => i.severity === 'info')).toBe(true)
    // Whole log inside the window: everyone is "new", so report nothing.
    const fresh = [ev(at('09:00'), 'a', 'vm.start'), ev(at('10:00'), 'b', 'vm.start', { ip_address: '10.9.9.9' })]
    expect(computeAuditInsights(fresh)).toEqual([])
  })

  it('is deterministic and does not mutate input', () => {
    const log = [...calmLog(), ev(at('15:00'), 'newbie', 'vm.start', { ip_address: '203.0.113.9' })]
    const copy = JSON.stringify(log)
    const a = computeAuditInsights(log)
    const b = computeAuditInsights([...log].reverse())
    expect(a).toEqual(b)
    expect(JSON.stringify(log)).toBe(copy)
  })

  it('orders critical before warning before info', () => {
    const fails = Array.from({ length: 10 }, (_, i) => ev(at(`10:0${i}`), 'eve', 'login', { status: 'failed' }))
    const log = [...calmLog(), ...fails, ev(at('03:00'), 'grace', 'user.delete'), ev(at('15:00'), 'newbie', 'vm.start')]
    const order = computeAuditInsights(log).map((i) => i.severity)
    expect(order).toEqual([...order].sort((a, b) => ['critical', 'warning', 'info'].indexOf(a) - ['critical', 'warning', 'info'].indexOf(b)))
    expect(insightsHeadline(computeAuditInsights(log))).toMatch(/critical/)
  })
})
