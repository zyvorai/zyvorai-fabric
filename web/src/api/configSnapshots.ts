// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost } from './client'
import type { SnapshotChange } from '../lib/snapshotDiff'

export interface ConfigSnapshotMeta {
  id: string
  created_at: string
  created_by: string
  vms: number
  network_policies: number
  storage_pools: number
}

export interface ConfigSnapshotList {
  snapshots: ConfigSnapshotMeta[]
  max: number
}

export interface ConfigDiff {
  from: string
  to: string
  change_count: number
  changes: SnapshotChange[]
}

export function listConfigSnapshots(): Promise<ConfigSnapshotList> {
  return apiGet<ConfigSnapshotList>('/api/config/snapshots')
}

/** Store the current configuration as a snapshot record (changes nothing else). */
export function createConfigSnapshot(): Promise<ConfigSnapshotMeta> {
  return apiPost<ConfigSnapshotMeta>('/api/config/snapshots')
}

/** `from` / `to` are a snapshot id or the literal `current`. */
export function getConfigDiff(from: string, to: string): Promise<ConfigDiff> {
  const q = new URLSearchParams({ from, to })
  return apiGet<ConfigDiff>(`/api/config/diff?${q}`)
}
