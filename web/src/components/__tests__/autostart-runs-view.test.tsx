// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

vi.mock('../../api/client', () => ({
  apiGet: vi.fn().mockResolvedValue({}),
  apiPost: vi.fn().mockResolvedValue({}),
}))

import { apiGet, apiPost } from '../../api/client'
import { getAutostartRun, listAutostartRuns, startAutostartRun, type AutostartRun, type AutostartRunSummary } from '../../api/autostart'
import { AutostartRunsView, runHeadline } from '../AutostartRunsView'

beforeEach(() => vi.clearAllMocks())

const summary = (o: Partial<AutostartRunSummary> = {}): AutostartRunSummary => ({
  id: 'r1',
  trigger: 'manual',
  state: 'completed',
  started_at: '2026-01-01T00:00:00Z',
  candidates: 3,
  started: 1,
  skipped: 1,
  failed: 1,
  ...o,
})

describe('autostart api', () => {
  it('lists, reads and starts runs', async () => {
    await listAutostartRuns()
    await getAutostartRun('a/b')
    await startAutostartRun()
    expect(apiGet).toHaveBeenNthCalledWith(1, '/api/vms/autostart/reconcile')
    expect(apiGet).toHaveBeenNthCalledWith(2, '/api/vms/autostart/reconcile/a%2Fb')
    expect(apiPost).toHaveBeenCalledWith('/api/vms/autostart/reconcile')
  })
})

describe('AutostartRunsView', () => {
  it('says so when there are no runs', () => {
    const html = renderToStaticMarkup(<AutostartRunsView runs={[]} selected={null} onSelect={() => {}} />)
    expect(html).toContain('No autostart runs yet')
  })

  it('lists runs with their result and asks to pick one', () => {
    const html = renderToStaticMarkup(<AutostartRunsView runs={[summary()]} selected={null} onSelect={() => {}} />)
    expect(html).toContain('1 started, 1 skipped, 1 failed')
    expect(html).toContain('Select a run')
  })

  it('labels a running run with progress', () => {
    expect(runHeadline(summary({ state: 'running' }))).toBe('Running: 1 started, 1 skipped, 1 failed of 3')
  })

  it('shows per-VM status, code and reason in the detail', () => {
    const run: AutostartRun = {
      id: 'r1', trigger: 'manual', state: 'completed', started_at: 'x',
      vms: [
        { vm: 'db', order: 1, status: 'started' },
        { vm: 'tpm', order: 2, status: 'skipped', code: 'preflight_blocked', reason: 'No TPM here.' },
        { vm: 'web', order: 3, status: 'failed', reason: 'driver said no' },
      ],
    }
    const html = renderToStaticMarkup(<AutostartRunsView runs={[summary()]} selected={run} onSelect={() => {}} />)
    expect(html).toContain('db')
    expect(html).toContain('skipped (preflight_blocked)')
    expect(html).toContain('No TPM here.')
    expect(html).toContain('driver said no')
  })

  it('shows a run-level error', () => {
    const run: AutostartRun = { id: 'r2', trigger: 'startup', state: 'failed', started_at: 'x', vms: [], error: 'store down' }
    const html = renderToStaticMarkup(<AutostartRunsView runs={[summary({ id: 'r2', state: 'failed' })]} selected={run} onSelect={() => {}} />)
    expect(html).toContain('store down')
  })
})
