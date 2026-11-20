// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiDelete } from './client'

const API_BASE = '/api'

export interface ScalingPolicy {
  vm_name: string
  enabled: boolean
  cpu_scale_up_threshold?: number
  cpu_scale_down_threshold?: number
  memory_scale_up_threshold?: number
  memory_scale_down_threshold?: number
  min_cpus: number
  max_cpus: number
  min_memory_mb: number
  max_memory_mb: number
  cooldown_secs: number
  last_scale_action?: string
  created: string
}

export interface ScaleEvent {
  id: string
  vm_name: string
  action: 'scale_up' | 'scale_down'
  resource: string
  from_value: string
  to_value: string
  reason: string
  timestamp: string
}

export async function listPolicies(): Promise<ScalingPolicy[]> {
  return apiGet<ScalingPolicy[]>(`${API_BASE}/autoscale`)
}

export async function createPolicy(policy: Omit<ScalingPolicy, 'enabled' | 'created' | 'last_scale_action'>): Promise<ScalingPolicy> {
  return apiPost<ScalingPolicy>(`${API_BASE}/autoscale`, policy)
}

export async function deletePolicy(vmName: string): Promise<void> {
  return apiDelete(`${API_BASE}/autoscale/${vmName}`)
}

export async function listScaleEvents(): Promise<ScaleEvent[]> {
  return apiGet<ScaleEvent[]>(`${API_BASE}/autoscale/events`)
}
