// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect } from 'vitest'
import {
  describeChange,
  formatValue,
  groupChanges,
  isRedacted,
  REDACTED,
  sectionLabel,
  summarizeChanges,
  type SnapshotChange,
} from './snapshotDiff'

const changes: SnapshotChange[] = [
  { section: 'storage_pools', key: 'p1', kind: 'changed', field: 'size', old: 1, new: 2 },
  { section: 'vms', key: 'web', kind: 'changed', field: 'cpus', old: 2, new: 4 },
  { section: 'vms', key: 'web', kind: 'changed', field: 'mem.mb', old: 512, new: 1024 },
  { section: 'vms', key: 'db', kind: 'added', new: { name: 'db' } },
  { section: 'network_policies', key: 'pol', kind: 'removed', old: { id: 'pol' } },
  { section: 'zzz', key: 'a', kind: 'added' },
]

describe('summarizeChanges', () => {
  it('counts by kind', () => {
    expect(summarizeChanges(changes)).toEqual({ added: 2, removed: 1, changed: 3, total: 6 })
  })
  it('handles empty', () => {
    expect(summarizeChanges([])).toEqual({ added: 0, removed: 0, changed: 0, total: 0 })
  })
})

describe('groupChanges', () => {
  it('groups per entity and orders known sections first, then key', () => {
    const g = groupChanges(changes)
    expect(g.map((x) => `${x.section}/${x.key}`)).toEqual([
      'vms/db',
      'vms/web',
      'network_policies/pol',
      'storage_pools/p1',
      'zzz/a',
    ])
    expect(g[1].changes.map((c) => c.field)).toEqual(['cpus', 'mem.mb'])
    expect(g[0].kind).toBe('added')
    expect(g[2].kind).toBe('removed')
  })
  it('does not mutate the input and handles empty', () => {
    const copy = JSON.parse(JSON.stringify(changes))
    groupChanges(changes)
    expect(changes).toEqual(copy)
    expect(groupChanges([])).toEqual([])
  })
  it('does not merge the same key in different sections', () => {
    const g = groupChanges([
      { section: 'vms', key: 'x', kind: 'added' },
      { section: 'storage_pools', key: 'x', kind: 'added' },
    ])
    expect(g).toHaveLength(2)
  })
})

describe('formatValue', () => {
  it('renders scalars, objects and missing values', () => {
    expect(formatValue('a')).toBe('a')
    expect(formatValue(3)).toBe('3')
    expect(formatValue(null)).toBe('null')
    expect(formatValue({ a: 1 })).toBe('{"a":1}')
    expect(formatValue(undefined)).toBe('(none)')
  })
  it('truncates long values', () => {
    const out = formatValue('x'.repeat(200), 10)
    expect(out).toHaveLength(10)
    expect(out.endsWith('…')).toBe(true)
  })
  it('does not throw on unserialisable values', () => {
    const a: Record<string, unknown> = {}
    a.self = a
    expect(() => formatValue(a)).not.toThrow()
  })
})

describe('describeChange and helpers', () => {
  it('describes each kind', () => {
    expect(describeChange(changes[1])).toBe('cpus: 2 → 4')
    expect(describeChange(changes[3])).toBe('added db')
    expect(describeChange(changes[4])).toBe('removed pol')
  })
  it('knows redaction and section labels', () => {
    expect(isRedacted(REDACTED)).toBe(true)
    expect(isRedacted('x')).toBe(false)
    expect(sectionLabel('vms')).toBe('Virtual machines')
    expect(sectionLabel('other')).toBe('other')
  })
})
