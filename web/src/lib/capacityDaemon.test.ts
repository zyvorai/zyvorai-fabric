// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect } from 'vitest'
import fx from './fixtures/system-capacity.json'
import { densityReport, hostFromMetrics, metricsFromCapacity, type CapacityResponseLike } from './capacity'

const f = fx as Record<string, CapacityResponseLike>

describe('metricsFromCapacity (daemon /api/system/capacity shape)', () => {
  it('maps allocated and effective to used and total', () => {
    const m = metricsFromCapacity(f.overcommitted)
    expect(m).toEqual([
      { label: 'CPU Cores', used: 60, total: 32, unit: 'cores', trend: 0 },
      { label: 'Memory', used: 88, total: 96, unit: 'GB', trend: 0 },
    ])
  })
  it('feeds the what-if local fallback', () => {
    const model = hostFromMetrics(metricsFromCapacity(f.plain))
    expect(model?.host).toEqual({ name: 'local', vcpus: 16, memory_mb: 65536, disk_gb: null })
    expect(model?.existing[0]).toMatchObject({ vcpus: 10, memory_mb: 40960 })
  })
})

describe('densityReport', () => {
  it('is ok for a lightly used host without overcommit', () => {
    const r = densityReport(f.plain)
    expect(r.rows.map((x) => x.level)).toEqual(['ok', 'ok'])
    expect(r.rows[0]).toMatchObject({ commit_ratio: 0.625, limit_ratio: 1, utilization_pct: 62.5, headroom: 6 })
    expect(r.vcpus_per_vm).toBe(2)
    expect(r.memory_gb_per_vm).toBe(8)
    expect(r.more_vms_fit).toBe(3)
  })
  it('flags CPU over the policy limit and memory near it', () => {
    const r = densityReport(f.overcommitted)
    expect(r.rows[0].level).toBe('critical')
    expect(r.rows[1].level).toBe('warn')
    expect(r.rows[1].warning).toContain('near the 1.50:1 limit')
    expect(r.more_vms_fit).toBe(0)
  })
  it('flags allocation beyond physical when overcommit is disabled', () => {
    const r = densityReport(f.beyond_physical_no_policy)
    expect(r.rows[0].level).toBe('critical')
    expect(r.rows[0].warning).toContain('overcommit is disabled')
    expect(r.rows[1].level).toBe('ok')
  })
  it('handles an empty host', () => {
    const r = densityReport(f.empty)
    expect(r.vcpus_per_vm).toBeNull()
    expect(r.more_vms_fit).toBeNull()
    expect(r.rows.every((x) => x.level === 'ok')).toBe(true)
  })
})
