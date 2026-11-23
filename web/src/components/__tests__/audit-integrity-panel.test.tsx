// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { AuditIntegrityPanel } from '../AuditIntegrityPanel'
import type { AuditVerifyReport } from '../../api/audit'

const noop = () => {}
const base = { onExport: noop, onVerify: noop, report: null, error: null, busy: null, exportNote: null } as const
const ok: AuditVerifyReport = {
  status: 'ok', ok: true, chain_enabled: true, total: 7, chained: 6, legacy_unchained: 1,
  malformed: [], anchor: { seq: 3, hash: 'h' }, checked_at: '2026-03-01T00:00:00Z',
}

describe('AuditIntegrityPanel', () => {
  it('shows the export and verify controls and no result yet', () => {
    const html = renderToStaticMarkup(<AuditIntegrityPanel {...base} />)
    expect(html).toContain('Export NDJSON')
    expect(html).toContain('Export CSV')
    expect(html).toContain('Verify integrity')
    expect(html).not.toContain('audit-verify-result')
  })

  it('shows an intact chain result with counts and the prune point', () => {
    const html = renderToStaticMarkup(<AuditIntegrityPanel {...base} report={ok} />)
    expect(html).toContain('data-tone="ok"')
    expect(html).toContain('Chain intact: 6 chained events verified')
    expect(html).toContain('7 events, 6 chained, pruned up to event 3')
  })

  it('shows the first broken link', () => {
    const bad: AuditVerifyReport = {
      ...ok, status: 'broken', ok: false,
      first_break: { reason: 'hash_mismatch', seq: 2, id: 'e2', detail: 'event e2 was modified after it was written' },
    }
    const html = renderToStaticMarkup(<AuditIntegrityPanel {...base} report={bad} />)
    expect(html).toContain('data-tone="broken"')
    expect(html).toContain('Chain broken at event 2')
    expect(html).toContain('An event was modified after it was written')
  })

  it('shows an error and disables the buttons while busy', () => {
    const html = renderToStaticMarkup(<AuditIntegrityPanel {...base} error="more than 100000 events match" busy="verify" />)
    expect(html).toContain('role="alert"')
    expect(html).toContain('more than 100000 events match')
    expect(html).toContain('Verifying...')
    expect(html.match(/disabled=""/g)?.length).toBe(3)
  })

  it('shows the export result line', () => {
    expect(renderToStaticMarkup(<AuditIntegrityPanel {...base} exportNote="Exported 12 events as NDJSON" />)).toContain('Exported 12 events as NDJSON')
  })
})
