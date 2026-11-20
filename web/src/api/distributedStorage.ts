// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiDelete } from './client'

export interface DiskContribution {
  disk_id: string
  path: string
  capacity_gb: number
  disk_type: string
  status: string
}

export interface StorageHost {
  host_id: string
  hostname: string
  disks: DiskContribution[]
}

export interface FaultDomain {
  id: string
  name: string
  host_ids: string[]
}

export interface DistributedStoragePool {
  id: string
  name: string
  cluster_id: string
  hosts: StorageHost[]
  replication_factor: number
  erasure_coding: boolean
  fault_domains: FaultDomain[]
  total_capacity_gb: number
  used_capacity_gb: number
  free_capacity_gb: number
  status: string
  health: string
  created: string
  updated?: string
}

export interface StoragePolicy {
  id: string
  name: string
  description: string
  replication_factor: number
  disk_type_required?: 'ssd' | 'hdd' | 'nvme'
  encryption_required: boolean
  iops_limit?: number
  throughput_limit_mbps?: number
  tier: 'gold' | 'silver' | 'bronze'
  created: string
  updated: string
}

export interface StorageMigration {
  id: string
  vm_id: string
  vm_name: string
  source_pool_id: string
  source_pool_name: string
  target_pool_id: string
  target_pool_name: string
  policy_id?: string
  status: 'pending' | 'in_progress' | 'completed' | 'failed' | 'cancelled'
  progress_pct: number
  bytes_migrated: number
  bytes_total: number
  started_at?: string
  completed_at?: string
  error?: string
}

export interface DatastoreCluster {
  id: string
  name: string
  cluster_id: string
  datastore_ids: string[]
  storage_drs_enabled: boolean
  space_threshold_pct: number
  io_latency_threshold_ms?: number
  automation_level: 'manual' | 'fully_automated'
  created: string
  updated?: string
}

export interface ComplianceReport {
  vm_name: string
  policy_id: string
  policy_name: string
  compliant: boolean
  violations: string[]
  checked_at: string
}

const API_BASE = '/api'

// Distributed storage pools

export async function listDistributedPools(): Promise<DistributedStoragePool[]> {
  return apiGet<DistributedStoragePool[]>(`${API_BASE}/distributed-storage/pools`)
}

export async function createDistributedPool(req: {
  name: string
  cluster_id: string
  hosts: StorageHost[]
  replication_factor: number
  erasure_coding: boolean
  fault_domains: FaultDomain[]
}): Promise<DistributedStoragePool> {
  return apiPost<DistributedStoragePool>(`${API_BASE}/distributed-storage/pools`, req)
}

export async function deleteDistributedPool(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/distributed-storage/pools/${id}`)
}

// Storage migrations

export async function startStorageMigration(req: {
  vm_id: string
  source_pool_id: string
  target_pool_id: string
  policy_id?: string
}): Promise<StorageMigration> {
  return apiPost<StorageMigration>(`${API_BASE}/distributed-storage/migrations`, req)
}

export async function listStorageMigrations(status?: string): Promise<StorageMigration[]> {
  const url = status
    ? `${API_BASE}/distributed-storage/migrations?status=${status}`
    : `${API_BASE}/distributed-storage/migrations`
  return apiGet<StorageMigration[]>(url)
}

// Storage policies

export async function listStoragePolicies(): Promise<StoragePolicy[]> {
  return apiGet<StoragePolicy[]>(`${API_BASE}/distributed-storage/policies`)
}

export async function createStoragePolicy(req: {
  name: string
  description: string
  replication_factor: number
  disk_type_required?: 'ssd' | 'hdd' | 'nvme'
  encryption_required: boolean
  iops_limit?: number
  throughput_limit_mbps?: number
  tier: 'gold' | 'silver' | 'bronze'
}): Promise<StoragePolicy> {
  const now = new Date().toISOString()
  return apiPost<StoragePolicy>(`${API_BASE}/distributed-storage/policies`, { id: '', ...req, created: now, updated: now })
}

export async function deleteStoragePolicy(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/distributed-storage/policies/${id}`)
}

// Compliance

export async function checkCompliance(policyId: string, vmName: string, poolId: string): Promise<ComplianceReport> {
  return apiPost<ComplianceReport>(`${API_BASE}/distributed-storage/policies/${policyId}/compliance`, { vm_name: vmName, pool_id: poolId })
}

// Datastore clusters

export async function listDatastoreClusters(): Promise<DatastoreCluster[]> {
  return apiGet<DatastoreCluster[]>(`${API_BASE}/distributed-storage/datastore-clusters`)
}

export async function createDatastoreCluster(req: {
  name: string
  cluster_id: string
  datastore_ids: string[]
  storage_drs_enabled: boolean
  space_threshold_pct: number
  io_latency_threshold_ms?: number
  automation_level: 'manual' | 'fully_automated'
}): Promise<DatastoreCluster> {
  return apiPost<DatastoreCluster>(`${API_BASE}/distributed-storage/datastore-clusters`, req)
}
