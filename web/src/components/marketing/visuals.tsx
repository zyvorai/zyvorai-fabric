// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { FlowLine } from '../ui'
import { LiveFabric } from '../LiveFabric'
import { SealedPipeline } from '../merion/SealedPipeline'
import { SAMPLE_FLEET } from '../../lib/sampleFleet'

/** Virtual machines: the live fleet map on the invented sample fleet. */
export function VmVisual() {
  return (
    <div className="mx-auto h-52 w-full max-w-xs">
      <LiveFabric vms={SAMPLE_FLEET} decorative />
    </div>
  )
}

/** Network fabric: a floating IP above a bridge, with three VMs under it and traffic moving down the running links. */
export function NetworkVisual() {
  const vms = [40, 120, 200]
  return (
    <svg viewBox="0 0 240 170" className="mx-auto h-52 w-full max-w-xs" fill="none">
      <rect x="86" y="8" width="68" height="22" rx="11" fill="var(--zf-surface)" stroke="var(--zf-brand)" strokeWidth="2" />
      <text x="120" y="23" textAnchor="middle" fontSize="10" fontWeight="600" fill="var(--zf-ink)">floating IP</text>
      <FlowLine d="M120 30V68" tone="brand" />
      <rect x="20" y="68" width="200" height="26" rx="8" fill="var(--zf-surface)" stroke="var(--zf-cool)" strokeWidth="2" />
      <text x="120" y="85" textAnchor="middle" fontSize="10" fontWeight="600" fill="var(--zf-ink)">bridge · VLANs · policy</text>
      {vms.map((x, i) => (
        <g key={x}>
          <FlowLine d={`M${x} 94V128`} tone={i === 2 ? 'neutral' : 'proof'} animated={i !== 2} />
          <circle cx={x} cy="138" r="11" fill="var(--zf-surface)" stroke={i === 2 ? 'var(--zf-muted)' : 'var(--zf-proof)'} strokeWidth="2.5" />
          <circle cx={x} cy="138" r="3.5" fill={i === 2 ? 'var(--zf-muted)' : 'var(--zf-proof)'} />
        </g>
      ))}
    </svg>
  )
}

/** Storage and resilience: a primary volume replicating to a second site, with a backup beside it. */
export function StorageVisual() {
  const disk = (x: number, y: number, color: string, label: string) => (
    <g>
      <ellipse cx={x} cy={y} rx="26" ry="8" fill="var(--zf-surface)" stroke={color} strokeWidth="2" />
      <path d={`M${x - 26} ${y}V${y + 24}a26 8 0 0 0 52 0V${y}`} fill="var(--zf-surface)" stroke={color} strokeWidth="2" />
      <ellipse cx={x} cy={y + 24} rx="26" ry="8" fill="none" stroke={color} strokeWidth="2" opacity="0.5" />
      <text x={x} y={y + 56} textAnchor="middle" fontSize="10" fill="var(--zf-secondary)">{label}</text>
    </g>
  )
  return (
    <svg viewBox="0 0 240 172" className="mx-auto h-52 w-full max-w-xs" fill="none">
      {disk(44, 40, 'var(--zf-proof)', 'primary')}
      {disk(196, 40, 'var(--zf-cool)', 'replica')}
      {disk(120, 104, 'var(--zf-warning)', 'backup')}
      <FlowLine d="M72 48H168" tone="proof" />
      <FlowLine d="M62 76L100 100" tone="warn" />
    </svg>
  )
}

/** Merion: the sealed-cell pipeline finished, with the proof under it. */
export function MerionVisual() {
  return (
    <div className="space-y-4">
      <SealedPipeline stage="done" labels={{ file: 'Your PDF', cell: 'Sealed cell', extract: 'Extract', artifact: 'Brief' }} />
      <p className="m-0 text-center text-sm font-semibold text-[var(--zf-proof-text)]">0 CONNECT, sealed</p>
    </div>
  )
}

/** Platform: four ways in around one daemon, with traffic moving along each line. */
export function HubVisual() {
  const spokes = [
    { label: 'Web', x: 40, y: 40 },
    { label: 'CLI', x: 200, y: 40 },
    { label: 'Operator', x: 40, y: 130 },
    { label: 'Terraform', x: 200, y: 130 },
  ]
  return (
    <svg viewBox="0 0 240 170" className="mx-auto h-72 w-full max-w-2xl" fill="none" aria-hidden="true">
      {spokes.map((s) => (
        <FlowLine key={s.label} d={`M${s.x} ${s.y}L120 85`} tone="brand" />
      ))}
      <circle cx="120" cy="85" r="30" fill="var(--zf-surface)" stroke="var(--zf-brand)" strokeWidth="2.5" />
      <text x="120" y="83" textAnchor="middle" fontSize="9" fontWeight="600" fill="var(--zf-ink)">zyvor-</text>
      <text x="120" y="95" textAnchor="middle" fontSize="9" fontWeight="600" fill="var(--zf-ink)">fabricd</text>
      {spokes.map((s) => (
        <g key={s.label}>
          <rect x={s.x - 30} y={s.y - 12} width="60" height="24" rx="12" fill="var(--zf-surface)" stroke="var(--zf-hairline)" strokeWidth="1.5" />
          <text x={s.x} y={s.y + 4} textAnchor="middle" fontSize="10" fill="var(--zf-secondary)">{s.label}</text>
        </g>
      ))}
    </svg>
  )
}
