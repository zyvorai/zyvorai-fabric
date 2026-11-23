// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import type { DrainPlan } from '../api/drainPlan'

export function drainHeadline(p: DrainPlan): string {
  const s = p.summary
  const parts = `${s.migrate} migrate, ${s.relocate} relocate, ${s.stop} stop, ${s.blocked} blocked`
  return `${p.can_drain ? 'Can drain' : 'Cannot drain'}: ${parts}`
}

export function DrainPlanView({ plan }: { plan: DrainPlan }) {
  return (
    <section aria-label="Drain plan">
      <p role="status" className={plan.can_drain ? 'mb-1' : 'mb-1 text-red-700'}>
        {drainHeadline(plan)}
      </p>
      <p className="mb-3 text-xs text-[var(--zf-muted)]">
        Dry run: nothing was moved or stopped. Plan hash <code>{plan.plan_hash}</code>
      </p>
      {plan.warnings.map((w) => (
        <p key={w} className="mb-1 text-sm text-amber-700">{w}</p>
      ))}
      {plan.actions.length === 0 ? (
        <p className="text-sm text-[var(--zf-muted)]">No VMs are assigned to this host.</p>
      ) : (
        <table className="mb-4 w-full text-sm" aria-label="Drain actions">
          <thead>
            <tr className="text-left text-[var(--zf-muted)]">
              <th className="py-2 pr-3">#</th>
              <th className="py-2 pr-3">VM</th>
              <th className="py-2 pr-3">Action</th>
              <th className="py-2">Target / reason</th>
            </tr>
          </thead>
          <tbody>
            {plan.actions.map((a) => (
              <tr key={a.vm}>
                <td className="py-2 pr-3">{a.order}</td>
                <td className="py-2 pr-3">{a.vm}</td>
                <td className="py-2 pr-3">{a.action}</td>
                <td className="py-2">
                  {a.target ?? a.reasons.join(', ')}
                  {a.warnings.map((w) => (
                    <span key={w} className="ml-2 text-amber-700">({w})</span>
                  ))}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {plan.targets.length > 0 && (
        <table className="w-full text-sm" aria-label="Target headroom">
          <thead>
            <tr className="text-left text-[var(--zf-muted)]">
              <th className="py-2 pr-3">Target host</th>
              <th className="py-2 pr-3">vCPU after / capacity</th>
              <th className="py-2">Memory MB after / capacity</th>
            </tr>
          </thead>
          <tbody>
            {plan.targets.map((t) => (
              <tr key={t.host_id}>
                <td className="py-2 pr-3">{t.host_id}</td>
                <td className="py-2 pr-3">{t.vcpus_used_after} / {t.vcpus_capacity}</td>
                <td className="py-2">{t.memory_mb_used_after} / {t.memory_mb_capacity}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  )
}
