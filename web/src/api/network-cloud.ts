// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost, apiPut, apiDelete } from './client'

const API_BASE = '/api'

export interface FloatingIp {
  id: string
  address: string
  interface: string
  assigned_vm?: string | null
  managed?: boolean
  created: string
}

export interface CreateFloatingIpRequest {
  address: string
  interface: string
}

export async function listFloatingIps(): Promise<FloatingIp[]> {
  return apiGet<FloatingIp[]>(`${API_BASE}/floating-ips`)
}

export async function createFloatingIp(req: CreateFloatingIpRequest): Promise<FloatingIp> {
  return apiPost<FloatingIp>(`${API_BASE}/floating-ips`, req)
}

export async function deleteFloatingIp(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/floating-ips/${id}`)
}

export async function adoptFloatingIp(hostId: string): Promise<FloatingIp> {
  return apiPost<FloatingIp>(`${API_BASE}/floating-ips/adopt`, { host_id: hostId })
}

export async function assignFloatingIp(id: string, vmName: string): Promise<FloatingIp> {
  return apiPost<FloatingIp>(`${API_BASE}/floating-ips/${id}/assign`, { vm_name: vmName })
}

export async function unassignFloatingIp(id: string): Promise<FloatingIp> {
  return apiPost<FloatingIp>(`${API_BASE}/floating-ips/${id}/unassign`, {})
}

// ─── Per-bridge DHCP server ──────────────────────────────────────────────────

export interface DhcpServerConfig {
  id: string
  bridge: string
  pool_offset: number
  pool_size: number
  default_lease_time_sec: number
  max_lease_time_sec: number
  dns_servers: string[]
  gateway?: string | null
  domain?: string | null
  enabled: boolean
  created: string
}

export interface CreateDhcpServerRequest {
  bridge: string
  pool_offset?: number
  pool_size?: number
  default_lease_time_sec?: number
  max_lease_time_sec?: number
  dns_servers?: string[]
  gateway?: string
  domain?: string
}

export async function listDhcpServers(): Promise<DhcpServerConfig[]> {
  return apiGet<DhcpServerConfig[]>(`${API_BASE}/dhcp-servers`)
}

export async function createDhcpServer(req: CreateDhcpServerRequest): Promise<DhcpServerConfig> {
  return apiPost<DhcpServerConfig>(`${API_BASE}/dhcp-servers`, req)
}

export async function updateDhcpServer(id: string, req: CreateDhcpServerRequest): Promise<DhcpServerConfig> {
  return apiPut<DhcpServerConfig>(`${API_BASE}/dhcp-servers/${id}`, req)
}

export async function deleteDhcpServer(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/dhcp-servers/${id}`)
}
