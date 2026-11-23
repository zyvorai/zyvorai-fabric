// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import type { SnapshotHealthReport, SnapshotHealthStatus } from '../api/snapshotHealth'
import { formatBytes } from '../utils/format'

export const STATUS_LABEL: Record<SnapshotHealthStatus, string> = {
  healthy: 'Healthy',
  at_risk: 'At risk',
  breached: 'RPO breached',
  no_snapshots: 'No snapshots',
}

/** Compact duration such as "2d 3h", "45m" or "30s". */
export function formatAge(secs: number | null): string {
  if (secs === null) return 'never'
  if (secs < 60) return `${secs}s`
  if (secs < 3600) return `${Math.floor(secs / 60)}m`
  if (secs < 86400) return `${Math.floor(secs / 3600)}h`
  const d = Math.floor(secs / 86400)
  const h = Math.floor((secs % 86400) / 3600)
  return h ? `${d}d ${h}h` : `${d}d`
}

export function SnapshotHealthView({ report }: { report: SnapshotHealthReport }) {
  const s = report.summary
  if (report.vms.length === 0) {
    return <p className="text-sm text-[var(--zf-muted)]">No VMs to assess.</p>
  }
  return (
    <section aria-label="Snapshot health">
      <p className="mb-3" role="status">
        {s.healthy} healthy, {s.at_risk} at risk, {s.breached} breached, {s.no_snapshots} without
        snapshots; {s.orphaned_snapshots} broken-chain snapshot(s). Snapshots use{' '}
        {formatBytes(s.total_bytes)}, projected {formatBytes(s.projected_bytes_30d)} in 30 days.
      </p>
      <table className="w-full text-sm" aria-label="Per-VM snapshot health">
        <thead>
          <tr className="text-left text-[var(--zf-muted)]">
            <th className="py-2 pr-3">VM</th>
            <th className="py-2 pr-3">Status</th>
            <th className="py-2 pr-3">Newest</th>
            <th className="py-2 pr-3">Target</th>
            <th className="py-2 pr-3">Used</th>
            <th className="py-2 pr-3">30d forecast</th>
            <th className="py-2">Findings</th>
          </tr>
        </thead>
        <tbody>
          {report.vms.map((v) => (
            <tr key={v.vm}>
              <td className="py-2 pr-3">{v.vm}</td>
              <td className="py-2 pr-3">{STATUS_LABEL[v.status]}</td>
              <td className="py-2 pr-3">{formatAge(v.age_secs)}</td>
              <td className="py-2 pr-3">
                {formatAge(v.rpo_target_secs)}
                {v.rpo_source === 'default' ? ' (default)' : ''}
              </td>
              <td className="py-2 pr-3">{formatBytes(v.total_bytes)}</td>
              <td className="py-2 pr-3">{formatBytes(v.projected_bytes_30d)}</td>
              <td className="py-2">{v.findings.join('; ')}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  )
}
