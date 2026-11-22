// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost, apiPut, apiPostVoid, apiDelete } from './client'

export interface ResourceQuota {
  id: string
  name: string
  max_cpus: number
  max_memory: number // MB
  max_disk: number // GB
  max_vms: number
  used_cpus: number
  used_memory: number
  used_disk: number
  used_vms: number
  tags?: string[] // Apply quota to VMs with these tags
  enabled: boolean
  created: string
  updated: string
}

export interface CreateQuotaRequest {
  name: string
  max_cpus: number
  max_memory: number
  max_disk: number
  max_vms: number
  tags?: string[]
  enabled?: boolean
}

export interface QuotaUsage {
  quota_id: string
  quota_name: string
  cpu_percent: number
  memory_percent: number
  disk_percent: number
  vms_percent: number
  is_exceeded: boolean
  exceeded_resources: string[]
}

const API_BASE = '/api'

export async function listQuotas(): Promise<ResourceQuota[]> {
  return apiGet<ResourceQuota[]>(`${API_BASE}/quotas`)
}

export async function getQuota(id: string): Promise<ResourceQuota> {
  return apiGet<ResourceQuota>(`${API_BASE}/quotas/${id}`)
}

export async function createQuota(req: CreateQuotaRequest): Promise<ResourceQuota> {
  return apiPost<ResourceQuota>(`${API_BASE}/quotas`, req)
}

export async function updateQuota(id: string, req: Partial<CreateQuotaRequest>): Promise<ResourceQuota> {
  return apiPut<ResourceQuota>(`${API_BASE}/quotas/${id}`, req)
}

export async function deleteQuota(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/quotas/${id}`)
}

export async function enableQuota(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/quotas/${id}/enable`)
}

export async function disableQuota(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/quotas/${id}/disable`)
}

export async function getQuotaUsage(id: string): Promise<QuotaUsage> {
  return apiGet<QuotaUsage>(`${API_BASE}/quotas/${id}/usage`)
}

export async function getAllQuotaUsage(): Promise<QuotaUsage[]> {
  return apiGet<QuotaUsage[]>(`${API_BASE}/quotas/usage`)
}
