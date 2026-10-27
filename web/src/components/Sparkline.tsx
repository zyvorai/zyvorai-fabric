// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

interface SparklineProps {
  values: number[]
  /** Fixed scale for percentages (0-100); otherwise it fits the data. */
  min?: number
  max?: number
  className?: string
}

const W = 120
const H = 32
const PAD = 3

/** A tiny trend line for a stat tile. Decorative (aria-hidden): the number next to it is the content.
    Fewer than two points draws a dashed baseline, so an empty tile still has a shape. */
export function Sparkline({ values, min, max, className = '' }: SparklineProps) {
  if (values.length < 2) {
    return (
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className={`w-full h-8 ${className}`} aria-hidden>
        <line x1={PAD} y1={H - PAD} x2={W - PAD} y2={H - PAD} stroke="currentColor" strokeOpacity={0.3} strokeDasharray="3 4" strokeLinecap="round" />
      </svg>
    )
  }
  const lo = min ?? Math.min(...values)
  const hi = max ?? Math.max(...values)
  const span = hi - lo || 1
  const x = (i: number) => PAD + (i / (values.length - 1)) * (W - PAD * 2)
  const y = (v: number) => H - PAD - ((v - lo) / span) * (H - PAD * 2)
  const points = values.map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`)
  const last = values.length - 1
  return (
    <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className={`w-full h-8 overflow-visible ${className}`} aria-hidden>
      <polyline points={points.join(' ')} fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" vectorEffect="non-scaling-stroke" />
      <circle cx={x(last)} cy={y(values[last])} r={2.5} fill="currentColor" />
    </svg>
  )
}
