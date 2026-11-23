// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet } from './client'

export type SnapshotHealthStatus = 'healthy' | 'at_risk' | 'breached' | 'no_snapshots'

export interface VmSnapshotHealth {
  vm: string
  status: SnapshotHealthStatus
  rpo_target_secs: number
  rpo_source: 'policy' | 'default'
  newest_snapshot_at: string | null
  age_secs: number | null
  snapshot_count: number
  total_bytes: number
  orphaned: string[]
  has_policy: boolean
  policy_enabled: boolean
  growth_bytes_per_day: number
  projected_bytes_30d: number
  findings: string[]
}

export interface SnapshotHealthReport {
  generated_at: string
  default_rpo_secs: number
  summary: {
    vms: number
    healthy: number
    at_risk: number
    breached: number
    no_snapshots: number
    without_policy: number
    orphaned_snapshots: number
    total_bytes: number
    projected_bytes_30d: number
  }
  vms: VmSnapshotHealth[]
}

export function getSnapshotHealth(defaultRpoSecs?: number): Promise<SnapshotHealthReport> {
  const q = defaultRpoSecs ? `?default_rpo_secs=${defaultRpoSecs}` : ''
  return apiGet(`/api/snapshots/health${q}`)
}
