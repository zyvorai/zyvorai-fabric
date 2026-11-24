// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

vi.mock('../../api/client', () => ({
  apiGet: vi.fn().mockResolvedValue({}),
  apiPost: vi.fn().mockResolvedValue({}),
  apiPut: vi.fn().mockResolvedValue({}),
  apiDelete: vi.fn().mockResolvedValue(undefined),
}))

import { apiGet, apiPost, apiPut, apiDelete } from '../../api/client'
import {
  clearOwnership,
  extendLease,
  getOwnershipReport,
  getReclaimReport,
  getSnapshotHygiene,
  setOwnership,
  type HygieneReport,
  type OwnershipReport,
  type ReclaimReport,
} from '../../api/housekeeping'
import {
  OwnershipReportView,
  ReclaimView,
  SnapshotHygieneView,
  formatHours,
  formatLease,
} from '../HousekeepingViews'

beforeEach(() => vi.clearAllMocks())

const hygiene: HygieneReport = {
  generated_at: '2026-06-01T00:00:00Z',
  max_age_hours: 72,
  max_chain: 3,
  summary: {
    vms_with_snapshots: 1,
    ok: 0,
    warning: 1,
    critical: 0,
    stale_snapshots: 2,
    protected_stale: 1,
    total_bytes: 3072,
    reclaimable_bytes: 1024,
  },
  vms: [
    {
      vm: 'web',
      status: 'warning',
      snapshot_count: 3,
      total_bytes: 3072,
      oldest_age_hours: 200,
      max_chain_depth: 4,
      orphaned: [],
      stale: [],
      reclaimable_bytes: 1024,
      findings: ['1 snapshot(s) older than 72h and not tagged keep'],
    },
  ],
}

describe('api calls', () => {
  it('builds query strings only for given thresholds', async () => {
    await getSnapshotHygiene()
    expect(apiGet).toHaveBeenLastCalledWith('/api/snapshots/hygiene')
    await getSnapshotHygiene(24, 2)
    expect(apiGet).toHaveBeenLastCalledWith('/api/snapshots/hygiene?max_age_hours=24&max_chain=2')
    await getReclaimReport(60)
    expect(apiGet).toHaveBeenLastCalledWith('/api/reclaim/report?stopped_days=60')
    await getOwnershipReport(7)
    expect(apiGet).toHaveBeenLastCalledWith('/api/ownership/report?within_days=7')
  })

  it('encodes VM names for ownership writes', async () => {
    await setOwnership('a/b', { owner: 'x' })
    expect(apiPut).toHaveBeenCalledWith('/api/vms/a%2Fb/ownership', { owner: 'x' })
    await extendLease('a/b', 30)
    expect(apiPost).toHaveBeenCalledWith('/api/vms/a%2Fb/ownership/extend', { days: 30 })
    await clearOwnership('a/b')
    expect(apiDelete).toHaveBeenCalledWith('/api/vms/a%2Fb/ownership')
  })
})

describe('formatters', () => {
  it('formats hours and lease days', () => {
    expect(formatHours(null)).toBe('none')
    expect(formatHours(5)).toBe('5h')
    expect(formatHours(72)).toBe('3d')
    expect(formatHours(76)).toBe('3d 4h')
    expect(formatLease(null)).toBe('no expiry')
    expect(formatLease(-1)).toBe('1 day ago')
    expect(formatLease(-3)).toBe('3 days ago')
    expect(formatLease(0)).toBe('today')
    expect(formatLease(1)).toBe('in 1 day')
  })
})

describe('views', () => {
  it('renders hygiene summary and rows', () => {
    const html = renderToStaticMarkup(<SnapshotHygieneView report={hygiene} />)
    expect(html).toContain('0 critical, 1 need cleanup')
    expect(html).toContain('Needs cleanup')
    expect(html).toContain('web')
    expect(html).toContain('8d 8h')
  })

  it('says so when no VM has snapshots', () => {
    const html = renderToStaticMarkup(
      <SnapshotHygieneView report={{ ...hygiene, vms: [] }} />,
    )
    expect(html).toContain('No VM has snapshots')
  })

  it('renders ownership rows with an edit button when selectable', () => {
    const report: OwnershipReport = {
      generated_at: '',
      within_days: 14,
      summary: { vms: 2, owned: 0, expiring: 0, expired: 1, unowned: 1 },
      vms: [
        { vm: 'a', status: 'expired', state: 'Stopped', owner: 'bob', contact: null, purpose: null, expires_at: '2026-05-01T00:00:00Z', days_to_expiry: -3 },
        { vm: 'b', status: 'unowned', state: 'Running', owner: null, contact: null, purpose: null, expires_at: null, days_to_expiry: null },
      ],
    }
    const html = renderToStaticMarkup(<OwnershipReportView report={report} onSelect={() => {}} />)
    expect(html).toContain('Lease expired')
    expect(html).toContain('3 days ago')
    expect(html).toContain('No owner')
    expect(html).toContain('Edit a')
    expect(renderToStaticMarkup(<OwnershipReportView report={report} />)).not.toContain('Edit a')
  })

  it('renders reclaim candidates and the empty state', () => {
    const report: ReclaimReport = {
      generated_at: '',
      stopped_days: 30,
      snapshot_age_hours: 72,
      summary: {
        candidates: 1,
        idle_vms: 1,
        expired_leases: 0,
        unowned_candidates: 1,
        reclaimable_disk_gb: 40,
        reclaimable_snapshot_bytes_candidates: 0,
        stale_snapshot_bytes_total: 2048,
      },
      candidates: [
        {
          vm: 'old',
          state: 'Stopped',
          reasons: ['Stopped with no recorded change for 90 days'],
          idle_basis: 'updated',
          idle_days: 90,
          disk_gb: 40,
          reclaimable_disk_gb: 40,
          reclaimable_snapshot_bytes: 0,
          owner: null,
          contact: null,
          lease_expires_at: null,
        },
      ],
    }
    const html = renderToStaticMarkup(<ReclaimView report={report} />)
    expect(html).toContain('old')
    expect(html).toContain('40 GB')
    expect(html).toContain('Nothing is deleted')
    const empty = renderToStaticMarkup(
      <ReclaimView report={{ ...report, candidates: [], summary: { ...report.summary, candidates: 0 } }} />,
    )
    expect(empty).toContain('Nothing to reclaim')
  })
})
