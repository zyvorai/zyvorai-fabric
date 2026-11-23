// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiPost, apiPut, apiDelete, apiFetch } from './client'
import { API_BASE_URL } from './config'
import { parseJsonResponse } from '../utils/parseJsonResponse'
import { formatHttpErrorBody } from '../utils/apiError'

export interface SnapshotPolicyInput {
  enabled: boolean
  interval_secs?: number | null
  cron?: string | null
  keep_last?: number | null
  max_age_secs?: number | null
  name_prefix: string
  snapshot_type: 'Disk' | 'Full'
}

export interface SnapshotPolicy extends SnapshotPolicyInput {
  vm_name: string
  updated_at: string
  last_run?: string | null
  last_result?: string | null
  /** Server-computed; null when the policy is disabled. */
  next_run?: string | null
}

export interface PruneDecision {
  id: string
  name: string
  created: string
  action: 'keep' | 'delete'
  reason: string
}

export interface SnapshotPolicyRunReport {
  vm_name: string
  dry_run: boolean
  created: string | null
  decisions: PruneDecision[]
  deleted: string[]
  delete_failed: string[]
  errors: string[]
}

const base = (vm: string) => `${API_BASE_URL}/vms/${encodeURIComponent(vm)}/snapshot-policy`

/** The VM's policy, or null when none is set (404). */
export async function getSnapshotPolicy(vm: string): Promise<SnapshotPolicy | null> {
  const res = await apiFetch(base(vm))
  if (res.status === 404) return null
  if (!res.ok) {
    const body = await res.text().catch(() => '')
    throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
  }
  return parseJsonResponse<SnapshotPolicy>(res)
}

export function putSnapshotPolicy(vm: string, input: SnapshotPolicyInput): Promise<SnapshotPolicy> {
  return apiPut<SnapshotPolicy>(base(vm), input)
}

export function deleteSnapshotPolicy(vm: string): Promise<void> {
  return apiDelete(base(vm))
}

/** Dry-run preview: creates and deletes nothing. */
export function previewSnapshotPolicy(vm: string): Promise<SnapshotPolicyRunReport> {
  return apiPost<SnapshotPolicyRunReport>(`${base(vm)}/run?dry_run=true`)
}
