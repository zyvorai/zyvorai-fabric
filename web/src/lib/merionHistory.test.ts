// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest'
import type { ApprovalItem, ArtifactItem } from '../api/agents'
import { chainLabel, filterApprovals, orderPair, toggleSelection, demoIdsOf } from './merionHistory'

const art = (id: string, at: string, demo?: string): ArtifactItem => ({
  id,
  kind: 'report',
  title: id,
  created_at: at,
  metadata: demo ? { demo } : null,
})

describe('merionHistory', () => {
  it('orders a pair oldest first whichever way it was picked', () => {
    const early = art('a', '2026-09-01T00:00:00Z')
    const late = art('b', '2026-09-02T00:00:00Z')
    expect(orderPair(late, early)).toEqual([early, late])
    expect(orderPair(early, late)).toEqual([early, late])
  })

  it('keeps at most two picks and lets a third displace the oldest', () => {
    expect(toggleSelection([], 'a')).toEqual(['a'])
    expect(toggleSelection(['a'], 'b')).toEqual(['a', 'b'])
    expect(toggleSelection(['a', 'b'], 'c')).toEqual(['b', 'c'])
    expect(toggleSelection(['a', 'b'], 'a')).toEqual(['b'])
  })

  it('lists each use case once, sorted, skipping artifacts without one', () => {
    const items = [art('1', 'x', 'log-triage'), art('2', 'x', 'csv-clean'), art('3', 'x', 'log-triage'), art('4', 'x')]
    expect(demoIdsOf(items)).toEqual(['csv-clean', 'log-triage'])
  })

  it('filters approvals into pending and decided', () => {
    const mk = (status: string): ApprovalItem => ({
      id: status,
      session_id: 's',
      kind: 'send',
      prompt: 'p',
      status,
      created_at: 'x',
    })
    const all = [mk('pending'), mk('approved'), mk('denied')]
    expect(filterApprovals(all, 'pending').map((a) => a.status)).toEqual(['pending'])
    expect(filterApprovals(all, 'decided').map((a) => a.status)).toEqual(['approved', 'denied'])
    expect(filterApprovals(all, 'all')).toHaveLength(3)
  })

  it('says plainly when the chain is broken', () => {
    expect(chainLabel({ entries: 12, chain_ok: true })).toBe('Hash chain intact · 12 entries')
    expect(chainLabel({ entries: 12, chain_ok: false, broken_at: 7 })).toBe('Hash chain BROKEN at entry 7 of 12')
  })
})
