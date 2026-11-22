// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { AuditInsightsPanel } from '../AuditInsightsPanel'
import { computeAuditInsights, type AuditEntry } from '../../lib/auditInsights'

const e = (i: number, user: string, status: 'success' | 'failed', action: string, ts: string): AuditEntry => ({
  id: `p${i}`, timestamp: ts, user, action, resource_type: 'auth', resource_name: '-', status, ip_address: '10.0.0.1',
})

describe('AuditInsightsPanel', () => {
  it('says nothing unusual for a calm log and lists no insights', () => {
    const html = renderToStaticMarkup(<AuditInsightsPanel insights={[]} analysed={12} />)
    expect(html).toContain('No unusual activity found')
    expect(html).toContain('12 entries')
    expect(html).not.toContain('audit-insight"')
  })

  it('asks for data when nothing is loaded', () => {
    expect(renderToStaticMarkup(<AuditInsightsPanel insights={[]} analysed={0} />)).toContain('No entries to analyse')
  })

  it('renders insights with severity and detail', () => {
    const log = Array.from({ length: 10 }, (_, i) => e(i, 'eve', 'failed', 'login', `2026-03-02T10:0${i}:00Z`))
    const html = renderToStaticMarkup(<AuditInsightsPanel insights={computeAuditInsights(log)} analysed={log.length} />)
    expect(html).toContain('Critical: ')
    expect(html).toContain('10 failed sign-in attempts in a row for eve')
    expect(html).toContain('1 critical')
  })
})
