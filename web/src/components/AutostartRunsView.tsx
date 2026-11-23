// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import type { AutostartRun, AutostartRunSummary } from '../api/autostart'

export interface AutostartRunsViewProps {
  runs: AutostartRunSummary[]
  selected: AutostartRun | null
  onSelect: (id: string) => void
}

export function runHeadline(r: AutostartRunSummary): string {
  const parts = `${r.started} started, ${r.skipped} skipped, ${r.failed} failed`
  return r.state === 'running' ? `Running: ${parts} of ${r.candidates}` : parts
}

export function AutostartRunsView({ runs, selected, onSelect }: AutostartRunsViewProps) {
  if (runs.length === 0) {
    return <p className="text-sm text-[var(--zf-muted)]">No autostart runs yet.</p>
  }
  return (
    <div className="grid gap-6 lg:grid-cols-2">
      <table className="w-full text-sm" aria-label="Autostart runs">
        <thead>
          <tr className="text-left text-[var(--zf-muted)]">
            <th className="py-2 pr-3">Started</th>
            <th className="py-2 pr-3">Trigger</th>
            <th className="py-2 pr-3">State</th>
            <th className="py-2">Result</th>
          </tr>
        </thead>
        <tbody>
          {runs.map((r) => (
            <tr key={r.id} className={selected?.id === r.id ? 'bg-[var(--zf-canvas)]' : ''}>
              <td className="py-2 pr-3">
                <button type="button" className="underline" onClick={() => onSelect(r.id)}>
                  {r.started_at}
                </button>
              </td>
              <td className="py-2 pr-3">{r.trigger}</td>
              <td className="py-2 pr-3">{r.state}</td>
              <td className="py-2">{runHeadline(r)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {selected ? (
        <section aria-label="Run detail">
          <h2 className="mb-2 font-semibold">Run {selected.id}</h2>
          {selected.error && <p role="alert" className="mb-2 text-red-700">{selected.error}</p>}
          {selected.vms.length === 0 ? (
            <p className="text-sm text-[var(--zf-muted)]">No VMs are flagged for autostart.</p>
          ) : (
            <table className="w-full text-sm" aria-label="Per-VM results">
              <thead>
                <tr className="text-left text-[var(--zf-muted)]">
                  <th className="py-2 pr-3">VM</th>
                  <th className="py-2 pr-3">Status</th>
                  <th className="py-2">Detail</th>
                </tr>
              </thead>
              <tbody>
                {selected.vms.map((v) => (
                  <tr key={v.vm}>
                    <td className="py-2 pr-3">{v.vm}</td>
                    <td className="py-2 pr-3">{v.status}{v.code ? ` (${v.code})` : ''}</td>
                    <td className="py-2">{v.reason ?? ''}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      ) : (
        <p className="text-sm text-[var(--zf-muted)]">Select a run to see per-VM results.</p>
      )}
    </div>
  )
}
