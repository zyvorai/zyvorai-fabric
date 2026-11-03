// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react'
import { GlowCard } from './ui'
import {
  buildNetworkMap,
  describeConnections,
  netTone,
  type Column,
  type NetNode,
  type NetTone,
  type NetworkMapInput,
} from '../lib/networkMap'

const COLUMNS: Array<{ key: Column; label: string; x: number }> = [
  { key: 'interfaces', label: 'Interfaces', x: 12 },
  { key: 'virtual', label: 'Bonds, VLANs, VXLANs, macvtaps', x: 192 },
  { key: 'bridges', label: 'Bridges', x: 372 },
  { key: 'taps', label: 'Taps', x: 552 },
]
const NODE_W = 156
const NODE_H = 32
const ROW = 42
const TOP = 34

const STROKE: Record<NetTone, string> = { proof: 'var(--zf-proof)', warn: 'var(--zf-warning)', neutral: 'var(--zf-muted)' }
const KIND_ACCENT: Record<NetNode['kind'], string> = {
  interface: 'var(--zf-muted)',
  bond: 'var(--zf-brand)',
  vlan: 'var(--zf-cool)',
  vxlan: 'var(--zf-cool)',
  macvtap: 'var(--zf-cool)',
  bridge: 'var(--zf-brand)',
  tap: 'var(--zf-muted)',
}

const short = (s: string, n = 17) => (s.length > n ? `${s.slice(0, n - 1)}…` : s)

/** How the host's network pieces connect, drawn as four columns: interfaces, then bonds, VLANs, VXLANs and macvtaps, then bridges, then
    taps, with a line wherever the configuration ties two together (a bond's members, a VLAN's parent, a tap's bridge). A node is outlined
    by its networkd state; a line moves only when both ends are up. The picture is decorative: the same connections are listed in
    words for a screen reader, and the tabs below hold every setting. Draws nothing when nothing is configured. */
export function NetworkMap(props: NetworkMapInput) {
  const { links, bonds, vlans, vxlans, macvtaps, bridges, taps, netfiles } = props
  const map = useMemo(
    () => buildNetworkMap({ links, bonds, vlans, vxlans, macvtaps, bridges, taps, netfiles }),
    [links, bonds, vlans, vxlans, macvtaps, bridges, taps, netfiles],
  )
  if (map.nodes.length === 0) return null

  const rows = new Map<Column, NetNode[]>()
  for (const n of map.nodes) rows.set(n.column, [...(rows.get(n.column) ?? []), n])
  const pos = new Map<string, { x: number; y: number }>()
  for (const col of COLUMNS) (rows.get(col.key) ?? []).forEach((n, i) => pos.set(n.id, { x: col.x, y: TOP + i * ROW }))
  const height = TOP + Math.max(...COLUMNS.map((c) => (rows.get(c.key) ?? []).length)) * ROW + 6
  const byId = new Map(map.nodes.map((n) => [n.id, n]))
  const colIndex = (n: NetNode) => COLUMNS.findIndex((c) => c.key === n.column)
  const connections = describeConnections(map)

  return (
    <GlowCard className="mb-6 p-5">
      <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="text-base font-semibold text-[var(--zf-ink)]">How it connects</h2>
        <p className="m-0 flex flex-wrap items-center gap-x-4 text-xs text-[var(--zf-muted)]">
          {([['proof', 'Up'], ['warn', 'Degraded'], ['neutral', 'Down or not reported']] as const).map(([tone, label]) => (
            <span key={tone} className="flex items-center gap-1.5">
              <span className="h-2 w-2 rounded-full" style={{ background: STROKE[tone] }} aria-hidden="true" />
              {label}
            </span>
          ))}
        </p>
      </div>
      <div tabIndex={0} role="region" aria-label="Network map" className="overflow-x-auto rounded-lg">
        <svg viewBox={`0 0 720 ${height}`} className="w-full min-w-[640px]" aria-hidden="true" fill="none">
          {COLUMNS.filter((c) => (rows.get(c.key) ?? []).length > 0).map((c) => (
            <text key={c.key} x={c.x} y="14" fontSize="10" fontWeight="600" fill="var(--zf-muted)">
              {c.label}
            </text>
          ))}
          {map.edges.map((e) => {
            const a = byId.get(e.from)
            const b = byId.get(e.to)
            const pa = pos.get(e.from)
            const pb = pos.get(e.to)
            if (!a || !b || !pa || !pb) return null
            const up = netTone(a.state) === 'proof' && netTone(b.state) === 'proof'
            const ya = pa.y + NODE_H / 2
            const yb = pb.y + NODE_H / 2
            let d: string
            if (a.column === b.column) {
              // two virtual devices in one column (a VLAN on a bond): bow the line out to the left of both
              d = `M${pa.x} ${ya}C${pa.x - 42} ${ya},${pb.x - 42} ${yb},${pb.x} ${yb}`
            } else {
              const [l, r, yl, yr] = colIndex(a) < colIndex(b) ? [pa, pb, ya, yb] : [pb, pa, yb, ya]
              const x1 = l.x + NODE_W
              const x2 = r.x
              const mx = (x1 + x2) / 2
              d = `M${x1} ${yl}C${mx} ${yl},${mx} ${yr},${x2} ${yr}`
            }
            return (
              <path
                key={`${e.from}>${e.to}`}
                d={d}
                stroke={up ? 'var(--zf-proof)' : 'var(--zf-hairline)'}
                strokeWidth="1.6"
                strokeLinecap="round"
                strokeDasharray={up ? undefined : '4 5'}
                className={up ? 'zf-flow' : undefined}
              />
            )
          })}
          {map.nodes.map((n) => {
            const p = pos.get(n.id)!
            const tone = netTone(n.state)
            return (
              <g key={n.id}>
                <rect x={p.x} y={p.y} width={NODE_W} height={NODE_H} rx="8" fill="var(--zf-surface)" stroke={STROKE[tone]} strokeWidth="1.6" strokeDasharray={n.missing ? '3 3' : undefined} />
                <rect x={p.x} y={p.y + 6} width="3" height={NODE_H - 12} rx="1.5" fill={KIND_ACCENT[n.kind]} />
                <text x={p.x + 12} y={p.y + 14} fontSize="11" fontWeight="600" fill="var(--zf-ink)">{short(n.name)}</text>
                <text x={p.x + 12} y={p.y + 26} fontSize="9" fill="var(--zf-muted)">{n.missing ? `${n.kind} (not listed)` : n.kind}</text>
                <title>{`${n.name}: ${n.kind}${n.state ? `, ${n.state}` : ''}`}</title>
              </g>
            )
          })}
        </svg>
      </div>
      <ul className="sr-only" aria-label="Connections">
        {connections.length === 0 ? <li>Nothing is connected to anything else yet.</li> : connections.map((c) => <li key={c}>{c}</li>)}
      </ul>
    </GlowCard>
  )
}
