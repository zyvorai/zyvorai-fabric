// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/** Pure helpers behind the dashboard's fleet visuals, kept apart from the components so they can be tested without a DOM. */

export interface FleetVM {
  name: string
  state: string
}

export type FleetTone = 'proof' | 'warn' | 'danger' | 'neutral'

/** The tone a VM state is drawn in. Only a failure is red: a stopped VM is a normal state, not an error. */
export function stateTone(state: string): FleetTone {
  switch (state) {
    case 'running':
      return 'proof'
    case 'paused':
    case 'starting':
    case 'stopping':
      return 'warn'
    case 'failed':
      return 'danger'
    default:
      return 'neutral'
  }
}

export interface FleetCounts {
  total: number
  running: number
  paused: number
  stopped: number
  failed: number
  /** Everything else: starting, stopping, unknown. */
  other: number
}

export function fleetCounts(vms: FleetVM[]): FleetCounts {
  const c: FleetCounts = { total: vms.length, running: 0, paused: 0, stopped: 0, failed: 0, other: 0 }
  for (const v of vms) {
    if (v.state === 'running') c.running++
    else if (v.state === 'paused') c.paused++
    else if (v.state === 'stopped') c.stopped++
    else if (v.state === 'failed') c.failed++
    else c.other++
  }
  return c
}

/** One sentence for a screen reader: the graphic and the donut say the same thing in words. */
export function fleetSummary(c: FleetCounts): string {
  if (c.total === 0) return 'No virtual machines yet'
  const parts = [
    c.running && `${c.running} running`,
    c.paused && `${c.paused} paused`,
    c.stopped && `${c.stopped} stopped`,
    c.failed && `${c.failed} failed`,
    c.other && `${c.other} in transition or unknown`,
  ].filter(Boolean)
  return `${c.total} virtual machine${c.total === 1 ? '' : 's'}: ${parts.join(', ')}`
}

export interface FabricNode {
  name: string
  state: string
  x: number
  y: number
}

/** Up to `max` VMs placed evenly on a ring around the core at (cx, cy), starting at the top and going clockwise, and how
    many did not fit. Running VMs come first so the ones doing work are never the ones dropped. */
export function fabricNodes(
  vms: FleetVM[],
  opts: { cx: number; cy: number; r: number; max: number },
): { nodes: FabricNode[]; overflow: number } {
  const rank = (s: string) => (s === 'running' ? 0 : s === 'failed' ? 1 : s === 'paused' ? 2 : 3)
  const sorted = [...vms].sort((a, b) => rank(a.state) - rank(b.state) || a.name.localeCompare(b.name))
  const shown = sorted.slice(0, opts.max)
  const n = shown.length
  const nodes = shown.map((v, i) => {
    const a = -Math.PI / 2 + (i * 2 * Math.PI) / Math.max(n, 1)
    return { name: v.name, state: v.state, x: opts.cx + opts.r * Math.cos(a), y: opts.cy + opts.r * Math.sin(a) }
  })
  return { nodes, overflow: Math.max(0, sorted.length - shown.length) }
}

/** Load as a whole percentage, clamped to 0 to 100. A missing or non-finite reading is `null`, never drawn as zero. */
export function loadPercent(value: number | undefined | null): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value)) return null
  return Math.max(0, Math.min(100, Math.round(value)))
}

/** The tone of a load bar: amber from 50, red from 80. */
export function loadTone(pct: number): FleetTone {
  return pct >= 80 ? 'danger' : pct >= 50 ? 'warn' : 'proof'
}

/** One VM's latest reading as CPU % and memory as a % of the VM's own allocation (`memoryMiB`). The runtime reports memory use in
    bytes, so it has to be set against what the VM was given. A VM with no allocation has no memory share (`null`), never zero. */
export function vmLoadFrom(
  vm: { memory: number },
  m: { cpu_usage: number; memory_usage: number },
): { cpu: number; mem: number | null } {
  return { cpu: m.cpu_usage, mem: vm.memory > 0 ? (m.memory_usage / (vm.memory * 1024 * 1024)) * 100 : null }
}

export interface FleetSegment {
  key: 'running' | 'paused' | 'stopped' | 'failed' | 'other'
  label: string
  count: number
  /** Share of the fleet, 0 to 1. */
  share: number
}

/** The fleet as ordered segments for a proportional bar. Segments with no VMs are left out; an empty fleet has none. */
export function fleetSegments(c: FleetCounts): FleetSegment[] {
  if (c.total === 0) return []
  const all: Array<[FleetSegment['key'], string, number]> = [
    ['running', 'running', c.running],
    ['paused', 'paused', c.paused],
    ['stopped', 'stopped', c.stopped],
    ['failed', 'failed', c.failed],
    ['other', 'other', c.other],
  ]
  return all.filter(([, , n]) => n > 0).map(([key, label, count]) => ({ key, label, count, share: count / c.total }))
}
