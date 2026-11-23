// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiPost } from './client'

export type BulkAction = 'start' | 'stop' | 'snapshot' | 'tag-add' | 'tag-remove'

export interface BulkRequest {
  selector: { tags?: string[]; labels?: Record<string, string>; names?: string[]; name_prefix?: string }
  action: BulkAction
  tag?: string
  snapshot_prefix?: string
  max_vms?: number
  dry_run: boolean
  plan_hash?: string
  continue_on_error?: boolean
}

export interface BulkPlan {
  action: BulkAction
  items: { vm: string; state: string; will: string; reason?: string }[]
  summary: { matched: number; actionable: number; skipped: number }
  max_vms: number
  over_cap: boolean
  plan_hash: string
}

export interface BulkDryRun {
  dry_run: true
  can_apply: boolean
  plan: BulkPlan
}

export interface BulkApplied {
  dry_run: false
  plan_hash: string
  action: BulkAction
  summary: { ok: number; failed: number; skipped: number; not_attempted: number; vms: number }
  results: { vm: string; status: 'ok' | 'failed' | 'skipped' | 'not_attempted'; error?: string }[]
}

/** Dry run: nothing changes. */
export function planBulk(req: Omit<BulkRequest, 'dry_run' | 'plan_hash'>): Promise<BulkDryRun> {
  return apiPost('/api/vms/bulk', { ...req, dry_run: true })
}

/** Apply requires the hash of a dry run; the daemon recomputes and refuses a stale one. */
export function applyBulk(req: Omit<BulkRequest, 'dry_run'> & { plan_hash: string }): Promise<BulkApplied> {
  return apiPost('/api/vms/bulk', { ...req, dry_run: false })
}

export function bulkHeadline(p: BulkPlan): string {
  const { matched, actionable, skipped } = p.summary
  const cap = p.over_cap ? ` (over the cap of ${p.max_vms}, cannot apply)` : ''
  return `${matched} matched, ${actionable} to ${p.action}, ${skipped} skipped${cap}`
}
