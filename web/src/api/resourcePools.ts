// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiPut, apiPostVoid, apiDelete } from './client'

/** Mirrors the backend's CpuShares/MemoryShares enum: a preset level, or a
    custom numeric value. Serializes to the exact shape serde expects --
    "low"/"normal"/"high" as a bare string, { custom: N } for a custom value. */
export type SharesLevel = 'low' | 'normal' | 'high' | { custom: number }

export function sharesValue(s: SharesLevel): number {
  if (typeof s === 'string') return s === 'low' ? 1000 : s === 'high' ? 4000 : 2000
  return s.custom
}

export function sharesLabel(s: SharesLevel): string {
  if (typeof s === 'string') return s[0].toUpperCase() + s.slice(1)
  return `Custom (${s.custom})`
}

export interface ResourcePool {
  id: string
  name: string
  parent_id?: string
  cluster_id: string
  cpu_shares: SharesLevel
  cpu_reservation_mhz: number
  cpu_limit_mhz?: number
  cpu_expandable_reservation: boolean
  memory_shares: SharesLevel
  memory_reservation_mb: number
  memory_limit_mb?: number
  memory_expandable_reservation: boolean
  vms: string[]
  children: string[]
  created: string
  updated?: string
}

export interface ResourcePoolSummary {
  id: string
  name: string
  parent_id?: string
  cluster_id: string
  cpu_reservation_mhz: number
  cpu_limit_mhz?: number
  cpu_used_mhz: number
  memory_reservation_mb: number
  memory_limit_mb?: number
  memory_used_mb: number
  vm_count: number
  child_pool_count: number
}

export interface AdmissionControlResult {
  admitted: boolean
  reason?: string
  available_cpu: number
  available_memory_mb: number
  requested_cpu: number
  requested_memory_mb: number
}

const API_BASE = '/api'

// Resource pool CRUD

export async function listPools(clusterId?: string): Promise<ResourcePool[]> {
  const url = clusterId
    ? `${API_BASE}/resource-pools?cluster_id=${clusterId}`
    : `${API_BASE}/resource-pools`
  return apiGet<ResourcePool[]>(url)
}

export async function createPool(req: {
  name: string
  cluster_id: string
  parent_id?: string
  cpu_shares: SharesLevel
  cpu_reservation_mhz?: number
  cpu_limit_mhz?: number
  cpu_expandable_reservation?: boolean
  memory_shares: SharesLevel
  memory_reservation_mb?: number
  memory_limit_mb?: number
  memory_expandable_reservation?: boolean
}): Promise<ResourcePool> {
  return apiPost<ResourcePool>(`${API_BASE}/resource-pools`, {
    cpu_reservation_mhz: 0,
    cpu_expandable_reservation: false,
    memory_reservation_mb: 0,
    memory_expandable_reservation: false,
    ...req,
  })
}

export async function getPool(id: string): Promise<ResourcePool> {
  return apiGet<ResourcePool>(`${API_BASE}/resource-pools/${id}`)
}

export async function updatePool(id: string, req: Partial<{
  name: string
  cpu_shares: SharesLevel
  cpu_reservation_mhz: number
  cpu_limit_mhz: number | null
  cpu_expandable_reservation: boolean
  memory_shares: SharesLevel
  memory_reservation_mb: number
  memory_limit_mb: number | null
  memory_expandable_reservation: boolean
}>): Promise<ResourcePool> {
  return apiPut<ResourcePool>(`${API_BASE}/resource-pools/${id}`, req)
}

export async function deletePool(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/resource-pools/${id}`)
}

export async function getPoolSummary(id: string): Promise<ResourcePoolSummary> {
  return apiGet<ResourcePoolSummary>(`${API_BASE}/resource-pools/${id}/summary`)
}

// VM assignment

export async function assignVm(poolId: string, vmName: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/resource-pools/${poolId}/vms`, { vm_name: vmName })
}

export async function unassignVm(poolId: string, vmName: string): Promise<void> {
  return apiDelete(`${API_BASE}/resource-pools/${poolId}/vms/${encodeURIComponent(vmName)}`)
}

export async function moveVm(vmId: string, fromPoolId: string, toPoolId: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/resource-pools/${fromPoolId}/vms/move`, {
    vm_name: vmId,
    target_pool_id: toPoolId,
  })
}

// Admission control

export async function checkAdmission(poolId: string, req: {
  cpu: number
  memory_mb: number
}): Promise<AdmissionControlResult> {
  return apiPost<AdmissionControlResult>(`${API_BASE}/resource-pools/${poolId}/admission`, {
    cpu_mhz: req.cpu,
    memory_mb: req.memory_mb,
  })
}
