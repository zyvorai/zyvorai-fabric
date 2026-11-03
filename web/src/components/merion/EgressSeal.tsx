// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { ShieldAlert, ShieldCheck, ShieldQuestionMark } from 'lucide-react'
import { GlowCard, StatusPulse } from '../ui'
import { dropBars, egressState } from '../../lib/merionView'

interface EgressSealProps {
  /** Count of egress.connect / ebpf.* audit rows, or undefined when the runtime did not report one. */
  connects: number | undefined
  pausedReason?: string | null
  /** FluxVM drop reasons when the dataplane is attached; any other shape draws no bars. */
  dropReasons?: unknown
}

/** The 0 CONNECT proof as a seal: green and still when nothing left the cell, red when something did, grey when the
    runtime did not say. The text carries the whole meaning; the shield and halo repeat it. */
export function EgressSeal({ connects, pausedReason, dropReasons }: EgressSealProps) {
  const state = egressState(connects)
  const bars = dropBars(dropReasons)
  const tone = state === 'sealed' ? 'proof' : state === 'leaked' ? 'danger' : 'neutral'
  const Icon = state === 'sealed' ? ShieldCheck : state === 'leaked' ? ShieldAlert : ShieldQuestionMark
  const headline =
    state === 'sealed' ? '0 CONNECT, sealed' : state === 'leaked' ? `${connects} CONNECT, not sealed` : 'CONNECT count not reported'
  const detail =
    state === 'sealed'
      ? 'No connection left the cell.'
      : state === 'leaked'
        ? `${connects} connection attempt${connects === 1 ? '' : 's'} left the cell. Check the journal for where.`
        : 'This runtime did not report an egress count, so nothing is claimed.'
  return (
    <GlowCard tone={tone} className="p-4" >
      <div className="flex items-center gap-4" data-testid="egress-seal" data-state={state}>
        <div className="relative shrink-0">
          <Icon
            className="h-14 w-14"
            style={{ color: state === 'sealed' ? 'var(--zf-proof)' : state === 'leaked' ? 'var(--zf-danger)' : 'var(--zf-muted)' }}
            strokeWidth={1.5}
            aria-hidden="true"
          />
        </div>
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <StatusPulse tone={tone} live={state === 'sealed'} label={headline} />
            <h2 className="text-lg font-semibold tracking-[-0.016em] text-[var(--zf-ink)]">{headline}</h2>
          </div>
          <p className="mt-0.5 text-sm text-[var(--zf-secondary)]">
            {detail}
            {pausedReason ? ` Agent paused: ${pausedReason}.` : ''}
          </p>
        </div>
      </div>
      {bars.length > 0 && (
        <ul className="mt-3 space-y-1.5 border-t border-[var(--zf-hairline)] pt-3" aria-label="Dropped traffic by reason">
          {bars.map((b) => (
            <li key={b.reason} className="flex items-center gap-2 text-xs text-[var(--zf-secondary)]">
              <span className="w-28 shrink-0 truncate">{b.reason}</span>
              <span className="h-1.5 flex-1 rounded-full bg-[var(--zf-hairline)]">
                <span className="block h-full rounded-full bg-[var(--zf-brand)]" style={{ width: `${Math.max(4, b.share * 100)}%` }} />
              </span>
              <span className="w-8 text-right tabular-nums">{b.count}</span>
            </li>
          ))}
        </ul>
      )}
      <p className="mt-3 text-xs text-[var(--zf-muted)]">
        Counted from the Merion audit (egress.connect and ebpf rows). FluxVM drop reasons are shown when the dataplane is attached.
      </p>
    </GlowCard>
  )
}
