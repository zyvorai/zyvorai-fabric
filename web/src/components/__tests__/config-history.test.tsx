// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'

vi.mock('../../api/client', () => ({
  apiGet: vi.fn().mockResolvedValue({}),
  apiPost: vi.fn().mockResolvedValue({}),
}))

import { apiGet, apiPost } from '../../api/client'
import {
  diffVmRevisions,
  getSinceGood,
  getVmHistory,
  getVmRevision,
  goodLabel,
  markRevisionGood,
  type SinceGood,
  type VmDiff,
  type VmHistory,
} from '../../api/configHistory'
import { ChangeList, DiffView, RevisionTable, SinceGoodView } from '../ConfigHistoryView'

beforeEach(() => vi.clearAllMocks())

const history: VmHistory = {
  vm: 'web-01',
  exists: true,
  current_rev: 2,
  last_good_rev: 1,
  retention: { max_revisions: 100, kept: 2, evicted_total: 0 },
  good_after_minutes: 15,
  good_signal: 'signal',
  redaction: 'r',
  chain: { status: 'ok', revisions: 2 },
  truncated_list: false,
  revisions: [
    {
      rev: 2, timestamp: '2026-01-02T00:00:00Z', actor: 'bob', cause: 'UPDATE_TAGS', content_hash: 'a', rev_hash: 'b',
      truncated: false, audit: { id: 'x', link: 'ok' }, good: null, changed_fields: 2, high_risk_changes: 1,
    },
    {
      rev: 1, timestamp: '2026-01-01T00:00:00Z', actor: 'alice', cause: 'CREATE', content_hash: 'c', rev_hash: 'd',
      truncated: false, audit: { id: 'y', link: 'ok' }, good: { source: 'auto', at: 't', healthy_minutes: 22 }, changed_fields: 0, high_risk_changes: 0,
    },
  ],
}

describe('config history api', () => {
  it('calls the history endpoints with encoded names', async () => {
    await getVmHistory('a/b')
    expect(apiGet).toHaveBeenCalledWith('/api/vms/a%2Fb/history')
    await getVmHistory('vm', 10)
    expect(apiGet).toHaveBeenCalledWith('/api/vms/vm/history?limit=10')
    await getVmRevision('vm', 3)
    expect(apiGet).toHaveBeenCalledWith('/api/vms/vm/history/3')
    await diffVmRevisions('vm', 1, 3)
    expect(apiGet).toHaveBeenCalledWith('/api/vms/vm/history/diff?from=1&to=3')
    await diffVmRevisions('vm', 1)
    expect(apiGet).toHaveBeenCalledWith('/api/vms/vm/history/diff?from=1')
    await getSinceGood('vm')
    expect(apiGet).toHaveBeenCalledWith('/api/vms/vm/history/since-good')
    await markRevisionGood('vm', 2, 'checked')
    expect(apiPost).toHaveBeenCalledWith('/api/vms/vm/history/2/mark-good', { note: 'checked' })
  })

  it('describes how a revision became good', () => {
    expect(goodLabel(null)).toBe('')
    expect(goodLabel({ source: 'operator', at: 't', by: 'ann' })).toBe('Marked good by ann')
    expect(goodLabel({ source: 'auto', at: 't', healthy_minutes: 22 })).toBe('Good automatically (22 min running)')
  })
})

describe('config history views', () => {
  it('lists revisions, offers mark-good only to writers and only when not good', () => {
    const noop = () => undefined
    const w = renderToStaticMarkup(<RevisionTable history={history} selected={[1]} onToggle={noop} onMarkGood={noop} canWrite />)
    expect(w).toContain('Mark revision 2 good')
    expect(w).not.toContain('Mark revision 1 good')
    expect(w).toContain('Good automatically (22 min running)')
    expect(w).toContain('1 high risk')
    const r = renderToStaticMarkup(<RevisionTable history={history} selected={[]} onToggle={noop} onMarkGood={noop} canWrite={false} />)
    expect(r).not.toContain('Mark revision')
    expect(r).toContain('Not marked')
  })

  it('renders risky changes with the reason, and hidden values without data', () => {
    const html = renderToStaticMarkup(
      <ChangeList
        changes={[
          { path: 'cpus', label: 'vCPUs', kind: 'changed', summary: 'vCPU count decreased from 4 to 2', risk: { level: 'high', reason: 'starved' }, hidden: false },
          { path: 'labels.api_token', label: 'Label', kind: 'changed', summary: 'Label api_token changed (value hidden)', hidden: true },
        ]}
      />,
    )
    expect(html).toContain('High risk')
    expect(html).toContain('Why it matters: starved')
    expect(html).toContain('only the fact that it changed')
    expect(renderToStaticMarkup(<ChangeList changes={[]} />)).toContain('No configuration differences')
  })

  it('renders the since-good states', () => {
    const none: SinceGood = { vm: 'v', status: 'no_known_good', current_rev: 1, message: 'No revision is marked good yet.', good_after_minutes: 15 }
    expect(renderToStaticMarkup(<SinceGoodView data={none} />)).toContain('No revision is marked good yet.')
    const same: SinceGood = { vm: 'v', status: 'unchanged', good_rev: 1, current_rev: 1, good: null, summary: 'nothing changed' }
    expect(renderToStaticMarkup(<SinceGoodView data={same} />)).toContain('nothing changed')
    const changed: SinceGood = {
      vm: 'v', status: 'changed', good_rev: 1, current_rev: 3, good: null, summary: 'since revision 1 ...',
      counts: { total: 1, high: 1, medium: 0, low: 0 },
      culprits: [{ rank: 1, path: 'memory', label: 'Memory', kind: 'changed', summary: 'Memory decreased', risk: { level: 'high', reason: 'oom' }, hidden: false, changed_in: [{ rev: 3, actor: 'bob', at: 't', cause: 'edit' }] }],
      revisions_since: [], note: 'Read-only.',
    }
    const html = renderToStaticMarkup(<SinceGoodView data={changed} />)
    expect(html).toContain('Likely culprits, most likely first')
    expect(html).toContain('Revision 3 by bob')
    expect(html).toContain('Read-only.')
  })

  it('renders a diff header', () => {
    const d: VmDiff = {
      vm: 'v', from: { rev: 1, actor: 'a', at: 't', cause: 'c' }, to: { rev: 2, actor: 'b', at: 't', cause: 'c' },
      identical: true, identical_content_hash: true, counts: { total: 0, high: 0, medium: 0, low: 0 }, summary: 'identical', changes: [],
    }
    expect(renderToStaticMarkup(<DiffView diff={d} />)).toContain('revision 1 by a, revision 2 by b')
  })
})
