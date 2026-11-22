// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect, vi } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

// The container's API module is mocked so importing it can never reach the network.
vi.mock('../../api/configSnapshots', () => ({
  listConfigSnapshots: vi.fn().mockResolvedValue({ snapshots: [], max: 50 }),
  createConfigSnapshot: vi.fn(),
  getConfigDiff: vi.fn(),
}))

import { SnapshotDiffView, type SnapshotDiffViewProps } from '../SnapshotDiffView'
import ConfigSnapshotsTab from '../ConfigSnapshotsTab'
import { listConfigSnapshots } from '../../api/configSnapshots'

const snap = (id: string) => ({
  id,
  created_at: '2026-01-01T00:00:00Z',
  created_by: 'admin',
  vms: 1,
  network_policies: 0,
  storage_pools: 0,
})

const base: SnapshotDiffViewProps = {
  snapshots: [snap('snap-b'), snap('snap-a')],
  max: 50,
  from: 'snap-a',
  to: 'current',
  onFromChange: () => {},
  onToChange: () => {},
  onCreate: () => {},
  creating: false,
  diff: null,
  diffLoading: false,
  error: null,
}

describe('SnapshotDiffView', () => {
  it('shows an empty state with no snapshots and still offers to take one', () => {
    const html = renderToStaticMarkup(<SnapshotDiffView {...base} snapshots={[]} />)
    expect(html).toContain('No snapshots yet')
    expect(html).toContain('Take snapshot')
    expect(html).toContain('0 of 50 kept')
    expect(html).not.toContain('<select')
  })

  it('lists stored snapshots and "current" in both pickers', () => {
    const html = renderToStaticMarkup(<SnapshotDiffView {...base} />)
    expect((html.match(/<select/g) ?? []).length).toBe(2)
    expect(html).toContain('Current configuration')
    expect(html).toContain('snap-a')
    expect(html).toContain('snap-b')
    expect(html).toContain('2 of 50 kept')
  })

  it('reports no differences', () => {
    const diff = { from: 'snap-a', to: 'current', change_count: 0, changes: [] }
    const html = renderToStaticMarkup(<SnapshotDiffView {...base} diff={diff} />)
    expect(html).toContain('No differences.')
  })

  it('renders added, removed and changed entities with old and new values', () => {
    const diff = {
      from: 'snap-a',
      to: 'current',
      change_count: 4,
      changes: [
        { section: 'vms', key: 'web', kind: 'changed' as const, field: 'cpus', old: 2, new: 4 },
        { section: 'vms', key: 'web', kind: 'changed' as const, field: 'admin_password', old: '***REDACTED***', new: '***REDACTED***' },
        { section: 'vms', key: 'db', kind: 'added' as const, new: { name: 'db' } },
        { section: 'storage_pools', key: 'p1', kind: 'removed' as const, old: { name: 'p1' } },
      ],
    }
    const html = renderToStaticMarkup(<SnapshotDiffView {...base} diff={diff} />)
    expect(html).toContain('1 added, 1 removed, 2 field changes')
    expect(html).toContain('Virtual machines')
    expect(html).toContain('Storage pools')
    expect(html).toContain('cpus')
    expect(html).toMatch(/cpus:.*2.*→.*4/s)
    expect(html).toContain('***REDACTED***')
    expect(html).toContain('italic')
    // whole-entity added/removed do not dump their contents
    expect(html).not.toContain('{&quot;name&quot;')
  })

  it('shows loading, error and a disabled button while saving', () => {
    const html = renderToStaticMarkup(
      <SnapshotDiffView {...base} diffLoading error="boom" creating diff={{ from: 'a', to: 'b', change_count: 0, changes: [] }} />,
    )
    expect(html).toContain('Comparing…')
    expect(html).toContain('role="alert"')
    expect(html).toContain('boom')
    expect(html).toContain('disabled')
    expect(html).toContain('Saving…')
    expect(html).not.toContain('No differences.')
  })
})

describe('ConfigSnapshotsTab', () => {
  it('renders the empty view on first paint, before the mocked API answers', () => {
    const html = renderToStaticMarkup(<ConfigSnapshotsTab />)
    expect(html).toContain('data-testid="snapshot-diff-view"')
    expect(html).toContain('No snapshots yet')
    // effects do not run in a static render
    expect(listConfigSnapshots).not.toHaveBeenCalled()
  })
})
