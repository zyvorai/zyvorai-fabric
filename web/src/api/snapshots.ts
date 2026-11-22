// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost, apiPostVoid, apiDelete, apiFetch } from './client'
import { API_BASE_URL } from './config'
import { formatHttpErrorBody } from '../utils/apiError'
import { parseJsonResponse } from '../utils/parseJsonResponse'

export interface VMSnapshot {
  id: string
  vm_name: string
  name: string
  description: string | null
  snapshot_type: 'Disk' | 'Full'
  parent_id: string | null
  size_bytes: number
  created: string
}

export interface SnapshotTreeNode {
  snapshot: VMSnapshot
  children: SnapshotTreeNode[]
}

export interface CreateSnapshotRequest {
  name: string
  description?: string
  snapshot_type?: 'Disk' | 'Full'
}

export async function listSnapshots(vmName: string): Promise<VMSnapshot[]> {
  return apiGet<VMSnapshot[]>(`${API_BASE_URL}/vms/${vmName}/snapshots`)
}

export async function createSnapshot(vmName: string, req: CreateSnapshotRequest): Promise<VMSnapshot> {
  return apiPost<VMSnapshot>(`${API_BASE_URL}/vms/${vmName}/snapshots`, req)
}

/**
 * Create a snapshot, retrying briefly on 409 (QMP not ready yet after start).
 * Full snapshots can take minutes — callers should show progress messaging.
 */
export async function createSnapshotWithRetry(
  vmName: string,
  req: CreateSnapshotRequest,
  opts?: { retries?: number; delayMs?: number; onRetry?: (attempt: number, message: string) => void },
): Promise<VMSnapshot> {
  const retries = opts?.retries ?? 5
  const delayMs = opts?.delayMs ?? 2000
  let lastErr: Error | null = null

  for (let attempt = 0; attempt <= retries; attempt++) {
    const res = await apiFetch(`${API_BASE_URL}/vms/${vmName}/snapshots`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    })
    if (res.ok) {
      return parseJsonResponse<VMSnapshot>(res)
    }
    const body = await res.text().catch(() => '')
    const msg = formatHttpErrorBody(res.status, res.statusText, body)
    lastErr = new Error(msg)
    const retryable =
      res.status === 409 ||
      /could not reach the VM monitor|QMP control socket isn't available/i.test(msg)
    if (!retryable || attempt === retries) {
      throw lastErr
    }
    opts?.onRetry?.(attempt + 1, msg)
    await new Promise((r) => setTimeout(r, delayMs))
  }
  throw lastErr ?? new Error('Failed to create snapshot')
}

export async function getSnapshot(vmName: string, id: string): Promise<VMSnapshot> {
  return apiGet<VMSnapshot>(`${API_BASE_URL}/vms/${vmName}/snapshots/${id}`)
}

export async function deleteSnapshot(vmName: string, id: string): Promise<void> {
  return apiDelete(`${API_BASE_URL}/vms/${vmName}/snapshots/${id}`)
}

export async function revertSnapshot(vmName: string, id: string): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/snapshots/${id}/revert`)
}

export async function getSnapshotTree(vmName: string): Promise<SnapshotTreeNode[]> {
  return apiGet<SnapshotTreeNode[]>(`${API_BASE_URL}/vms/${vmName}/snapshots/tree`)
}
