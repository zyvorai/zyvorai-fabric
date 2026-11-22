// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect } from 'vitest'
import fixtures from './fixtures/capacity-whatif.json'
import { fitVms, hostFromMetrics, parseRows, reasonText, summarizeReport, EMPTY_ROW, NO_OVERCOMMIT } from './capacity'
import type { WhatIfHost, ExistingVm, NewVmSpec, Overcommit, FitReport } from './capacity'

type Case = {
  name: string
  input: { hosts: WhatIfHost[]; existing: ExistingVm[]; new_vms: NewVmSpec[]; overcommit: Overcommit }
  expected: FitReport
}

describe('fitVms shared fixtures (also read by the Rust tests)', () => {
  for (const c of (fixtures as { cases: Case[] }).cases) {
    it(c.name, () => {
      const { hosts, existing, new_vms, overcommit } = c.input
      expect(fitVms(hosts, existing, new_vms, overcommit)).toEqual(c.expected)
    })
  }
})

describe('fitVms properties', () => {
  const host: WhatIfHost = { name: 'h', vcpus: 4, memory_mb: 4096, disk_gb: 100 }
  const spec = (name: string, count: number): NewVmSpec => ({ name, vcpus: 1, memory_mb: 1024, disk_gb: 10, count })

  it('is deterministic regardless of input order and does not mutate inputs', () => {
    const a = [spec('a', 2), spec('b', 2)]
    const snapshot = JSON.stringify(a)
    const r1 = fitVms([host], [], a, NO_OVERCOMMIT)
    const r2 = fitVms([host], [], [...a].reverse(), NO_OVERCOMMIT)
    expect(r1).toEqual(r2)
    expect(JSON.stringify(a)).toBe(snapshot)
  })

  it('reports no_hosts with no hosts', () => {
    expect(fitVms([], [], [spec('a', 1)], NO_OVERCOMMIT).unplaced[0].reasons).toEqual(['no_hosts'])
  })
})

describe('hostFromMetrics', () => {
  it('converts GB memory and keeps cores and storage', () => {
    const m = hostFromMetrics([
      { label: 'CPU Cores', used: 2, total: 8, unit: 'cores' },
      { label: 'Memory', used: 4, total: 16, unit: 'GB' },
      { label: 'Storage', used: 50, total: 500, unit: 'GB' },
    ])
    expect(m?.host).toEqual({ name: 'local', vcpus: 8, memory_mb: 16384, disk_gb: 500 })
    expect(m?.existing[0]).toEqual({ host: null, vcpus: 2, memory_mb: 4096, disk_gb: 50 })
  })
  it('returns null when cpu or memory totals are unknown', () => {
    expect(hostFromMetrics([{ label: 'CPU Cores', used: 0, total: 0, unit: 'cores' }])).toBeNull()
    expect(hostFromMetrics([])).toBeNull()
  })
  it('treats a zero storage total as unknown disk', () => {
    const m = hostFromMetrics([
      { label: 'CPU Cores', used: 0, total: 4, unit: 'cores' },
      { label: 'Memory', used: 0, total: 8, unit: 'GB' },
      { label: 'Storage', used: 0, total: 0, unit: 'GB' },
    ])
    expect(m?.host.disk_gb).toBeNull()
  })
})

describe('parseRows', () => {
  it('accepts a valid row', () => {
    expect(parseRows([{ ...EMPTY_ROW, name: 'web', count: '3' }])).toEqual({
      specs: [{ name: 'web', vcpus: 2, memory_mb: 4096, disk_gb: 40, count: 3 }],
    })
  })
  it('rejects missing name, non-numbers, zero and oversized counts, and empty lists', () => {
    expect('error' in parseRows([{ ...EMPTY_ROW }])).toBe(true)
    expect('error' in parseRows([{ ...EMPTY_ROW, name: 'a', vcpus: 'x' }])).toBe(true)
    expect('error' in parseRows([{ ...EMPTY_ROW, name: 'a', count: '0' }])).toBe(true)
    expect('error' in parseRows([{ ...EMPTY_ROW, name: 'a', count: '1001' }])).toBe(true)
    expect('error' in parseRows([])).toBe(true)
  })
})

describe('text helpers', () => {
  it('describes reasons and summaries', () => {
    expect(reasonText(['vcpu', 'disk'])).toBe('not enough vCPUs, not enough disk')
    const [, c2] = (fixtures as { cases: Case[] }).cases
    expect(summarizeReport(c2.expected)).toBe('3 of 4 VMs fit')
  })
})
