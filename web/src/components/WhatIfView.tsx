// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { reasonText, summarizeReport, type FitReport, type WhatIfFormRow } from '../lib/capacity'

export interface WhatIfViewProps {
  rows: WhatIfFormRow[]
  report: FitReport | null
  source: 'api' | 'local' | null
  loading: boolean
  error: string | null
  onRowChange: (index: number, field: keyof WhatIfFormRow, value: string) => void
  onAddRow: () => void
  onRemoveRow: (index: number) => void
  onRun: () => void
}

const FIELDS: { key: keyof WhatIfFormRow; label: string; wide?: boolean }[] = [
  { key: 'name', label: 'Name', wide: true },
  { key: 'vcpus', label: 'vCPUs' },
  { key: 'memory_mb', label: 'Memory (MB)' },
  { key: 'disk_gb', label: 'Disk (GB)' },
  { key: 'count', label: 'Count' },
]

const fmtRoom = (n: number | null) => (n === null ? 'unknown' : String(n))

/** Presentational "What if I add..." panel: no fetching, renders statically. */
export function WhatIfView({ rows, report, source, loading, error, onRowChange, onAddRow, onRemoveRow, onRun }: WhatIfViewProps) {
  return (
    <section className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-5 space-y-4" data-testid="whatif-panel" aria-label="What if I add">
      <div>
        <h3 className="text-sm font-semibold text-[var(--zf-ink)]">What if I add...</h3>
        <p className="text-xs text-[var(--zf-muted)]">Simulates placing new VMs on this host. Nothing is created or reserved.</p>
      </div>

      <div className="space-y-2">
        {rows.map((row, i) => (
          <div key={i} className="flex flex-wrap items-end gap-2" data-testid="whatif-row">
            {FIELDS.map((f) => (
              <label key={f.key} className={`text-xs text-[var(--zf-muted)] ${f.wide ? 'flex-1 min-w-32' : 'w-28'}`}>
                {f.label}
                <input
                  className="mt-1 w-full rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-canvas)] px-2 py-1 text-sm text-[var(--zf-ink)]"
                  value={row[f.key]}
                  onChange={(e) => onRowChange(i, f.key, e.target.value)}
                />
              </label>
            ))}
            {rows.length > 1 && (
              <button type="button" onClick={() => onRemoveRow(i)} className="px-2 py-1 text-xs text-[var(--zf-muted)]" aria-label={`Remove row ${i + 1}`}>
                Remove
              </button>
            )}
          </div>
        ))}
      </div>

      <div className="flex gap-2">
        <button type="button" onClick={onAddRow} className="px-3 py-1.5 text-xs rounded-lg border border-[var(--zf-hairline)] text-[var(--zf-muted)]">
          Add another VM type
        </button>
        <button type="button" onClick={onRun} disabled={loading} className="px-3 py-1.5 text-xs font-medium rounded-lg bg-[var(--zf-ink)] text-[var(--zf-canvas)] disabled:opacity-50">
          {loading ? 'Simulating…' : 'Simulate'}
        </button>
      </div>

      {error && (
        <div role="alert" className="bg-red-50 rounded-lg border border-red-200 px-4 py-2 text-xs text-red-700">{error}</div>
      )}

      {report && (
        <div className="space-y-3" data-testid="whatif-report">
          <div
            role="status"
            data-testid="whatif-verdict"
            className={`rounded-lg border px-4 py-2 text-sm font-medium ${report.all_fit ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-amber-800 bg-amber-50 border-amber-200'}`}
          >
            {summarizeReport(report)}
          </div>

          {report.unplaced.length > 0 && (
            <ul className="text-xs space-y-1" aria-label="Does not fit">
              {report.unplaced.map((u) => (
                <li key={`${u.name}-${u.index}`} data-testid="whatif-unplaced" className="text-[var(--zf-danger-text)]">
                  {u.name} #{u.index}: {reasonText(u.reasons)}
                </li>
              ))}
            </ul>
          )}

          <table className="w-full text-xs">
            <thead>
              <tr className="text-left text-[var(--zf-muted)]">
                <th className="py-1">Host</th>
                <th>vCPUs left</th>
                <th>Memory left (MB)</th>
                <th>Disk left (GB)</th>
              </tr>
            </thead>
            <tbody>
              {report.hosts.map((h) => (
                <tr key={h.name} data-testid="whatif-host">
                  <td className="py-1">{h.name}</td>
                  <td>{h.headroom_after.vcpus} of {h.effective.vcpus}</td>
                  <td>{h.headroom_after.memory_mb} of {h.effective.memory_mb}</td>
                  <td>{fmtRoom(h.headroom_after.disk_gb)}{h.effective.disk_gb !== null ? ` of ${h.effective.disk_gb}` : ''}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {source && (
            <p className="text-[11px] text-[var(--zf-muted)]">
              {source === 'api' ? 'Calculated by the daemon.' : 'Calculated in the browser from the figures above (daemon what-if endpoint unavailable).'}
            </p>
          )}
        </div>
      )}
    </section>
  )
}
