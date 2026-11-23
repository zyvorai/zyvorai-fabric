// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describeVerdict, type CpuCompat, type CpuInventory } from '../api/cpuCompat'

export function CpuInventoryView({ inv }: { inv: CpuInventory }) {
  return (
    <section aria-label="CPU inventory">
      <table className="mb-4 w-full text-sm" aria-label="Host CPUs">
        <thead>
          <tr className="text-left text-[var(--zf-muted)]">
            <th className="py-2 pr-3">Host</th>
            <th className="py-2 pr-3">Cluster</th>
            <th className="py-2">CPU</th>
          </tr>
        </thead>
        <tbody>
          {inv.hosts.map((h) => (
            <tr key={h.host_id}>
              <td className="py-2 pr-3">{h.hostname}</td>
              <td className="py-2 pr-3">{h.cluster_id}</td>
              <td className="py-2">
                {h.cpu
                  ? `${h.cpu.vendor} ${h.cpu.model_name} (${h.cpu.flags.length} flags, ${h.cpu.source})`
                  : 'No CPU inventory recorded'}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <table className="mb-4 w-full text-sm" aria-label="Cluster baselines">
        <thead>
          <tr className="text-left text-[var(--zf-muted)]">
            <th className="py-2 pr-3">Cluster</th>
            <th className="py-2 pr-3">Hosts known</th>
            <th className="py-2">Baseline model</th>
          </tr>
        </thead>
        <tbody>
          {inv.clusters.map((c) => (
            <tr key={c.cluster_id}>
              <td className="py-2 pr-3">{c.cluster_id}</td>
              <td className="py-2 pr-3">{c.hosts_known} / {c.hosts_total}</td>
              <td className="py-2">{c.baseline_model ?? 'Unknown'}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="text-sm">
        VMs with a stored CPU model: {inv.vms.filter((v) => v.configured).length}
        {inv.vms_truncated ? ' (list truncated)' : ''}
      </p>
    </section>
  )
}

export function CpuCompatView({ compat }: { compat: CpuCompat }) {
  if (!compat.configured) {
    return <p className="text-sm">VM {compat.vm} has no stored CPU model, so there is nothing to check.</p>
  }
  return (
    <section aria-label="CPU compatibility">
      <p className="mb-2 text-sm">
        {compat.vm}: {compat.mode} {compat.model}
      </p>
      {compat.requirement.notes.map((n) => (
        <p key={n} className="mb-1 text-sm text-amber-700">{n}</p>
      ))}
      <table className="w-full text-sm" aria-label="Compatibility per host">
        <tbody>
          {compat.results.map((r) => (
            <tr key={r.host_id}>
              <td className="py-2 pr-3">{r.hostname}</td>
              <td className={r.result.verdict === 'incompatible' ? 'py-2 text-red-700' : 'py-2'}>
                {describeVerdict(r.result)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  )
}
