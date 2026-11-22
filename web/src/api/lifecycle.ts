// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost, apiPostVoid, apiDelete } from './client'

export interface Baseline {
  id: string
  name: string
  description?: string
  baseline_type: 'patch' | 'upgrade' | 'extension'
  release_date: string
  severity: 'critical' | 'important' | 'moderate' | 'low'
  patches: string[]
  host_count: number
  compliant_count: number
  created: string
  updated?: string
}

export interface HostComplianceStatus {
  id: string
  host_id: string
  hostname: string
  baseline_id: string
  baseline_name: string
  status: 'compliant' | 'non_compliant' | 'incompatible' | 'unknown'
  missing_patches: string[]
  installed_patches: string[]
  last_scanned: string
}

export interface RemediationTask {
  id: string
  host_id: string
  hostname: string
  baseline_id: string
  baseline_name: string
  status: 'pending' | 'pre_check' | 'maintenance_mode' | 'remediating' | 'rebooting' | 'completed' | 'failed'
  progress: number
  patches_applied: number
  patches_total: number
  started_at?: string
  completed_at?: string
  error?: string
}

export interface RollingUpdatePlan {
  id: string
  name: string
  description?: string
  baseline_id: string
  host_ids: string[]
  parallel_count: number
  failure_threshold: number
  pre_check_enabled: boolean
  auto_remediate: boolean
  status: 'pending' | 'running' | 'paused' | 'completed' | 'failed'
  completed_hosts: number
  failed_hosts: number
  total_hosts: number
  current_host?: string
  started_at?: string
  completed_at?: string
  error?: string
}

export interface ComplianceSummary {
  total_hosts: number
  compliant: number
  non_compliant: number
  incompatible: number
  unknown: number
  baselines: Array<{
    baseline_id: string
    baseline_name: string
    compliant_hosts: number
    total_hosts: number
  }>
  last_scan?: string
}

const API_BASE = '/api'

// Baselines

export async function listBaselines(): Promise<Baseline[]> {
  return apiGet<Baseline[]>(`${API_BASE}/lifecycle/baselines`)
}

export async function createBaseline(req: {
  name: string
  description?: string
  baseline_type: 'patch' | 'upgrade' | 'extension'
  severity: 'critical' | 'important' | 'moderate' | 'low'
  patches?: string[]
}): Promise<Baseline> {
  return apiPost<Baseline>(`${API_BASE}/lifecycle/baselines`, req)
}

export async function deleteBaseline(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/lifecycle/baselines/${id}`)
}

// Compliance scanning

export async function scanHostCompliance(baselineId: string, hostIds?: string[]): Promise<void> {
  return apiPostVoid(`${API_BASE}/lifecycle/compliance/scan`, { baseline_id: baselineId, host_ids: hostIds })
}

export async function getComplianceStatus(baselineId?: string, hostId?: string): Promise<HostComplianceStatus[]> {
  const params = new URLSearchParams()
  if (baselineId) params.append('baseline_id', baselineId)
  if (hostId) params.append('host_id', hostId)
  const url = `${API_BASE}/lifecycle/compliance${params.toString() ? `?${params}` : ''}`
  return apiGet<HostComplianceStatus[]>(url)
}

// Remediation

export async function listRemediations(): Promise<RemediationTask[]> {
  return apiGet<RemediationTask[]>(`${API_BASE}/lifecycle/remediations`)
}

export async function createRemediation(req: {
  host_id: string
  baseline_id: string
}): Promise<RemediationTask> {
  return apiPost<RemediationTask>(`${API_BASE}/lifecycle/remediations`, req)
}

// Rolling updates

export async function listRollingUpdates(): Promise<RollingUpdatePlan[]> {
  return apiGet<RollingUpdatePlan[]>(`${API_BASE}/lifecycle/rolling-updates`)
}

export async function createRollingUpdate(req: {
  name: string
  description?: string
  baseline_id: string
  host_ids: string[]
  parallel_count?: number
  failure_threshold?: number
  pre_check_enabled?: boolean
  auto_remediate?: boolean
}): Promise<RollingUpdatePlan> {
  return apiPost<RollingUpdatePlan>(`${API_BASE}/lifecycle/rolling-updates`, req)
}

export async function startRollingUpdate(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/lifecycle/rolling-updates/${id}/start`)
}

export async function pauseRollingUpdate(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/lifecycle/rolling-updates/${id}/pause`)
}

export async function advanceRollingUpdate(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/lifecycle/rolling-updates/${id}/advance`)
}
