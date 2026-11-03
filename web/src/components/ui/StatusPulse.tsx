// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import type { Tone } from './GlowCard'

const COLOR: Record<Tone, string> = {
  neutral: 'var(--zf-muted)',
  brand: 'var(--zf-brand)',
  proof: 'var(--zf-proof)',
  warn: 'var(--zf-warning)',
  danger: 'var(--zf-danger)',
}

interface StatusPulseProps {
  tone?: Tone
  /** Ring that expands from the dot. Use it for something live (running, connected), not for a finished state. */
  live?: boolean
  /** The state in words, read by screen readers. Colour is never the only signal. */
  label: string
}

export function StatusPulse({ tone = 'neutral', live = false, label }: StatusPulseProps) {
  return (
    <span role="img" aria-label={label} className="inline-flex items-center">
      <span className="zf-pulse" data-live={live} style={{ color: COLOR[tone] }} />
    </span>
  )
}
