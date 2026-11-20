// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost } from './client'

const API_BASE = '/api'

export type MigrationType = 'live' | 'offline' | 'storage'
export type MigrationState = 'pending' | 'precheck' | 'syncing' | 'switching' | 'completed' | 'failed' | 'cancelled'

export interface MigrationRequest {
  vm_name: string
  target_host: string
  migration_type: MigrationType
  compress?: boolean
  bandwidth_mbps?: number
}

export interface MigrationStatus {
  id: string
  vm_name: string
  target_host: string
  migration_type: MigrationType
  state: MigrationState
  progress_percent: number
  bytes_transferred: number
  started: string
  completed?: string
  error?: string
}

export interface RuntimeMigrationCapability {
  backend: string
  live: boolean
  preCopy: boolean
  postCopy: boolean
  multifd: boolean
  requiresSharedStorage: boolean
  transports: string[]
}

export interface RuntimeSnapshotCapability {
  backend: string
  memory: boolean
  disk: boolean
  portable: boolean
}

export interface RuntimeCapabilities {
  apiVersion: string
  scope: string
  orchestrationOwner: string
  migration: RuntimeMigrationCapability[]
  snapshot: RuntimeSnapshotCapability[]
}

export interface NativeMigrationStartRequest {
  target_uri: string
  mode?: string
  bandwidth_mbps?: number
  max_downtime_ms?: number
  multifd_channels?: number
  shared_storage_confirmed?: boolean
}

export interface NativeMigrationStatus {
  vm_name: string
  phase: string
  status: string
  progress_percent: number
  ram_transferred?: number
  ram_remaining?: number
  ram_total?: number
  total_time_ms?: number
  downtime_ms?: number
  error?: string
}

export async function startMigration(req: MigrationRequest): Promise<MigrationStatus> {
  return apiPost<MigrationStatus>(`${API_BASE}/migrations`, req)
}

export async function listMigrations(): Promise<MigrationStatus[]> {
  return apiGet<MigrationStatus[]>(`${API_BASE}/migrations`)
}

export async function getMigration(id: string): Promise<MigrationStatus> {
  return apiGet<MigrationStatus>(`${API_BASE}/migrations/${id}`)
}

export async function cancelMigration(id: string): Promise<MigrationStatus> {
  return apiPost<MigrationStatus>(`${API_BASE}/migrations/${id}/cancel`)
}

export async function getRuntimeCapabilities(): Promise<RuntimeCapabilities> {
  return apiGet<RuntimeCapabilities>(`${API_BASE}/runtime/capabilities`)
}

export async function startNativeMigration(
  vmName: string,
  req: NativeMigrationStartRequest,
): Promise<NativeMigrationStatus> {
  return apiPost<NativeMigrationStatus>(
    `${API_BASE}/vms/${encodeURIComponent(vmName)}/migration/native/start`,
    req,
  )
}

export async function getNativeMigrationStatus(vmName: string): Promise<NativeMigrationStatus> {
  return apiGet<NativeMigrationStatus>(
    `${API_BASE}/vms/${encodeURIComponent(vmName)}/migration/native/status`,
  )
}

export async function cancelNativeMigration(vmName: string): Promise<NativeMigrationStatus> {
  return apiPost<NativeMigrationStatus>(
    `${API_BASE}/vms/${encodeURIComponent(vmName)}/migration/native/cancel`,
  )
}
