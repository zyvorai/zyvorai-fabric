// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { ReactNode } from 'react'

export type Tone = 'neutral' | 'brand' | 'proof' | 'warn' | 'danger'

interface GlowCardProps {
  children: ReactNode
  /** A tinted halo that says what state the card is in. Never the only signal: put the state in text too. */
  tone?: Tone
  /** Lifts on hover; use it only for a card that is a link or a button. */
  interactive?: boolean
  className?: string
}

/** A card with depth: a soft sheen, a shadow scale and an optional state halo. Reads the --zf-* tokens, so it works in
    both themes. For plain content keep using Card. */
export function GlowCard({ children, tone = 'neutral', interactive = false, className = '' }: GlowCardProps) {
  return (
    <div className={`zf-glow ${className}`} data-tone={tone} data-interactive={interactive}>
      {children}
    </div>
  )
}
