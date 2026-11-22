// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

// Capacity what-if maths. Mirrors backend/zyvor-fabricd/src/api/capacity_whatif.rs
// (fit_vms); both are tested against src/lib/fixtures/capacity-whatif.json.

export interface WhatIfHost { name: string; vcpus: number; memory_mb: number; disk_gb: number | null }
export interface ExistingVm { host: string | null; vcpus: number; memory_mb: number; disk_gb: number }
export interface NewVmSpec { name: string; vcpus: number; memory_mb: number; disk_gb: number; count: number }
export interface Overcommit { cpu_ratio: number; memory_ratio: number; storage_ratio: number }
export interface Usage { vcpus: number; memory_mb: number; disk_gb: number }
export interface Room { vcpus: number; memory_mb: number; disk_gb: number | null }
export interface Placement { name: string; index: number; host: string }
export interface Unplaced { name: string; index: number; reasons: string[] }
export interface HostReport { name: string; effective: Room; used_before: Usage; used_after: Usage; headroom_after: Room }
export interface FitReport {
  requested: number
  fitted: number
  all_fit: boolean
  placements: Placement[]
  unplaced: Unplaced[]
  hosts: HostReport[]
}

export const NO_OVERCOMMIT: Overcommit = { cpu_ratio: 1, memory_ratio: 1, storage_ratio: 1 }

const scale = (total: number, ratio: number): number => Math.floor(total * ratio)
const sub = (a: number, b: number): number => Math.max(0, a - b)

/** First-fit-decreasing placement of new VMs; never mutates its inputs. */
export function fitVms(
  hosts: WhatIfHost[],
  existing: ExistingVm[],
  newVms: NewVmSpec[],
  overcommit: Overcommit,
): FitReport {
  const eff: Room[] = hosts.map((h) => ({
    vcpus: scale(h.vcpus, overcommit.cpu_ratio),
    memory_mb: scale(h.memory_mb, overcommit.memory_ratio),
    disk_gb: h.disk_gb === null ? null : scale(h.disk_gb, overcommit.storage_ratio),
  }))
  const used: Usage[] = hosts.map(() => ({ vcpus: 0, memory_mb: 0, disk_gb: 0 }))
  if (hosts.length > 0) {
    for (const vm of existing) {
      const found = vm.host === null ? -1 : hosts.findIndex((h) => h.name === vm.host)
      const i = found < 0 ? 0 : found
      used[i].vcpus += vm.vcpus
      used[i].memory_mb += vm.memory_mb
      used[i].disk_gb += vm.disk_gb
    }
  }
  const usedBefore = used.map((u) => ({ ...u }))

  const queue: { spec: NewVmSpec; index: number }[] = []
  for (const spec of newVms) for (let i = 1; i <= spec.count; i++) queue.push({ spec, index: i })
  queue.sort((a, b) =>
    (b.spec.memory_mb - a.spec.memory_mb) ||
    (b.spec.vcpus - a.spec.vcpus) ||
    (b.spec.disk_gb - a.spec.disk_gb) ||
    (a.spec.name < b.spec.name ? -1 : a.spec.name > b.spec.name ? 1 : 0) ||
    (a.index - b.index),
  )

  const free = (i: number): Room => ({
    vcpus: sub(eff[i].vcpus, used[i].vcpus),
    memory_mb: sub(eff[i].memory_mb, used[i].memory_mb),
    disk_gb: eff[i].disk_gb === null ? null : sub(eff[i].disk_gb, used[i].disk_gb),
  })
  const idx = hosts.map((_, i) => i)

  const placements: Placement[] = []
  const unplaced: Unplaced[] = []
  for (const { spec, index } of queue) {
    const target = idx.find((i) => {
      const f = free(i)
      return spec.vcpus <= f.vcpus && spec.memory_mb <= f.memory_mb && (f.disk_gb === null || spec.disk_gb <= f.disk_gb)
    })
    if (target !== undefined) {
      used[target].vcpus += spec.vcpus
      used[target].memory_mb += spec.memory_mb
      used[target].disk_gb += spec.disk_gb
      placements.push({ name: spec.name, index, host: hosts[target].name })
      continue
    }
    const reasons: string[] = []
    if (hosts.length === 0) {
      reasons.push('no_hosts')
    } else {
      if (idx.every((i) => spec.vcpus > free(i).vcpus)) reasons.push('vcpu')
      if (idx.every((i) => spec.memory_mb > free(i).memory_mb)) reasons.push('memory')
      if (idx.every((i) => { const d = free(i).disk_gb; return d !== null && spec.disk_gb > d })) reasons.push('disk')
      if (reasons.length === 0) reasons.push('no_single_host')
    }
    unplaced.push({ name: spec.name, index, reasons })
  }

  return {
    requested: queue.length,
    fitted: placements.length,
    all_fit: unplaced.length === 0,
    placements,
    unplaced,
    hosts: hosts.map((h, i) => ({
      name: h.name,
      effective: eff[i],
      used_before: usedBefore[i],
      used_after: { ...used[i] },
      headroom_after: free(i),
    })),
  }
}

