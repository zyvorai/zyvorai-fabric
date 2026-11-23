// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet } from './client'

export interface HostCpuRecord {
  host_id: string
  vendor: string
  model_name: string
  flags: string[]
  source: string
  updated: string
}

export interface CpuInventory {
  hosts: { host_id: string; hostname: string; cluster_id: string; cpu: HostCpuRecord | null }[]
  clusters: {
    cluster_id: string
    hosts_total: number
    hosts_known: number
    baseline_model: string | null
    common_features: number
  }[]
  vms: { name: string; configured: boolean; model?: string; mode?: string }[]
  vms_truncated: boolean
}

export type CpuVerdict =
  | { verdict: 'compatible' }
  | { verdict: 'unknown' }
  | { verdict: 'incompatible'; missing: string[]; vendor_mismatch: boolean }

export interface CpuCompat {
  vm: string
  configured: boolean
  mode?: string
  model?: string
  requirement: { vendor?: string; features: string[]; notes: string[] }
  results: { host_id: string; hostname: string; result: CpuVerdict }[]
}

export function getCpuInventory(): Promise<CpuInventory> {
  return apiGet('/api/cpu/inventory')
}

export function getCpuCompat(vm: string, host?: string): Promise<CpuCompat> {
  const q = host ? `?host=${encodeURIComponent(host)}` : ''
  return apiGet(`/api/vms/${encodeURIComponent(vm)}/cpu-compat${q}`)
}

export function describeVerdict(v: CpuVerdict): string {
  switch (v.verdict) {
    case 'compatible':
      return 'Compatible'
    case 'unknown':
      return 'Unknown (no CPU inventory for this host)'
    case 'incompatible': {
      const parts: string[] = []
      if (v.vendor_mismatch) parts.push('CPU vendor differs')
      if (v.missing.length > 0) parts.push(`missing ${v.missing.join(', ')}`)
      return `Incompatible: ${parts.join('; ')}`
    }
  }
}
