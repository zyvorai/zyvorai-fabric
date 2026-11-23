// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useState } from 'react'
import type { AuditExportFormat, AuditExportParams, AuditVerifyReport } from '../api/audit'
import { describeVerify } from '../lib/auditVerify'

export interface AuditIntegrityPanelProps {
  onExport: (params: AuditExportParams, format: AuditExportFormat) => void | Promise<void>
  onVerify: () => void | Promise<void>
  /** Latest verification result, if any. */
  report: AuditVerifyReport | null
  /** Why the last verify or export failed, if it did. */
  error: string | null
  /** Which action is running. */
  busy: 'export' | 'verify' | null
  /** Result line for the last export (for example "Exported 12 events as NDJSON"). */
  exportNote: string | null
}

const TONE: Record<'ok' | 'broken' | 'neutral', string> = {
  ok: 'border-green-200 bg-green-50 text-green-800',
  broken: 'border-red-200 bg-red-50 text-red-700',
  neutral: 'border-[var(--zf-hairline)] bg-black/[0.02] text-[var(--zf-ink)]',
}

const FIELD =
  'w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)]'

/** Export and integrity controls for the audit log. Holds only the form inputs. */
export function AuditIntegrityPanel({ onExport, onVerify, report, error, busy, exportNote }: AuditIntegrityPanelProps) {
  const [params, setParams] = useState<AuditExportParams>({})
  const set = (k: keyof AuditExportParams) => (e: { target: { value: string } }) =>
    setParams((p) => ({ ...p, [k]: e.target.value }))
  const summary = report ? describeVerify(report) : null

  return (
    <section className="zf-panel-muted p-4 mb-4" data-testid="audit-integrity" aria-label="Audit export and integrity">
      <h3 className="text-sm font-medium text-[var(--zf-ink)] mb-2">Export and integrity</h3>
      <div className="grid grid-cols-1 md:grid-cols-4 gap-3">
        <label className="text-xs text-[var(--zf-muted)]">
          From (date or RFC 3339)
          <input aria-label="Export from" className={FIELD} placeholder="2026-03-01" value={params.from ?? ''} onChange={set('from')} />
        </label>
        <label className="text-xs text-[var(--zf-muted)]">
          To (exclusive)
          <input aria-label="Export to" className={FIELD} placeholder="2026-03-31" value={params.to ?? ''} onChange={set('to')} />
        </label>
        <label className="text-xs text-[var(--zf-muted)]">
          Actor
          <input aria-label="Export actor" className={FIELD} value={params.actor ?? ''} onChange={set('actor')} />
        </label>
        <label className="text-xs text-[var(--zf-muted)]">
          Action
          <input aria-label="Export action" className={FIELD} value={params.action ?? ''} onChange={set('action')} />
        </label>
      </div>
      <div className="flex flex-wrap gap-2 mt-3">
        <button type="button" disabled={busy !== null} onClick={() => onExport(params, 'ndjson')}
          className="px-3 py-2 rounded-lg border border-[var(--zf-hairline)] text-sm disabled:opacity-50">
          Export NDJSON
        </button>
        <button type="button" disabled={busy !== null} onClick={() => onExport(params, 'csv')}
          className="px-3 py-2 rounded-lg border border-[var(--zf-hairline)] text-sm disabled:opacity-50">
          Export CSV
        </button>
        <button type="button" disabled={busy !== null} onClick={() => onVerify()}
          className="px-3 py-2 rounded-lg bg-[var(--zf-link-fill)] text-white text-sm disabled:opacity-50">
          {busy === 'verify' ? 'Verifying...' : 'Verify integrity'}
        </button>
      </div>
      {busy === 'export' && <p role="status" className="mt-2 text-xs text-[var(--zf-muted)]">Preparing export...</p>}
      {exportNote && <p role="status" data-testid="audit-export-note" className="mt-2 text-xs text-[var(--zf-muted)]">{exportNote}</p>}
      {error && (
        <p role="alert" data-testid="audit-integrity-error" className="mt-2 rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700">
          {error}
        </p>
      )}
      {summary && (
        <div role="status" data-testid="audit-verify-result" data-tone={summary.tone}
          className={`mt-3 rounded-lg border px-3 py-2 text-xs ${TONE[summary.tone]}`}>
          <span className="font-semibold">{summary.headline}</span>
          <span className="block opacity-80">{summary.detail}</span>
          {report && (
            <span className="block opacity-70 mt-1">
              {report.total} events, {report.chained} chained
              {report.anchor ? `, pruned up to event ${report.anchor.seq}` : ''}
            </span>
          )}
        </div>
      )}
    </section>
  )
}
