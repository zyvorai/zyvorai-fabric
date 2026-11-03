// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { FlowLine } from './ui'
import { fabricNodes, fleetCounts, fleetSummary, stateTone, type FleetTone, type FleetVM } from '../lib/fleet'

const COLOR: Record<FleetTone, string> = {
  proof: 'var(--zf-proof)',
  warn: 'var(--zf-warning)',
  danger: 'var(--zf-danger)',
  neutral: 'var(--zf-muted)',
}

const CORE = { x: 100, y: 90 }

/** The fleet as a constellation (`decorative` hides it from assistive tech, for a backdrop that says nothing a person needs): a core with one node per real VM (up to eight, running ones first), each in the colour of
    its state. A running VM's line has moving dashes; every other line is still. With no VMs it shows the empty core. The svg
    carries one sentence for a screen reader that says what it shows, and the VM table below lists every VM. */
export function LiveFabric({ vms, decorative = false }: { vms: FleetVM[]; decorative?: boolean }) {
  const counts = fleetCounts(vms)
  const { nodes, overflow } = fabricNodes(vms, { cx: CORE.x, cy: CORE.y, r: 62, max: 8 })
  return (
    <svg
      viewBox="0 0 200 180"
      className="h-full w-full"
      {...(decorative ? { 'aria-hidden': true } : { role: 'img', 'aria-label': fleetSummary(counts) })}
    >
      <defs>
        <radialGradient id="liveFabricCore" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--zf-brand)" stopOpacity="0.55" />
          <stop offset="100%" stopColor="var(--zf-brand)" stopOpacity="0" />
        </radialGradient>
      </defs>
      <circle cx={CORE.x} cy={CORE.y} r="46" fill="url(#liveFabricCore)" />
      {nodes.map((n) => (
        <FlowLine
          key={`l-${n.name}`}
          d={`M${CORE.x} ${CORE.y}L${n.x.toFixed(1)} ${n.y.toFixed(1)}`}
          tone={stateTone(n.state)}
          animated={n.state === 'running'}
        />
      ))}
      <circle cx={CORE.x} cy={CORE.y} r="11" fill="var(--zf-surface)" stroke="var(--zf-brand)" strokeWidth="2" />
      <text x={CORE.x} y={CORE.y + 4} textAnchor="middle" fontSize="11" fontWeight="600" fill="var(--zf-ink)">
        {counts.total}
      </text>
      {nodes.map((n) => (
        <g key={n.name}>
          <circle cx={n.x} cy={n.y} r="8" fill="var(--zf-surface)" stroke={COLOR[stateTone(n.state)]} strokeWidth="2.5" />
          <circle cx={n.x} cy={n.y} r="3" fill={COLOR[stateTone(n.state)]} />
          {!decorative && <title>{`${n.name}: ${n.state}`}</title>}
        </g>
      ))}
      {overflow > 0 && (
        <text x="196" y="174" textAnchor="end" fontSize="9" fill="var(--zf-muted)">
          +{overflow} more
        </text>
      )}
    </svg>
  )
}
