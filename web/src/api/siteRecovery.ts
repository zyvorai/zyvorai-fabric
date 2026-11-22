// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost, apiPut, apiPostVoid, apiDelete } from './client'

export interface RecoveryPlan {
  id: string
  name: string
  description?: string
  source_site_id: string
  target_site_id: string
  vm_groups: Array<{
    name: string
    vm_ids: string[]
    boot_order: number
    boot_delay_seconds: number
    pre_power_on_script?: string
    post_power_on_script?: string
  }>
  network_mappings: Array<{
    source_network: string
    target_network: string
  }>
  storage_mappings: Array<{
    source_datastore: string
    target_datastore: string
  }>
  test_network?: string
  status: string
  last_tested?: string
  last_executed?: string
  created: string
  updated?: string
}

export interface RecoveryExecution {
  id: string
  plan_id: string
  plan_name: string
  execution_type: 'planned_migration' | 'disaster_recovery' | 'test_failover'
  status: 'running' | 'completed' | 'failed' | 'cancelled' | 'rolling_back'
  progress_pct: number
  current_step: string
  steps: Array<{
    name: string
    status: 'pending' | 'running' | 'completed' | 'failed' | 'skipped'
    started_at?: string
    completed_at?: string
    error?: string
  }>
  vms_recovered: number
  vms_total: number
  rto_actual_seconds?: number
  started_at: string
  completed_at?: string
  error?: string
}

export interface DrDashboard {
  total_plans: number
  plans_tested: number
  plans_untested: number
  last_test_date?: string
  active_executions: number
  total_protected_vms: number
  total_unprotected_vms: number
  avg_rto_seconds: number
  avg_rpo_minutes: number
  sites: Array<{
    site_id: string
    site_name: string
    status: string
    protected_vms: number
    plans: number
  }>
  recent_executions: RecoveryExecution[]
}

const API_BASE = '/api'

// Recovery plans

export async function listPlans(): Promise<RecoveryPlan[]> {
  return apiGet<RecoveryPlan[]>(`${API_BASE}/site-recovery/plans`)
}

export async function createPlan(req: {
  name: string
  description?: string
  source_site_id: string
  target_site_id: string
  vm_groups: Array<{
    name: string
    vm_ids: string[]
    boot_order: number
    boot_delay_seconds?: number
    pre_power_on_script?: string
    post_power_on_script?: string
  }>
  network_mappings?: Array<{
    source_network: string
    target_network: string
  }>
  storage_mappings?: Array<{
    source_datastore: string
    target_datastore: string
  }>
  test_network?: string
}): Promise<RecoveryPlan> {
  return apiPost<RecoveryPlan>(`${API_BASE}/site-recovery/plans`, req)
}

export async function getPlan(id: string): Promise<RecoveryPlan> {
  return apiGet<RecoveryPlan>(`${API_BASE}/site-recovery/plans/${id}`)
}

export async function updatePlan(id: string, req: Partial<RecoveryPlan>): Promise<RecoveryPlan> {
  return apiPut<RecoveryPlan>(`${API_BASE}/site-recovery/plans/${id}`, req)
}

export async function deletePlan(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/site-recovery/plans/${id}`)
}

// Plan executions

export async function executePlannedMigration(planId: string): Promise<RecoveryExecution> {
  return apiPost<RecoveryExecution>(`${API_BASE}/site-recovery/plans/${planId}/planned-migration`)
}

export async function executeDisasterRecovery(planId: string): Promise<RecoveryExecution> {
  return apiPost<RecoveryExecution>(`${API_BASE}/site-recovery/plans/${planId}/disaster-recovery`)
}

export async function executeTestFailover(planId: string): Promise<RecoveryExecution> {
  return apiPost<RecoveryExecution>(`${API_BASE}/site-recovery/plans/${planId}/test-failover`)
}

export async function listExecutions(planId?: string): Promise<RecoveryExecution[]> {
  const url = planId
    ? `${API_BASE}/site-recovery/executions?plan_id=${planId}`
    : `${API_BASE}/site-recovery/executions`
  return apiGet<RecoveryExecution[]>(url)
}

export async function getExecution(id: string): Promise<RecoveryExecution> {
  return apiGet<RecoveryExecution>(`${API_BASE}/site-recovery/executions/${id}`)
}

export async function cancelExecution(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/site-recovery/executions/${id}/cancel`)
}

// Dashboard

export async function getDrDashboard(): Promise<DrDashboard> {
  return apiGet<DrDashboard>(`${API_BASE}/site-recovery/dashboard`)
}
