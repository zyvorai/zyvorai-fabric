// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

export interface DonutSegment {
  label: string
  value: number
  /** A CSS colour, normally a token such as var(--zf-success). */
  color: string
}

interface DonutProps {
  segments: DonutSegment[]
  size?: number
  thickness?: number
  /** Big text in the middle, such as the total. */
  center?: string
  centerLabel?: string
}

/** A status breakdown ring. The segments are listed in an sr-only table too, because a ring alone does not tell a screen
    reader the numbers. Zero total draws an empty track. */
export function Donut({ segments, size = 140, thickness = 14, center, centerLabel }: DonutProps) {
  const total = segments.reduce((s, x) => s + Math.max(0, x.value), 0)
  const r = (size - thickness) / 2
  const c = 2 * Math.PI * r
  let offset = 0
  const gap = total > 0 && segments.filter((s) => s.value > 0).length > 1 ? 3 : 0
  return (
    <div className="relative shrink-0" style={{ width: size, height: size }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="-rotate-90" aria-hidden="true">
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--zf-hairline)" strokeWidth={thickness} />
        {total > 0 &&
          segments.map((s) => {
            if (s.value <= 0) return null
            const len = (s.value / total) * c
            const dash = Math.max(0, len - gap)
            const el = (
              <circle
                key={s.label}
                cx={size / 2}
                cy={size / 2}
                r={r}
                fill="none"
                stroke={s.color}
                strokeWidth={thickness}
                strokeLinecap="butt"
                strokeDasharray={`${dash} ${c - dash}`}
                strokeDashoffset={-offset}
              />
            )
            offset += len
            return el
          })}
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center" aria-hidden="true">
        {center && <span className="text-2xl font-semibold tabular-nums leading-none text-[var(--zf-ink)]">{center}</span>}
        {centerLabel && <span className="text-xs mt-1 text-[var(--zf-muted)]">{centerLabel}</span>}
      </div>
      <table className="sr-only">
        <caption>{centerLabel ?? 'Breakdown'}</caption>
        <tbody>
          {segments.map((s) => (
            <tr key={s.label}>
              <th scope="row">{s.label}</th>
              <td>{s.value}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
