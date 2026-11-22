// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost, apiPut, apiPostVoid, apiDelete } from './client'

export interface Datacenter {
  id: string
  name: string
  description?: string
  clusters: string[]
  status: string
  created: string
  updated?: string
}

export interface Cluster {
  id: string
  name: string
  description?: string
  datacenter_id: string
  hosts: string[]
  ha_enabled: boolean
  drs_enabled: boolean
  drs_mode: string
  evc_mode?: string
  status: string
  created: string
  updated?: string
}

export interface SecureContainersDetail {
  available: boolean
  shim_installed: boolean
  guest_image_present: boolean
}

export interface HostInfo {
  id: string
  hostname: string
  address: string
  cluster_id: string
  datacenter_id: string
  cpus: number
  memory_mb: number
  status: string
  last_heartbeat?: string
  vm_count: number
  cpu_usage_pct: number
  memory_usage_pct: number
  agent_version?: string
  created: string
  updated?: string
  secure_containers_ready?: boolean
  secure_containers?: SecureContainersDetail | null
}

export interface DatacenterSummary {
  id: string
  name: string
  cluster_count: number
  host_count: number
  vm_count: number
  total_cpus: number
  total_memory_mb: number
}

const API_BASE = '/api'

// Datacenter CRUD

export async function listDatacenters(): Promise<Datacenter[]> {
  return apiGet<Datacenter[]>(`${API_BASE}/datacenters`)
}

export async function createDatacenter(req: { name: string; description?: string }): Promise<Datacenter> {
  return apiPost<Datacenter>(`${API_BASE}/datacenters`, req)
}

export async function getDatacenter(id: string): Promise<Datacenter> {
  return apiGet<Datacenter>(`${API_BASE}/datacenters/${id}`)
}

export async function updateDatacenter(id: string, req: Partial<Datacenter>): Promise<Datacenter> {
  return apiPut<Datacenter>(`${API_BASE}/datacenters/${id}`, req)
}

export async function deleteDatacenter(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/datacenters/${id}`)
}

export async function getDatacenterSummary(id: string): Promise<DatacenterSummary> {
  return apiGet<DatacenterSummary>(`${API_BASE}/datacenters/${id}/summary`)
}

// Cluster CRUD

export async function listClusters(datacenterId?: string): Promise<Cluster[]> {
  const url = datacenterId
    ? `${API_BASE}/clusters?datacenter_id=${datacenterId}`
    : `${API_BASE}/clusters`
  return apiGet<Cluster[]>(url)
}

export async function createCluster(req: { name: string; datacenter_id: string; description?: string }): Promise<Cluster> {
  return apiPost<Cluster>(`${API_BASE}/clusters`, req)
}

export async function getCluster(id: string): Promise<Cluster> {
  return apiGet<Cluster>(`${API_BASE}/clusters/${id}`)
}

export async function updateCluster(id: string, req: Partial<Cluster>): Promise<Cluster> {
  return apiPut<Cluster>(`${API_BASE}/clusters/${id}`, req)
}

export async function deleteCluster(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/clusters/${id}`)
}

// Host management

export async function listHosts(clusterId?: string): Promise<HostInfo[]> {
  const url = clusterId
    ? `${API_BASE}/hosts?cluster_id=${clusterId}`
    : `${API_BASE}/hosts`
  return apiGet<HostInfo[]>(url)
}

export async function registerHost(req: {
  hostname: string
  address: string
  cluster_id: string
  cpus: number
  memory_mb: number
}): Promise<HostInfo> {
  return apiPost<HostInfo>(`${API_BASE}/hosts`, req)
}

export async function getHost(id: string): Promise<HostInfo> {
  return apiGet<HostInfo>(`${API_BASE}/hosts/${id}`)
}

export async function removeHost(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/hosts/${id}`)
}

export async function hostEnterMaintenance(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/hosts/${id}/maintenance/enter`)
}

export async function hostExitMaintenance(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/hosts/${id}/maintenance/exit`)
}
