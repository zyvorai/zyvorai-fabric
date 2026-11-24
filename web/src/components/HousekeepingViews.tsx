// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import type {
  HygieneReport,
  HygieneStatus,
  OwnershipReport,
  OwnershipStatus,
  ReclaimReport,
} from '../api/housekeeping'
import { formatBytes } from '../utils/format'

export const HYGIENE_LABEL: Record<HygieneStatus, string> = {
  ok: 'OK',
  warning: 'Needs cleanup',
  critical: 'Critical',
}

export const OWNERSHIP_LABEL: Record<OwnershipStatus, string> = {
  expired: 'Lease expired',
  expiring: 'Expiring soon',
  unowned: 'No owner',
  owned: 'Owned',
}

/** Whole hours as "5h", "3d" or "3d 4h". */
export function formatHours(h: number | null): string {
  if (h === null) return 'none'
  if (h < 24) return `${h}h`
  const d = Math.floor(h / 24)
  const r = h % 24
  return r ? `${d}d ${r}h` : `${d}d`
}

/** "in 5 days", "today" or "3 days ago" from a signed day count. */
export function formatLease(days: number | null): string {
  if (days === null) return 'no expiry'
  if (days < 0) return `${-days} day${days === -1 ? '' : 's'} ago`
  if (days === 0) return 'today'
  return `in ${days} day${days === 1 ? '' : 's'}`
}

export function SnapshotHygieneView({ report }: { report: HygieneReport }) {
  const s = report.summary
  if (report.vms.length === 0) {
    return <p className="text-sm text-[var(--zf-muted)]">No VM has snapshots.</p>
  }
  return (
    <section aria-label="Snapshot hygiene">
      <p className="mb-3" role="status">
        {s.critical} critical, {s.warning} need cleanup, {s.ok} OK. {s.stale_snapshots} snapshot(s)
        older than {formatHours(report.max_age_hours)} ({s.protected_stale} protected by keep);{' '}
        {formatBytes(s.reclaimable_bytes)} of {formatBytes(s.total_bytes)} could be reclaimed at most.
      </p>
      <table className="w-full text-sm" aria-label="Per-VM snapshot hygiene">
        <thead>
          <tr className="text-left text-[var(--zf-muted)]">
            <th className="py-2 pr-3">VM</th>
            <th className="py-2 pr-3">Status</th>
            <th className="py-2 pr-3">Snapshots</th>
            <th className="py-2 pr-3">Chain</th>
            <th className="py-2 pr-3">Oldest</th>
            <th className="py-2 pr-3">Reclaimable</th>
            <th className="py-2">Findings</th>
          </tr>
        </thead>
        <tbody>
          {report.vms.map((v) => (
            <tr key={v.vm}>
              <td className="py-2 pr-3">{v.vm}</td>
              <td className="py-2 pr-3">{HYGIENE_LABEL[v.status]}</td>
              <td className="py-2 pr-3">{v.snapshot_count}</td>
              <td className="py-2 pr-3">{v.max_chain_depth}</td>
              <td className="py-2 pr-3">{formatHours(v.oldest_age_hours)}</td>
              <td className="py-2 pr-3">{formatBytes(v.reclaimable_bytes)}</td>
              <td className="py-2">{v.findings.join('; ')}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  )
}

export function OwnershipReportView({
  report,
  onSelect,
}: {
  report: OwnershipReport
  onSelect?: (vm: string) => void
}) {
  const s = report.summary
  if (report.vms.length === 0) {
    return <p className="text-sm text-[var(--zf-muted)]">No VMs.</p>
  }
  return (
    <section aria-label="Ownership">
      <p className="mb-3" role="status">
        {s.expired} expired, {s.expiring} expiring within {report.within_days} days, {s.unowned} without
        an owner, {s.owned} owned.
      </p>
      <table className="w-full text-sm" aria-label="VM ownership">
        <thead>
          <tr className="text-left text-[var(--zf-muted)]">
            <th className="py-2 pr-3">VM</th>
            <th className="py-2 pr-3">Status</th>
            <th className="py-2 pr-3">State</th>
            <th className="py-2 pr-3">Owner</th>
            <th className="py-2 pr-3">Contact</th>
            <th className="py-2 pr-3">Lease</th>
            <th className="py-2">Edit</th>
          </tr>
        </thead>
        <tbody>
          {report.vms.map((v) => (
            <tr key={v.vm}>
              <td className="py-2 pr-3">{v.vm}</td>
              <td className="py-2 pr-3">{OWNERSHIP_LABEL[v.status]}</td>
              <td className="py-2 pr-3">{v.state}</td>
              <td className="py-2 pr-3">{v.owner ?? '-'}</td>
              <td className="py-2 pr-3">{v.contact ?? '-'}</td>
              <td className="py-2 pr-3">{formatLease(v.days_to_expiry)}</td>
              <td className="py-2">
                {onSelect && (
                  <button type="button" className="btn-secondary" onClick={() => onSelect(v.vm)}>
                    Edit {v.vm}
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  )
}

export function ReclaimView({ report }: { report: ReclaimReport }) {
  const s = report.summary
  if (report.candidates.length === 0) {
    return (
      <p className="text-sm text-[var(--zf-muted)]" role="status">
        Nothing to reclaim: no VM has been stopped for {report.stopped_days} days or has an expired
        lease. Stale snapshots fleet-wide: {formatBytes(s.stale_snapshot_bytes_total)}.
      </p>
    )
  }
  return (
    <section aria-label="Reclaim candidates">
      <p className="mb-3" role="status">
        {s.candidates} candidate(s): {s.idle_vms} idle, {s.expired_leases} with an expired lease,{' '}
        {s.unowned_candidates} without an owner. Up to {s.reclaimable_disk_gb} GB of disk and{' '}
        {formatBytes(s.reclaimable_snapshot_bytes_candidates)} of snapshots on these VMs; stale
        snapshots fleet-wide {formatBytes(s.stale_snapshot_bytes_total)}. Nothing is deleted from here.
      </p>
      <table className="w-full text-sm" aria-label="Reclaim candidates">
        <thead>
          <tr className="text-left text-[var(--zf-muted)]">
            <th className="py-2 pr-3">VM</th>
            <th className="py-2 pr-3">State</th>
            <th className="py-2 pr-3">Disk</th>
            <th className="py-2 pr-3">Snapshots</th>
            <th className="py-2 pr-3">Owner</th>
            <th className="py-2">Why</th>
          </tr>
        </thead>
        <tbody>
          {report.candidates.map((c) => (
            <tr key={c.vm}>
              <td className="py-2 pr-3">{c.vm}</td>
              <td className="py-2 pr-3">{c.state}</td>
              <td className="py-2 pr-3">{c.reclaimable_disk_gb} GB</td>
              <td className="py-2 pr-3">{formatBytes(c.reclaimable_snapshot_bytes)}</td>
              <td className="py-2 pr-3">{c.owner ?? '-'}</td>
              <td className="py-2">{c.reasons.join('; ')}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  )
}
