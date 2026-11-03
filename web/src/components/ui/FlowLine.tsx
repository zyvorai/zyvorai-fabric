// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import type { Tone } from './GlowCard'

const STROKE: Record<Tone, string> = {
  neutral: 'var(--zf-muted)',
  brand: 'var(--zf-brand)',
  proof: 'var(--zf-proof)',
  warn: 'var(--zf-warning)',
  danger: 'var(--zf-danger)',
}

interface FlowLineProps {
  /** SVG path data, in the parent svg's coordinates. */
  d: string
  tone?: Tone
  /** Dashes travel along the line. Off for a finished or idle link. */
  animated?: boolean
  strokeWidth?: number
}

/** One connection line for a diagram, to be placed inside an svg. A moving dash shows traffic or data flowing; a still
    dash shows a link that exists. Decorative: the diagram it belongs to must say the same thing in text. */
export function FlowLine({ d, tone = 'neutral', animated = true, strokeWidth = 1.5 }: FlowLineProps) {
  return (
    <path
      d={d}
      fill="none"
      stroke={STROKE[tone]}
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeOpacity={0.7}
      className={animated ? 'zf-flow' : undefined}
      strokeDasharray={animated ? undefined : '6 8'}
    />
  )
}
