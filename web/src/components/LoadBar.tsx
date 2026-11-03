// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { loadPercent, loadTone, type FleetTone } from '../lib/fleet'

const COLOR: Record<FleetTone, string> = {
  proof: 'var(--zf-proof)',
  warn: 'var(--zf-warning)',
  danger: 'var(--zf-danger)',
  neutral: 'var(--zf-muted)',
}

/** One load reading as a thin bar, amber from 50% and red from 80%. A meter with the number beside it in text, so the colour is
    never the only signal. A missing reading shows a dash and no bar, never a zero. */
export function LoadBar({ label, value }: { label: string; value: number | undefined | null }) {
  const pct = loadPercent(value)
  if (pct === null) return <span className="text-[var(--zf-muted)]">–</span>
  return (
    <span className="flex items-center gap-2">
      <span
        role="meter"
        aria-label={`${label} ${pct}%`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={pct}
        className="h-1.5 w-16 rounded-full bg-[var(--zf-hairline)]"
      >
        <span className="block h-full rounded-full" style={{ width: `${Math.max(pct > 0 ? 4 : 0, pct)}%`, background: COLOR[loadTone(pct)] }} />
      </span>
      <span className="w-9 text-xs tabular-nums text-[var(--zf-secondary)]">{pct}%</span>
    </span>
  )
}
