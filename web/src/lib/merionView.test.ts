// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest'
import { barShare, dropBars, egressState, timelineTone } from './merionView'

describe('egressState', () => {
  it('is sealed only for a reported zero, never for a missing count', () => {
    expect(egressState(0)).toBe('sealed')
    expect(egressState(3)).toBe('leaked')
    expect(egressState(undefined)).toBe('unknown')
    expect(egressState(null)).toBe('unknown')
    expect(egressState(-1)).toBe('unknown')
    expect(egressState(Number.NaN)).toBe('unknown')
  })
})

describe('dropBars', () => {
  it('sorts by count and scales to the widest', () => {
    const bars = dropBars([{ reason: 'a', count: 2 }, { reason: 'b', count: 8 }, { reason: 'c', count: 0 }])
    expect(bars.map((b) => b.reason)).toEqual(['b', 'a'])
    expect(bars.map((b) => b.share)).toEqual([1, 0.25])
  })
  it('gives nothing for a shape it does not know', () => {
    expect(dropBars(undefined)).toEqual([])
    expect(dropBars({ a: 1 })).toEqual([])
    expect(dropBars([{ reason: 1, count: 'x' }, null])).toEqual([])
  })
})

describe('timelineTone', () => {
  it('lets failure win over words that also look like success', () => {
    expect(timelineTone('changeset.apply Failed')).toBe('danger')
    expect(timelineTone('model budget reached for this run')).toBe('danger')
    // the words are plain English from the report, with no "fail" in them
    expect(timelineTone('changeset.apply FluxVM could not apply it: the changeset was already rejected')).toBe('danger')
    expect(timelineTone('nothing was applied: cannot reach the cell')).toBe('danger')
    expect(timelineTone('approval.approved')).toBe('proof')
    expect(timelineTone('approval.denied')).toBe('warn')
    expect(timelineTone('model.call')).toBe('neutral')
  })
})

describe('barShare', () => {
  it('clamps and handles a zero or missing cap', () => {
    expect(barShare(5, 10)).toBe(0.5)
    expect(barShare(15, 10)).toBe(1)
    expect(barShare(1, 0)).toBe(1)
    expect(barShare(1, null)).toBeNull()
  })
})
