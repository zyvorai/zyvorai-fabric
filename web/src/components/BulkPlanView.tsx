// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { bulkHeadline, type BulkApplied, type BulkPlan } from '../api/bulkOps'

export function BulkPlanView({ plan }: { plan: BulkPlan }) {
  return (
    <section aria-label="Bulk plan">
      <p role="status" className={plan.over_cap ? 'mb-1 text-red-700' : 'mb-1'}>{bulkHeadline(plan)}</p>
      <p className="mb-3 text-xs text-[var(--zf-muted)]">
        Dry run: nothing was changed. Plan hash <code>{plan.plan_hash}</code>
      </p>
      <table className="w-full text-sm" aria-label="Bulk plan items">
        <tbody>
          {plan.items.map((i) => (
            <tr key={i.vm}>
              <td className="py-2 pr-3">{i.vm}</td>
              <td className="py-2 pr-3">{i.will}</td>
              <td className="py-2">{i.reason ?? ''}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  )
}

export function BulkResultView({ result }: { result: BulkApplied }) {
  const s = result.summary
  return (
    <section aria-label="Bulk result">
      <p role="status" className={s.failed > 0 ? 'mb-2 text-red-700' : 'mb-2'}>
        Applied: {s.ok} ok, {s.failed} failed, {s.skipped} skipped, {s.not_attempted} not attempted
      </p>
      <table className="w-full text-sm" aria-label="Bulk results">
        <tbody>
          {result.results.map((r) => (
            <tr key={r.vm}>
              <td className="py-2 pr-3">{r.vm}</td>
              <td className="py-2 pr-3">{r.status}</td>
              <td className="py-2">{r.error ?? ''}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  )
}