// ---- helpers for the form and the offline fallback -------------------------

export interface CapacityMetric { label: string; used: number; total: number; unit: string }

/**
 * Build a single-host model from the metrics the Capacity page already shows.
 * Returns null when CPU or memory totals are unknown (nothing sensible to simulate).
 * Overcommit is not applied: the metrics already report effective totals.
 */
export function hostFromMetrics(metrics: CapacityMetric[]): { host: WhatIfHost; existing: ExistingVm[] } | null {
  const find = (re: RegExp) => metrics.find((m) => re.test(m.label.toLowerCase()))
  const cpu = find(/cpu|core/)
  const mem = find(/memory|ram/)
  const disk = find(/storage|disk/)
  if (!cpu || !mem || cpu.total <= 0 || mem.total <= 0) return null
  const memMb = (m: CapacityMetric) => Math.round(m.unit.toUpperCase() === 'GB' ? m.total * 1024 : m.total)
  const memUsedMb = Math.round(mem.unit.toUpperCase() === 'GB' ? mem.used * 1024 : mem.used)
  const diskOk = disk && disk.total > 0
  return {
    host: {
      name: 'local',
      vcpus: Math.floor(cpu.total),
      memory_mb: memMb(mem),
      disk_gb: diskOk ? Math.floor(disk.total) : null,
    },
    existing: [{
      host: null,
      vcpus: Math.ceil(cpu.used),
      memory_mb: memUsedMb,
      disk_gb: diskOk ? Math.ceil(disk.used) : 0,
    }],
  }
}

export interface WhatIfFormRow { name: string; vcpus: string; memory_mb: string; disk_gb: string; count: string }

export const EMPTY_ROW: WhatIfFormRow = { name: '', vcpus: '2', memory_mb: '4096', disk_gb: '40', count: '1' }

/** Parse form rows into specs, or return a human-readable error. */
export function parseRows(rows: WhatIfFormRow[]): { specs: NewVmSpec[] } | { error: string } {
  const specs: NewVmSpec[] = []
  for (const [i, r] of rows.entries()) {
    const label = `Row ${i + 1}`
    const nums = [r.vcpus, r.memory_mb, r.disk_gb, r.count].map((v) => (/^\d+$/.test(v.trim()) ? Number(v) : NaN))
    if (!r.name.trim()) return { error: `${label}: enter a name` }
    if (nums.some((n) => Number.isNaN(n))) return { error: `${label}: vCPUs, memory, disk and count must be whole numbers` }
    const [vcpus, memory_mb, disk_gb, count] = nums
    if (vcpus < 1 || memory_mb < 1 || count < 1) return { error: `${label}: vCPUs, memory and count must be at least 1` }
    if (count > 1000) return { error: `${label}: count is limited to 1000` }
    specs.push({ name: r.name.trim(), vcpus, memory_mb, disk_gb, count })
  }
  if (specs.length === 0) return { error: 'Add at least one VM' }
  return { specs }
}

const REASON_TEXT: Record<string, string> = {
  vcpu: 'not enough vCPUs',
  memory: 'not enough memory',
  disk: 'not enough disk',
  no_single_host: 'each resource fits somewhere, but not all on one host',
  no_hosts: 'no host capacity is known',
}

export function reasonText(reasons: string[]): string {
  return reasons.map((r) => REASON_TEXT[r] ?? r).join(', ')
}

export function summarizeReport(r: FitReport): string {
  if (r.requested === 0) return 'Nothing to simulate'
  if (r.all_fit) return `All ${r.requested} VM${r.requested === 1 ? '' : 's'} fit`
  return `${r.fitted} of ${r.requested} VMs fit`
}

// ---- /api/system/capacity mapping and density ------------------------------

