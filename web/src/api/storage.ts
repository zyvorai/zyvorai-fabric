// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost, apiPostVoid, apiDelete } from './client'
import { API_BASE_URL } from './config'

export interface NfsConfig {
  server: string
  export_path: string
  mount_path: string
  mount_options: string[]
  auto_start: boolean
  nfs_version: 'V3' | 'V4' | 'V4_1' | 'V4_2'
}

export interface StoragePool {
  id: string
  name: string
  pool_type:
    | 'Local'
    | 'Directory'
    | { NFS: { server: string; export_path: string; mount_options: string[] } }
    | { LVM: { volume_group: string } }
    | { LVMThin: { volume_group: string; thin_pool: string } }
    | { ZFS: { zpool: string; dataset: string | null } }
    | { Ceph: { monitors: string[]; pool_name: string } }
  path: string
  capacity: number
  available: number
  state: 'Inactive' | 'Starting' | 'Active' | 'Stopping' | 'Degraded' | 'Failed'
  auto_start: boolean
  created: string
  updated: string
}

export interface NfsStats {
  total_kb: number
  used_kb: number
  available_kb: number
  use_percent: number
  mount_point: string
}

export interface NfsHealth {
  status: 'Healthy' | 'ServerUnreachable' | 'Unmounted' | 'Degraded'
  server_reachable: boolean
  is_mounted: boolean
  last_check: string
}

export interface CreateNfsPoolRequest {
  name: string
  config: NfsConfig
}

export interface CreateLocalPoolRequest {
  name: string
  path: string
  auto_start: boolean
}

// List all storage pools
export async function listStoragePools(): Promise<StoragePool[]> {
  return apiGet<StoragePool[]>(`${API_BASE_URL}/storage/pools`)
}

// Get storage pool details
export async function getStoragePool(name: string): Promise<StoragePool> {
  return apiGet<StoragePool>(`${API_BASE_URL}/storage/pools/${name}`)
}

// Create local storage pool
export async function createLocalPool(request: CreateLocalPoolRequest): Promise<StoragePool> {
  return apiPost<StoragePool>(`${API_BASE_URL}/storage/pools/local`, request)
}

// Create NFS storage pool
export async function createNfsPool(request: CreateNfsPoolRequest): Promise<StoragePool> {
  return apiPost<StoragePool>(`${API_BASE_URL}/storage/pools/nfs`, request)
}

// Create LVM storage pool
export async function createLvmPool(request: {
  name: string
  volume_group: string
  auto_start: boolean
}): Promise<StoragePool> {
  return apiPost<StoragePool>(`${API_BASE_URL}/storage/pools/lvm`, request)
}

// Create LVM thin storage pool
export async function createLvmThinPool(request: {
  name: string
  volume_group: string
  thin_pool: string
  auto_start: boolean
}): Promise<StoragePool> {
  return apiPost<StoragePool>(`${API_BASE_URL}/storage/pools/lvm-thin`, request)
}

// Create ZFS storage pool
export async function createZfsPool(request: {
  name: string
  zpool: string
  dataset?: string
  auto_start: boolean
}): Promise<StoragePool> {
  return apiPost<StoragePool>(`${API_BASE_URL}/storage/pools/zfs`, request)
}

// Create Ceph storage pool
export async function createCephPool(request: {
  name: string
  monitors: string[]
  pool_name: string
  user?: string
  keyring?: string
  auto_start: boolean
}): Promise<StoragePool> {
  return apiPost<StoragePool>(`${API_BASE_URL}/storage/pools/ceph`, request)
}

// Get Ceph health
export async function getCephHealth(name: string): Promise<{
  status: 'Ok' | 'Warn' | 'Error'
  detail: string
}> {
  return apiGet(`${API_BASE_URL}/storage/pools/${name}/health`)
}

// Get Ceph stats
export async function getCephStats(name: string): Promise<{
  total_bytes: number
  used_bytes: number
  available_bytes: number
  objects: number
}> {
  return apiGet(`${API_BASE_URL}/storage/pools/${name}/stats`)
}

// List RBD images in a Ceph pool
export async function listRbdImages(name: string): Promise<string[]> {
  return apiGet<string[]>(`${API_BASE_URL}/storage/pools/${name}/images`)
}

// Create an RBD image
export async function createRbdImage(poolName: string, request: {
  name: string
  size_mb: number
}): Promise<void> {
  return apiPost(`${API_BASE_URL}/storage/pools/${poolName}/images`, request)
}

// Delete an RBD image
export async function deleteRbdImage(poolName: string, imageName: string): Promise<void> {
  return apiDelete(`${API_BASE_URL}/storage/pools/${poolName}/images/${imageName}`)
}

// Delete storage pool
export async function deleteStoragePool(name: string): Promise<void> {
  return apiDelete(`${API_BASE_URL}/storage/pools/${name}`)
}

// Start storage pool
export async function startStoragePool(name: string): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/storage/pools/${name}/start`)
}

// Stop storage pool
export async function stopStoragePool(name: string): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/storage/pools/${name}/stop`)
}

// Get NFS pool health
export async function getNfsHealth(name: string): Promise<NfsHealth> {
  return apiGet<NfsHealth>(`${API_BASE_URL}/storage/pools/${name}/health`)
}

// Get NFS pool stats
export async function getNfsStats(name: string): Promise<NfsStats> {
  return apiGet<NfsStats>(`${API_BASE_URL}/storage/pools/${name}/stats`)
}

// Refresh pool statistics
export async function refreshPoolStats(name: string): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/storage/pools/${name}/refresh`)
}
