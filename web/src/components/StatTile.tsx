// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react'
import { Link } from 'react-router'
import { Sparkline } from './Sparkline'

interface StatTileProps {
  label: string
  value: ReactNode
  /** One muted line under the number. */
  detail?: ReactNode
  to: string
  history: number[]
  min?: number
  max?: number
  /** Colour of the sparkline; a CSS colour or var(). */
  tone?: string
}

/** A dashboard number: big, one label, one line of detail, a sparkline, and the whole tile links to its page. */
export function StatTile({ label, value, detail, to, history, min, max, tone = 'var(--zf-link)' }: StatTileProps) {
  return (
    <Link
      to={to}
      className="zf-panel group flex flex-col gap-1 p-5 no-underline"
    >
      <span className="text-[13px] font-medium text-[var(--zf-secondary)]">{label}</span>
      <span className="text-[44px] leading-none font-semibold tracking-[-0.03em] tabular-nums text-[var(--zf-ink)]">{value}</span>
      <span className="text-[13px] text-[var(--zf-muted)] min-h-[1.25rem]">{detail}</span>
      <span className="mt-2 block" style={{ color: tone }}>
        <Sparkline values={history} min={min} max={max} />
      </span>
    </Link>
  )
}
