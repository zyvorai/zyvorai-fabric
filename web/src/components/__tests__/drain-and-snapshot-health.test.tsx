// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

vi.mock('../../api/client', () => ({
  apiGet: vi.fn().mockResolvedValue({}),
  apiPost: vi.fn().mockResolvedValue({}),
}))

import { apiGet, apiPost } from '../../api/client'
import { planHostDrain, type DrainPlan } from '../../api/drainPlan'
import { getSnapshotHealth, type SnapshotHealthReport } from '../../api/snapshotHealth'
import { DrainPlanView, drainHeadline } from '../DrainPlanView'
import { SnapshotHealthView, formatAge } from '../SnapshotHealthView'

beforeEach(() => vi.clearAllMocks())

const plan = (o: Partial<DrainPlan> = {}): DrainPlan => ({
  host_id: 'h1',
  dry_run: true,
  can_drain: true,
  actions: [
    { order: 1, vm: 'web', action: 'migrate', target: 'h2', reasons: [], warnings: [] },
    { order: 2, vm: 'db', action: 'blocked', target: null, reasons: ['memory'], warnings: [] },
  ],
  summary: { migrate: 1, relocate: 0, stop: 0, blocked: 1 },
  targets: [
    {
      host_id: 'h2',
      vcpus_used_before: 0,
      vcpus_used_after: 1,
      vcpus_capacity: 8,
      memory_mb_used_before: 0,
      memory_mb_used_after: 512,
      memory_mb_capacity: 4096,
    },
  ],
  warnings: ['2 VM(s) have no host label'],
  plan_hash: 'abc123',
  ...o,
})

describe('drain plan', () => {
  it('posts to the dry-run endpoint with an encoded host id', async () => {
    await planHostDrain('a/b')
    expect(apiPost).toHaveBeenCalledWith('/api/hosts/a%2Fb/drain-plan')
  })

  it('headline reflects whether the host can drain', () => {
    expect(drainHeadline(plan({ can_drain: false }))).toBe(
      'Cannot drain: 1 migrate, 0 relocate, 0 stop, 1 blocked',
    )
    expect(drainHeadline(plan({ can_drain: true }))).toMatch(/^Can drain/)
  })

  it('renders actions, reasons, headroom, warnings and the hash', () => {
    const html = renderToStaticMarkup(<DrainPlanView plan={plan()} />)
    expect(html).toContain('web')
    expect(html).toContain('h2')
    expect(html).toContain('memory')
    expect(html).toContain('512 / 4096')
    expect(html).toContain('no host label')
    expect(html).toContain('abc123')
    expect(html).toContain('Dry run')
  })

  it('says so when the host has no VMs', () => {
    const html = renderToStaticMarkup(
      <DrainPlanView plan={plan({ actions: [], targets: [], warnings: [] })} />,
    )
    expect(html).toContain('No VMs are assigned')
  })
})

const report = (o: Partial<SnapshotHealthReport> = {}): SnapshotHealthReport => ({
  generated_at: '2026-01-01T00:00:00Z',
  default_rpo_secs: 86400,
  summary: {
    vms: 1,
    healthy: 0,
    at_risk: 0,
    breached: 1,
    no_snapshots: 0,
    without_policy: 0,
    orphaned_snapshots: 1,
    total_bytes: 2048,
    projected_bytes_30d: 4096,
  },
  vms: [
    {
      vm: 'vm1',
      status: 'breached',
      rpo_target_secs: 3600,
      rpo_source: 'default',
      newest_snapshot_at: '2025-12-30T00:00:00Z',
      age_secs: 172800,
      snapshot_count: 2,
      total_bytes: 2048,
      orphaned: ['s2'],
      has_policy: false,
      policy_enabled: false,
      growth_bytes_per_day: 10,
      projected_bytes_30d: 4096,
      findings: ['no snapshot policy', '1 snapshot(s) have a missing parent (broken chain)'],
    },
  ],
  ...o,
})

describe('snapshot health', () => {
  it('requests the report, with an optional default RPO', async () => {
    await getSnapshotHealth()
    await getSnapshotHealth(600)
    expect(apiGet).toHaveBeenNthCalledWith(1, '/api/snapshots/health')
    expect(apiGet).toHaveBeenNthCalledWith(2, '/api/snapshots/health?default_rpo_secs=600')
  })

  it('formats ages compactly', () => {
    expect(formatAge(null)).toBe('never')
    expect(formatAge(30)).toBe('30s')
    expect(formatAge(2700)).toBe('45m')
    expect(formatAge(7300)).toBe('2h')
    expect(formatAge(183600)).toBe('2d 3h')
    expect(formatAge(172800)).toBe('2d')
  })

  it('renders the scoreboard row with status, forecast and findings', () => {
    const html = renderToStaticMarkup(<SnapshotHealthView report={report()} />)
    expect(html).toContain('vm1')
    expect(html).toContain('RPO breached')
    expect(html).toContain('(default)')
    expect(html).toContain('4.0 KB')
    expect(html).toContain('broken chain')
    expect(html).toContain('1 breached')
  })

  it('says so when there are no VMs', () => {
    const html = renderToStaticMarkup(
      <SnapshotHealthView report={report({ vms: [] })} />,
    )
    expect(html).toContain('No VMs to assess')
  })
})
