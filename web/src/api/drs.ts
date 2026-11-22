// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost, apiPut, apiPostVoid, apiDelete } from './client'

export interface DrsConfig {
  cluster_id: string
  enabled: boolean
  automation_level: 'manual' | 'partially_automated' | 'fully_automated'
  migration_threshold: number
  balance_cpu_weight: number
  balance_memory_weight: number
  check_interval_seconds: number
  min_improvement_pct: number
  updated?: string
}

export interface PlacementRequest {
  cluster_id: string
  vm_cpus: number
  vm_memory_mb: number
  affinity_rules?: string[]
  anti_affinity_rules?: string[]
  preferred_hosts?: string[]
}

export interface PlacementResult {
  host_id: string
  hostname: string
  score: number
  reason: string
  cpu_after_pct: number
  memory_after_pct: number
  alternatives: Array<{
    host_id: string
    hostname: string
    score: number
  }>
}

export interface MigrationRecommendation {
  id: string
  cluster_id: string
  vm_id: string
  vm_name: string
  source_host_id: string
  source_hostname: string
  target_host_id: string
  target_hostname: string
  reason: string
  priority: 'low' | 'medium' | 'high' | 'critical'
  estimated_benefit_pct: number
  status: 'pending' | 'approved' | 'rejected' | 'executing' | 'completed' | 'failed'
  created: string
  executed_at?: string
}

export interface AffinityRule {
  id: string
  cluster_id: string
  name: string
  rule_type: 'affinity' | 'anti_affinity'
  mandatory: boolean
  vm_ids: string[]
  host_ids?: string[]
  enabled: boolean
  created: string
  updated?: string
}

export interface ClusterBalance {
  cluster_id: string
  cpu_imbalance_pct: number
  memory_imbalance_pct: number
  overall_score: number
  hosts: Array<{
    host_id: string
    hostname: string
    cpu_usage_pct: number
    memory_usage_pct: number
    vm_count: number
    deviation_from_mean_cpu: number
    deviation_from_mean_memory: number
  }>
  recommendation_count: number
  last_analyzed: string
}

const API_BASE = '/api'

// DRS configuration

export async function configureDrs(req: Partial<DrsConfig> & { cluster_id: string }): Promise<DrsConfig> {
  return apiPost<DrsConfig>(`${API_BASE}/drs/config`, req)
}

export async function getDrsConfig(clusterId: string): Promise<DrsConfig> {
  return apiGet<DrsConfig>(`${API_BASE}/drs/config/${clusterId}`)
}

// Placement

export async function computePlacement(req: PlacementRequest): Promise<PlacementResult> {
  return apiPost<PlacementResult>(`${API_BASE}/drs/placement`, req)
}

// Balance analysis

export async function analyzeBalance(clusterId: string): Promise<ClusterBalance> {
  return apiGet<ClusterBalance>(`${API_BASE}/drs/balance/${clusterId}`)
}

// Migration recommendations

export async function generateRecommendations(clusterId: string): Promise<MigrationRecommendation[]> {
  return apiPost<MigrationRecommendation[]>(`${API_BASE}/drs/recommendations`, { cluster_id: clusterId })
}

export async function listRecommendations(clusterId: string): Promise<MigrationRecommendation[]> {
  return apiGet<MigrationRecommendation[]>(`${API_BASE}/drs/recommendations/${clusterId}`)
}

export async function approveRecommendation(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/drs/recommendations/${id}/approve`)
}

export async function rejectRecommendation(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/drs/recommendations/${id}/reject`)
}

// Affinity rules

export async function listAffinityRules(clusterId?: string): Promise<AffinityRule[]> {
  const url = clusterId
    ? `${API_BASE}/drs/affinity-rules?cluster_id=${clusterId}`
    : `${API_BASE}/drs/affinity-rules`
  return apiGet<AffinityRule[]>(url)
}

export async function createAffinityRule(req: {
  cluster_id: string
  name: string
  rule_type: 'affinity' | 'anti_affinity'
  mandatory: boolean
  vm_ids: string[]
  host_ids?: string[]
  enabled?: boolean
}): Promise<AffinityRule> {
  return apiPost<AffinityRule>(`${API_BASE}/drs/affinity-rules`, req)
}

export async function updateAffinityRule(id: string, req: Partial<AffinityRule>): Promise<AffinityRule> {
  return apiPut<AffinityRule>(`${API_BASE}/drs/affinity-rules/${id}`, req)
}

export async function deleteAffinityRule(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/drs/affinity-rules/${id}`)
}
