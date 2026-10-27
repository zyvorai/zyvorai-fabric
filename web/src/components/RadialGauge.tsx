// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

interface RadialGaugeProps {
  /** 0-100 */
  percent: number
  size?: number
  strokeWidth?: number
  color?: string
  trackColor?: string
  label?: string
  sublabel?: string
}

/** Compact circular progress ring -- used for "fleet health" style summaries
    where a single glanceable percentage says more than another stat tile. */
export function RadialGauge({
  percent,
  size = 96,
  strokeWidth = 8,
  color = '#34d399',
  trackColor = 'rgba(148, 163, 184, 0.18)',
  label,
  sublabel,
}: RadialGaugeProps) {
  const clamped = Math.max(0, Math.min(100, percent))
  const radius = (size - strokeWidth) / 2
  const circumference = 2 * Math.PI * radius
  const offset = circumference * (1 - clamped / 100)

  return (
    <div className="relative shrink-0" style={{ width: size, height: size }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="-rotate-90">
        <circle cx={size / 2} cy={size / 2} r={radius} fill="none" stroke={trackColor} strokeWidth={strokeWidth} />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke={color}
          strokeWidth={strokeWidth}
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={offset}
          style={{ transition: 'stroke-dashoffset 0.8s cubic-bezier(0.16, 1, 0.3, 1)', filter: `drop-shadow(0 0 6px ${color}90)` }}
        />
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center">
        <span className="text-lg font-bold text-[#1d1d1f] tabular-nums leading-none">{Math.round(clamped)}%</span>
        {label && <span className="text-xs text-[#6e6e73] mt-1 leading-none">{label}</span>}
      </div>
      {sublabel && <span className="sr-only">{sublabel}</span>}
    </div>
  )
}
