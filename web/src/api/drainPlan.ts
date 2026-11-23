// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiPost } from './client'

export type DrainActionKind = 'migrate' | 'relocate' | 'stop' | 'blocked'

export interface DrainAction {
  order: number
  vm: string
  action: DrainActionKind
  target: string | null
  reasons: string[]
  warnings: string[]
}

export interface DrainTarget {
  host_id: string
  vcpus_used_before: number
  vcpus_used_after: number
  vcpus_capacity: number
  memory_mb_used_before: number
  memory_mb_used_after: number
  memory_mb_capacity: number
}

export interface DrainPlan {
  host_id: string
  dry_run: boolean
  can_drain: boolean
  actions: DrainAction[]
  summary: { migrate: number; relocate: number; stop: number; blocked: number }
  targets: DrainTarget[]
  warnings: string[]
  plan_hash: string
}

/** Dry-run only: the daemon never moves or stops anything for this call. */
export function planHostDrain(hostId: string): Promise<DrainPlan> {
  return apiPost(`/api/hosts/${encodeURIComponent(hostId)}/drain-plan`)
}
