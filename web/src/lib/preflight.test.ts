// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, expect, it } from 'vitest'
import { groupFindings, preflightVerdict, type PreflightFinding } from './preflight'

const f = (id: string, severity: PreflightFinding['severity']): PreflightFinding => ({
  id,
  severity,
  message: `${id} message`,
  hint: `${id} hint`,
})

describe('groupFindings', () => {
  it('orders blockers, warnings, then notes regardless of input order', () => {
    const groups = groupFindings([f('i', 'info'), f('w', 'warning'), f('b', 'blocker'), f('b2', 'blocker')])
    expect(groups.map((g) => g.severity)).toEqual(['blocker', 'warning', 'info'])
    expect(groups[0].findings.map((x) => x.id)).toEqual(['b', 'b2'])
    expect(groups[2].label).toBe('Notes')
  })
  it('drops empty groups and handles no findings', () => {
    expect(groupFindings([])).toEqual([])
    expect(groupFindings([f('w', 'warning')]).map((g) => g.severity)).toEqual(['warning'])
  })
  it('files an unknown severity under notes rather than losing it', () => {
    const odd = { ...f('x', 'info'), severity: 'critical' } as unknown as PreflightFinding
    const groups = groupFindings([odd])
    expect(groups).toHaveLength(1)
    expect(groups[0].severity).toBe('info')
  })
})

describe('preflightVerdict', () => {
  it.each([
    [[], 'ok', 'Ready to start: no problems found'],
    [[f('i', 'info')], 'ok', 'Ready to start: no problems found'],
    [[f('w', 'warning')], 'warn', 'Can start, with 1 warning'],
    [[f('w', 'warning'), f('w2', 'warning')], 'warn', 'Can start, with 2 warnings'],
    [[f('b', 'blocker')], 'blocked', 'Cannot start: 1 blocker'],
    [[f('b', 'blocker'), f('b2', 'blocker'), f('w', 'warning')], 'blocked', 'Cannot start: 2 blockers, 1 warning'],
  ] as const)('%j -> %s', (findings, tone, text) => {
    expect(preflightVerdict([...findings])).toEqual({ tone, text })
  })
})
