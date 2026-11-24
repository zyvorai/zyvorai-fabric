// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost, apiPut, apiDelete } from './client'

export type HygieneStatus = 'ok' | 'warning' | 'critical'

export interface StaleSnapshot {
  id: string
  name: string
  age_hours: number
  size_bytes: number
  protected: boolean
  chain_depth: number
}

export interface VmHygiene {
  vm: string
  status: HygieneStatus
  snapshot_count: number
  total_bytes: number
  oldest_age_hours: number | null
  max_chain_depth: number
  orphaned: string[]
  stale: StaleSnapshot[]
  reclaimable_bytes: number
  findings: string[]
}

export interface HygieneReport {
  generated_at: string
  max_age_hours: number
  max_chain: number
  summary: {
    vms_with_snapshots: number
    ok: number
    warning: number
    critical: number
    stale_snapshots: number
    protected_stale: number
    total_bytes: number
    reclaimable_bytes: number
  }
  vms: VmHygiene[]
}

export type OwnershipStatus = 'expired' | 'expiring' | 'unowned' | 'owned'

export interface OwnershipRow {
  vm: string
  status: OwnershipStatus
  state: string
  owner: string | null
  contact: string | null
  purpose: string | null
  expires_at: string | null
  days_to_expiry: number | null
}

export interface OwnershipReport {
  generated_at: string
  within_days: number
  summary: { vms: number; owned: number; expiring: number; expired: number; unowned: number }
  vms: OwnershipRow[]
}

export interface Ownership {
  vm_name: string
  owner: string | null
  contact: string | null
  purpose: string | null
  notes: string | null
  expires_at: string | null
  updated_at: string | null
  updated_by: string | null
}

export interface OwnershipInput {
  owner?: string
  contact?: string
  purpose?: string
  notes?: string
  expires_at?: string
}

export interface ReclaimCandidate {
  vm: string
  state: string
  reasons: string[]
  idle_basis: 'updated' | 'created'
  idle_days: number
  disk_gb: number
  reclaimable_disk_gb: number
  reclaimable_snapshot_bytes: number
  owner: string | null
  contact: string | null
  lease_expires_at: string | null
}

export interface ReclaimReport {
  generated_at: string
  stopped_days: number
  snapshot_age_hours: number
  summary: {
    candidates: number
    idle_vms: number
    expired_leases: number
    unowned_candidates: number
    reclaimable_disk_gb: number
    reclaimable_snapshot_bytes_candidates: number
    stale_snapshot_bytes_total: number
  }
  candidates: ReclaimCandidate[]
}

function query(params: Record<string, number | undefined>): string {
  const parts = Object.entries(params)
    .filter(([, v]) => v !== undefined)
    .map(([k, v]) => `${k}=${v}`)
  return parts.length ? `?${parts.join('&')}` : ''
}

export function getSnapshotHygiene(maxAgeHours?: number, maxChain?: number): Promise<HygieneReport> {
  return apiGet(`/api/snapshots/hygiene${query({ max_age_hours: maxAgeHours, max_chain: maxChain })}`)
}

export function getOwnershipReport(withinDays?: number): Promise<OwnershipReport> {
  return apiGet(`/api/ownership/report${query({ within_days: withinDays })}`)
}

export function getOwnership(vm: string): Promise<Ownership> {
  return apiGet(`/api/vms/${encodeURIComponent(vm)}/ownership`)
}

export function setOwnership(vm: string, input: OwnershipInput): Promise<Ownership> {
  return apiPut(`/api/vms/${encodeURIComponent(vm)}/ownership`, input)
}

export function clearOwnership(vm: string): Promise<void> {
  return apiDelete(`/api/vms/${encodeURIComponent(vm)}/ownership`)
}

export function extendLease(vm: string, days: number): Promise<Ownership> {
  return apiPost(`/api/vms/${encodeURIComponent(vm)}/ownership/extend`, { days })
}

export function getReclaimReport(stoppedDays?: number, snapshotAgeHours?: number): Promise<ReclaimReport> {
  return apiGet(`/api/reclaim/report${query({ stopped_days: stoppedDays, snapshot_age_hours: snapshotAgeHours })}`)
}
