// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { EgressSeal } from '../EgressSeal'
import { BudgetGauges } from '../BudgetGauges'
import { ChangesetDiff } from '../ChangesetDiff'
import { RunTimeline } from '../RunTimeline'

describe('EgressSeal', () => {
  it('says sealed only for a reported zero', () => {
    const html = renderToStaticMarkup(<EgressSeal connects={0} />)
    expect(html).toContain('0 CONNECT, sealed')
    expect(html).toContain('data-state="sealed"')
  })

  it('never claims sealed when the count was not reported', () => {
    const html = renderToStaticMarkup(<EgressSeal connects={undefined} />)
    expect(html).not.toContain('sealed')
    expect(html).toContain('data-state="unknown"')
    expect(html).toContain('nothing is claimed')
  })

  it('says not sealed, with the count, when something left the cell', () => {
    const html = renderToStaticMarkup(<EgressSeal connects={3} pausedReason="egress" />)
    expect(html).toContain('3 CONNECT, not sealed')
    expect(html).toContain('3 connection attempts')
    expect(html).toContain('Agent paused: egress')
    expect(renderToStaticMarkup(<EgressSeal connects={1} />)).toContain('1 connection attempt left')
  })

  it('draws drop-reason bars only for a shape it knows', () => {
    const bars = renderToStaticMarkup(<EgressSeal connects={0} dropReasons={[{ reason: 'not-allowlisted', count: 4 }]} />)
    expect(bars).toContain('not-allowlisted')
    expect(renderToStaticMarkup(<EgressSeal connects={0} dropReasons={{ weird: 1 }} />)).not.toContain('Dropped traffic')
  })
})

describe('BudgetGauges', () => {
  const scope = (over: Record<string, unknown>) => ({
    scope: 'day', calls: { used: 41, max: 50 }, cost: { used: 0.72, max: 1.0, currency: 'USD' }, fraction: 0.82, reached: false, warn: true, ...over,
  })

  it('shows each scope as a meter with the reading in text, and warns near the cap', () => {
    const html = renderToStaticMarkup(<BudgetGauges budget={{ warn: true, scopes: [scope({}) as never] }} />)
    expect(html).toContain('role="meter"')
    expect(html).toContain('41 of 50 calls')
    expect(html).toContain('aria-valuenow="82"')
    expect(html).toContain('82% used: close to the limit.')
  })

  it('says refused at the cap, and a read error is shown as refusing, not as no budget', () => {
    const reached = renderToStaticMarkup(<BudgetGauges budget={{ reached: true, scopes: [scope({ reached: true, warn: false, calls: { used: 50, max: 50 } }) as never] }} />)
    expect(reached).toContain('Limit reached: further calls are refused.')
    const err = renderToStaticMarkup(<BudgetGauges budget={{ error: 'prices file: nope' }} />)
    expect(err).toContain('could not be read, so model calls are refused')
  })

  it('draws no bar for a cap that is not set', () => {
    const html = renderToStaticMarkup(<BudgetGauges budget={{ scopes: [scope({ calls: { used: 3, max: null }, cost: { used: 0, max: null }, warn: false }) as never] }} />)
    expect(html).toContain('aria-valuenow="0"')
  })
})

describe('ChangesetDiff', () => {
  const planned = {
    changeset_id: 'cs', command: 'make out', exit_code: 0,
    changes: { added: ['/w/a', '/w/b'], modified: ['/w/c'], deleted: ['/w/d'] },
    side_effects: { egress: 'blocked', replayable: ['4 file changes'], non_replayable: ['sent mail'] },
    unstaged: ['/w/big'], expires_at: Math.floor(Date.now() / 1000) + 600,
  }

  it('shows counts, files by kind, both replay columns, the masking note and the expiry', () => {
    const html = renderToStaticMarkup(<ChangesetDiff planned={planned} />)
    expect(html).toContain('+2')
    expect(html).toContain('~1')
    expect(html).toContain('−1')
    expect(html).toContain('/w/a')
    expect(html).toContain('Replayed on approval')
    expect(html).toContain('Recorded, never replayed')
    expect(html).toContain('4 file changes')
    expect(html).toContain('sent mail')
    expect(html).toContain('File contents are masked on the host')
    expect(html).toContain('Not captured, so not applied: /w/big')
    expect(html).toContain('data-testid="changeset-expiry"')
    expect(html).toContain('Nothing reaches the sandbox until you approve. Network: blocked.')
  })

  it('says an expired changeset will not apply, and renders nothing for a non-changeset', () => {
    const html = renderToStaticMarkup(<ChangesetDiff planned={{ ...planned, expires_at: 1 }} />)
    expect(html).toContain('Expired: approving will not apply it')
    expect(renderToStaticMarkup(<ChangesetDiff planned={{ host: 'x' }} />)).toBe('')
  })
})

describe('RunTimeline', () => {
  it('words every outcome, so colour is never the only signal, and a could-not-apply is Failed', () => {
    const html = renderToStaticMarkup(
      <RunTimeline entries={[
        { label: 'asha approved the changeset', kind: 'approval.approved' },
        { label: 'FluxVM could not apply it: already rejected', kind: 'changeset.apply' },
        { label: 'Asked the model', kind: 'model.call' },
      ]} />,
    )
    expect(html).toContain('data-tone="proof"')
    expect(html).toContain('>Done<')
    expect(html).toContain('data-tone="danger"')
    expect(html).toContain('>Failed<')
    expect(html).toContain('data-tone="neutral"')
  })
})
