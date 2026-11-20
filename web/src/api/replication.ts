// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiPostVoid, apiDelete } from './client'

export interface ReplicationSite {
  id: string
  name: string
  description?: string
  endpoint: string
  site_type: 'primary' | 'secondary' | 'bidirectional'
  status: string
  replication_count: number
  last_sync?: string
  created: string
  updated?: string
}

export interface ReplicationConfig {
  id: string
  vm_id: string
  vm_name: string
  source_site_id: string
  target_site_id: string
  rpo_minutes: number
  quiesce: boolean
  compression_enabled: boolean
  encryption_enabled: boolean
  bandwidth_limit_mbps?: number
  status: 'active' | 'paused' | 'error' | 'initial_sync' | 'disabled'
  last_sync?: string
  next_sync?: string
  sync_progress_pct?: number
  created: string
  updated?: string
}

export interface ReplicationMetrics {
  replication_id: string
  vm_id: string
  vm_name: string
  avg_sync_time_seconds: number
  last_sync_size_bytes: number
  total_bytes_transferred: number
  sync_count: number
  failure_count: number
  current_rpo_minutes: number
  rpo_target_minutes: number
  rpo_compliant: boolean
  bandwidth_usage_mbps: number
  collected_at: string
}

export interface ReplicationHealthSummary {
  total_replications: number
  active: number
  paused: number
  error: number
  rpo_violations: number
  avg_rpo_minutes: number
  total_bytes_transferred_24h: number
  sites: Array<{
    site_id: string
    site_name: string
    replication_count: number
    health: string
  }>
}

const API_BASE = '/api'

// Sites

export async function listSites(): Promise<ReplicationSite[]> {
  return apiGet<ReplicationSite[]>(`${API_BASE}/replication/sites`)
}

export async function registerSite(req: {
  name: string
  description?: string
  endpoint: string
  site_type: 'primary' | 'secondary' | 'bidirectional'
}): Promise<ReplicationSite> {
  return apiPost<ReplicationSite>(`${API_BASE}/replication/sites`, req)
}

export async function removeSite(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/replication/sites/${id}`)
}

// Replication configurations

export async function listReplications(siteId?: string): Promise<ReplicationConfig[]> {
  const url = siteId
    ? `${API_BASE}/replication/configs?site_id=${siteId}`
    : `${API_BASE}/replication/configs`
  return apiGet<ReplicationConfig[]>(url)
}

export async function configureReplication(req: {
  vm_id: string
  source_site_id: string
  target_site_id: string
  rpo_minutes: number
  quiesce?: boolean
  compression_enabled?: boolean
  encryption_enabled?: boolean
  bandwidth_limit_mbps?: number
}): Promise<ReplicationConfig> {
  return apiPost<ReplicationConfig>(`${API_BASE}/replication/configs`, req)
}

export async function pauseReplication(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/replication/configs/${id}/pause`)
}

export async function resumeReplication(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/replication/configs/${id}/resume`)
}

// Metrics

export async function getReplicationMetrics(replicationId: string): Promise<ReplicationMetrics> {
  return apiGet<ReplicationMetrics>(`${API_BASE}/replication/configs/${replicationId}/metrics`)
}

export async function checkRpoViolations(): Promise<ReplicationMetrics[]> {
  return apiGet<ReplicationMetrics[]>(`${API_BASE}/replication/rpo-violations`)
}

// Health

export async function getReplicationHealth(): Promise<ReplicationHealthSummary> {
  return apiGet<ReplicationHealthSummary>(`${API_BASE}/replication/health`)
}
