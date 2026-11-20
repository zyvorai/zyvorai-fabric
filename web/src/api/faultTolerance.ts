// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiDelete } from './client'

export interface FtConfig {
  vm_name: string
  primary_host_id: string
  secondary_host_id: string
  status: string
  replication_state?: string
  bandwidth_limit_mbps?: number
  last_sync?: string
  failover_count?: number
  created: string
  updated?: string
}

export interface FtCompatibility {
  vm_name: string
  compatible: boolean
  issues: Array<{
    severity: 'warning' | 'error'
    message: string
    resolution?: string
  }>
  recommended_secondary_hosts: Array<{
    host_id: string
    hostname: string
    score: number
  }>
}

export interface FailoverResult {
  vm_name: string
  failover_type: 'automatic' | 'manual' | 'test'
  source_host_id: string
  target_host_id: string
  status: 'success' | 'failed'
  downtime_ms: number
  started_at: string
  completed_at: string
  error?: string
}

export interface FtMetrics {
  vm_name: string
  log_bandwidth_usage_mbps: number
  checkpoint_latency_ms: number
  secondary_cpu_usage_pct: number
  secondary_memory_usage_pct: number
  failover_count: number
  last_failover?: string
  uptime_pct: number
  collected_at: string
}

const API_BASE = '/api/ft'

export async function enableFt(req: {
  vm_name: string
  primary_host_id: string
  secondary_host_id: string
}): Promise<FtConfig> {
  return apiPost<FtConfig>(`${API_BASE}/enable`, req)
}

export async function disableFt(vmName: string): Promise<void> {
  return apiDelete(`${API_BASE}/vms/${encodeURIComponent(vmName)}`)
}

export async function getFtConfig(vmName: string): Promise<FtConfig> {
  return apiGet<FtConfig>(`${API_BASE}/vms/${encodeURIComponent(vmName)}`)
}

export async function listFtVms(): Promise<FtConfig[]> {
  return apiGet<FtConfig[]>(`${API_BASE}/vms`)
}

export async function checkFtCompatibility(vmName: string): Promise<FtCompatibility> {
  return apiGet<FtCompatibility>(`${API_BASE}/vms/${encodeURIComponent(vmName)}/compatibility`)
}

export async function triggerFailover(vmName: string): Promise<FailoverResult> {
  return apiPost<FailoverResult>(`${API_BASE}/vms/${encodeURIComponent(vmName)}/failover`, {})
}

export async function testFailover(vmName: string): Promise<FailoverResult> {
  return apiPost<FailoverResult>(`${API_BASE}/vms/${encodeURIComponent(vmName)}/test-failover`, {})
}

export async function getFtMetrics(vmName: string): Promise<FtMetrics> {
  return apiGet<FtMetrics>(`${API_BASE}/vms/${encodeURIComponent(vmName)}/metrics`)
}

export async function getFtEvents(vmName?: string): Promise<FailoverResult[]> {
  const url = vmName
    ? `${API_BASE}/events?vm_name=${encodeURIComponent(vmName)}`
    : `${API_BASE}/events`
  return apiGet<FailoverResult[]>(url)
}
