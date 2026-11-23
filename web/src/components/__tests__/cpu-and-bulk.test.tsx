// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

vi.mock('../../api/client', () => ({
  apiGet: vi.fn().mockResolvedValue({}),
  apiPost: vi.fn().mockResolvedValue({}),
}))

import { apiGet, apiPost } from '../../api/client'
import { describeVerdict, getCpuCompat, getCpuInventory, type CpuCompat, type CpuInventory } from '../../api/cpuCompat'
import { applyBulk, bulkHeadline, planBulk, type BulkPlan } from '../../api/bulkOps'
import { CpuCompatView, CpuInventoryView } from '../CpuCompatView'
import { BulkPlanView, BulkResultView } from '../BulkPlanView'

beforeEach(() => vi.clearAllMocks())

const inv: CpuInventory = {
  hosts: [
    {
      host_id: 'h1',
      hostname: 'alpha',
      cluster_id: 'c1',
      cpu: { host_id: 'h1', vendor: 'GenuineIntel', model_name: 'Xeon', flags: ['avx2'], source: 'reported', updated: '' },
    },
    { host_id: 'h2', hostname: 'beta', cluster_id: 'c1', cpu: null },
  ],
  clusters: [{ cluster_id: 'c1', hosts_total: 2, hosts_known: 1, baseline_model: 'Haswell', common_features: 3 }],
  vms: [{ name: 'v', configured: true, model: 'Haswell', mode: 'custom' }, { name: 'w', configured: false }],
  vms_truncated: false,
}

describe('cpu compatibility', () => {
  it('calls the read-only endpoints with encoded names', async () => {
    await getCpuInventory()
    expect(apiGet).toHaveBeenCalledWith('/api/cpu/inventory')
    await getCpuCompat('a/b', 'h 1')
    expect(apiGet).toHaveBeenCalledWith('/api/vms/a%2Fb/cpu-compat?host=h%201')
    await getCpuCompat('v')
    expect(apiGet).toHaveBeenLastCalledWith('/api/vms/v/cpu-compat')
  })

  it('describes verdicts', () => {
    expect(describeVerdict({ verdict: 'compatible' })).toBe('Compatible')
    expect(describeVerdict({ verdict: 'unknown' })).toMatch(/^Unknown/)
    expect(describeVerdict({ verdict: 'incompatible', missing: ['avx2'], vendor_mismatch: true })).toBe(
      'Incompatible: CPU vendor differs; missing avx2',
    )
  })

  it('renders hosts, baseline and unknown hosts', () => {
    const html = renderToStaticMarkup(<CpuInventoryView inv={inv} />)
    expect(html).toContain('alpha')
    expect(html).toContain('GenuineIntel Xeon (1 flags, reported)')
    expect(html).toContain('No CPU inventory recorded')
    expect(html).toContain('Haswell')
    expect(html).toContain('VMs with a stored CPU model: 1')
  })

  it('renders per-host results and the unconfigured case', () => {
    const c: CpuCompat = {
      vm: 'v',
      configured: true,
      mode: 'custom',
      model: 'Haswell',
      requirement: { features: ['avx2'], notes: ['a note'] },
      results: [{ host_id: 'h2', hostname: 'beta', result: { verdict: 'incompatible', missing: ['avx2'], vendor_mismatch: false } }],
    }
    const html = renderToStaticMarkup(<CpuCompatView compat={c} />)
    expect(html).toContain('a note')
    expect(html).toContain('Incompatible: missing avx2')
    const none = renderToStaticMarkup(<CpuCompatView compat={{ ...c, configured: false }} />)
    expect(none).toContain('nothing to check')
  })
})

const plan = (o: Partial<BulkPlan> = {}): BulkPlan => ({
  action: 'stop',
  items: [
    { vm: 'a', state: 'running', will: 'stop' },
    { vm: 'b', state: 'stopped', will: 'skip', reason: 'not_running' },
  ],
  summary: { matched: 2, actionable: 1, skipped: 1 },
  max_vms: 10,
  over_cap: false,
  plan_hash: 'deadbeef',
  ...o,
})

describe('bulk operations', () => {
  it('plans as a dry run and applies with the plan hash', async () => {
    await planBulk({ selector: { tags: ['web'] }, action: 'stop' })
    expect(apiPost).toHaveBeenCalledWith('/api/vms/bulk', {
      selector: { tags: ['web'] },
      action: 'stop',
      dry_run: true,
    })
    await applyBulk({ selector: { tags: ['web'] }, action: 'stop', plan_hash: 'h' })
    expect(apiPost).toHaveBeenLastCalledWith('/api/vms/bulk', {
      selector: { tags: ['web'] },
      action: 'stop',
      plan_hash: 'h',
      dry_run: false,
    })
  })

  it('headline mentions the cap', () => {
    expect(bulkHeadline(plan())).toBe('2 matched, 1 to stop, 1 skipped')
    expect(bulkHeadline(plan({ over_cap: true }))).toContain('over the cap of 10')
  })

  it('renders the plan with reasons and the hash', () => {
    const html = renderToStaticMarkup(<BulkPlanView plan={plan()} />)
    expect(html).toContain('not_running')
    expect(html).toContain('deadbeef')
    expect(html).toContain('nothing was changed')
  })

  it('renders per-VM results with errors', () => {
    const html = renderToStaticMarkup(
      <BulkResultView
        result={{
          dry_run: false,
          plan_hash: 'h',
          action: 'stop',
          summary: { ok: 1, failed: 1, skipped: 0, not_attempted: 0, vms: 2 },
          results: [
            { vm: 'a', status: 'ok' },
            { vm: 'b', status: 'failed', error: 'boom' },
          ],
        }}
      />,
    )
    expect(html).toContain('1 ok, 1 failed')
    expect(html).toContain('boom')
  })
})