export interface CapacityResponseLike {
  physical: { cpus: number; memory_mb: number }
  effective: { cpus: number; memory_mb: number }
  allocated: { cpus: number; memory_mb: number }
  available: { cpus: number; memory_mb: number }
  overcommit: { enabled: boolean; cpu_ratio: number; memory_ratio: number }
  vm_count: number
}

export interface UsageMetric { label: string; used: number; total: number; unit: string; trend: number; projected_full?: string }

/** Map the daemon shape to metric cards: used = allocated, total = effective capacity. Memory in GB. */
export function metricsFromCapacity(c: CapacityResponseLike): UsageMetric[] {
  const gb = (mb: number) => mb / 1024
  return [
    { label: 'CPU Cores', used: c.allocated.cpus, total: c.effective.cpus, unit: 'cores', trend: 0 },
    { label: 'Memory', used: gb(c.allocated.memory_mb), total: gb(c.effective.memory_mb), unit: 'GB', trend: 0 },
  ]
}

export type DensityLevel = 'ok' | 'warn' | 'critical'
export interface DensityRow {
  resource: 'CPU' | 'Memory'
  /** allocated / physical */
  commit_ratio: number
  /** configured ratio when overcommit is enabled, otherwise 1 */
  limit_ratio: number
  /** allocated / effective, percent */
  utilization_pct: number
  headroom: number
  unit: string
  level: DensityLevel
  warning: string | null
}
export interface DensityReport {
  vm_count: number
  vcpus_per_vm: number | null
  memory_gb_per_vm: number | null
  /** how many more VMs of the current average size fit in the remaining headroom */
  more_vms_fit: number | null
  overcommit_enabled: boolean
  rows: DensityRow[]
}

/** Warn when the commit ratio reaches this share of the allowed ratio. */
export const DENSITY_WARN_SHARE = 0.9

function densityRow(resource: 'CPU' | 'Memory', physical: number, effective: number, allocated: number, available: number, enabled: boolean, ratio: number, unit: string): DensityRow {
  const commit = physical > 0 ? allocated / physical : 0
  const limit = enabled ? ratio : 1
  let level: DensityLevel = 'ok'
  let warning: string | null = null
  if (physical > 0 && commit > limit + 1e-9) {
    level = 'critical'
    warning = enabled
      ? `${resource} is committed ${commit.toFixed(2)}:1, above the configured ${limit.toFixed(2)}:1 limit`
      : `${resource} is committed ${commit.toFixed(2)}:1 but overcommit is disabled`
  } else if (physical > 0 && commit >= limit * DENSITY_WARN_SHARE && commit > 1) {
    level = 'warn'
    warning = `${resource} commit ${commit.toFixed(2)}:1 is near the ${limit.toFixed(2)}:1 limit`
  } else if (physical > 0 && limit === 1 && commit >= DENSITY_WARN_SHARE) {
    level = 'warn'
    warning = `${resource} is ${(commit * 100).toFixed(0)}% allocated with no overcommit headroom`
  }
  return {
    resource,
    commit_ratio: commit,
    limit_ratio: limit,
    utilization_pct: effective > 0 ? (allocated / effective) * 100 : 0,
    headroom: available,
    unit,
    level,
    warning,
  }
}

export function densityReport(c: CapacityResponseLike): DensityReport {
  const { enabled, cpu_ratio, memory_ratio } = c.overcommit
  const rows = [
    densityRow('CPU', c.physical.cpus, c.effective.cpus, c.allocated.cpus, c.available.cpus, enabled, cpu_ratio, 'vCPUs'),
    densityRow('Memory', c.physical.memory_mb / 1024, c.effective.memory_mb / 1024, c.allocated.memory_mb / 1024, c.available.memory_mb / 1024, enabled, memory_ratio, 'GB'),
  ]
  const n = c.vm_count
  const avgCpu = n > 0 ? c.allocated.cpus / n : null
  const avgMemMb = n > 0 ? c.allocated.memory_mb / n : null
  let more: number | null = null
  if (avgCpu && avgMemMb && avgCpu > 0 && avgMemMb > 0) {
    more = Math.floor(Math.min(c.available.cpus / avgCpu, c.available.memory_mb / avgMemMb))
  }
  return {
    vm_count: n,
    vcpus_per_vm: avgCpu,
    memory_gb_per_vm: avgMemMb === null ? null : avgMemMb / 1024,
    more_vms_fit: more,
    overcommit_enabled: enabled,
    rows,
  }
}
