// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { GlowCard } from '../ui'
import { barShare } from '../../lib/merionView'
import type { MerionCockpit } from '../../api/agents'

type Budget = NonNullable<MerionCockpit['budget']>

const LABEL: Record<string, string> = {
  day: 'Last 24 hours',
  run: 'This run',
  goal: 'This goal',
  user: 'Your last 24 hours, all agents',
}

/** The model budget as bars: amber from the operator's warning share, red at the cap. Each bar is a meter with a text
    reading next to it, so the colour is never the only signal. */
export function BudgetGauges({ budget }: { budget: Budget }) {
  if (budget.error) {
    return (
      <GlowCard tone="danger" className="p-4">
        <h2 className="text-base font-semibold text-[var(--zf-ink)]">Model budget</h2>
        <p className="mt-1 text-xs text-[var(--zf-danger-text)]">
          The budget could not be read, so model calls are refused: {budget.error}
        </p>
      </GlowCard>
    )
  }
  const tone = budget.reached ? 'danger' : budget.warn ? 'warn' : 'neutral'
  return (
    <GlowCard tone={tone} className="p-4">
      <h2 className="text-base font-semibold text-[var(--zf-ink)]">Model budget</h2>
      <div className="mt-3 space-y-3" data-testid="budget-gauges">
        {(budget.scopes ?? []).map((sc) => {
          const callShare = barShare(sc.calls.used, sc.calls.max)
          const costShare = barShare(sc.cost.used, sc.cost.max)
          const share = Math.max(callShare ?? 0, costShare ?? 0)
          const color = sc.reached ? 'var(--zf-danger)' : sc.warn ? 'var(--zf-warning)' : 'var(--zf-brand)'
          const reading = [
            sc.calls.max != null ? `${sc.calls.used} of ${sc.calls.max} calls` : null,
            sc.cost.max != null ? `${sc.cost.used.toFixed(4)} of ${sc.cost.max} ${sc.cost.currency ?? ''}`.trim() : null,
          ]
            .filter(Boolean)
            .join(' · ')
          const label = LABEL[sc.scope] ?? sc.scope
          return (
            <div key={sc.scope}>
              <div className="flex items-baseline justify-between gap-2 text-sm">
                <span className="font-medium text-[var(--zf-ink)]">{label}</span>
                <span className="tabular-nums text-[var(--zf-secondary)]">{reading}</span>
              </div>
              <div
                role="meter"
                aria-label={`${label}: ${reading}`}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={Math.round(share * 100)}
                className="mt-1 h-2 rounded-full bg-[var(--zf-hairline)]"
              >
                <div className="h-full rounded-full transition-[width] duration-700" style={{ width: `${Math.max(share > 0 ? 3 : 0, share * 100)}%`, background: color }} />
              </div>
              {sc.reached ? (
                <p className="mt-1 text-xs text-[var(--zf-danger-text)]">Limit reached: further calls are refused.</p>
              ) : sc.warn ? (
                <p className="mt-1 text-xs text-[var(--zf-warn-text)]">{Math.round(share * 100)}% used: close to the limit.</p>
              ) : null}
            </div>
          )
        })}
      </div>
    </GlowCard>
  )
}
