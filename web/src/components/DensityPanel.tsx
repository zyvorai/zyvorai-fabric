// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import type { DensityReport } from '../lib/capacity'

const LEVEL_TEXT = { ok: 'OK', warn: 'Near limit', critical: 'Over limit' } as const

export function DensityPanel({ report }: { report: DensityReport }) {
  const warnings = report.rows.filter((r) => r.warning)
  return (
    <section data-testid="density-panel" className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-5 space-y-3">
      <div>
        <h3 className="text-sm font-semibold text-[var(--zf-ink)]">VM density and overcommit</h3>
        <p className="text-xs text-[var(--zf-muted)]">
          Overcommit policy: {report.overcommit_enabled ? "enabled" : "disabled"} · {report.vm_count} VM{report.vm_count === 1 ? "" : "s"}
          {report.vcpus_per_vm !== null && report.memory_gb_per_vm !== null
            ? ` · average ${report.vcpus_per_vm.toFixed(1)} vCPUs and ${report.memory_gb_per_vm.toFixed(1)} GB per VM`
            : ""}
          {report.more_vms_fit !== null ? ` · room for about ${report.more_vms_fit} more average VM${report.more_vms_fit === 1 ? "" : "s"}` : ""}
        </p>
      </div>
      {warnings.length > 0 && (
        <ul role="alert" data-testid="density-warnings" className="text-xs text-amber-800 list-disc pl-5">
          {warnings.map((r) => <li key={r.resource}>{r.warning}</li>)}
        </ul>
      )}
      <table className="w-full text-xs">
        <thead>
          <tr className="text-left text-[var(--zf-muted)]">
            <th className="py-1">Resource</th><th>Committed</th><th>Limit</th><th>Of effective</th><th>Headroom</th><th>Status</th>
          </tr>
        </thead>
        <tbody>
          {report.rows.map((r) => (
            <tr key={r.resource} data-level={r.level} className="text-[var(--zf-ink)]">
              <td className="py-1">{r.resource}</td>
              <td>{r.commit_ratio.toFixed(2)}:1</td>
              <td>{r.limit_ratio.toFixed(2)}:1</td>
              <td>{r.utilization_pct.toFixed(1)}%</td>
              <td>{r.headroom.toFixed(r.unit === "GB" ? 1 : 0)} {r.unit}</td>
              <td>{LEVEL_TEXT[r.level]}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  )
}
