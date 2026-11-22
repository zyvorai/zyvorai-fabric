// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { API_BASE_URL } from './config'
import { apiPost, apiFetch } from './client'

export interface HotplugCpuRequest {
  count: number
}

export interface HotplugMemoryRequest {
  size_mb: number
}

export interface HotplugDiskRequest {
  path: string
  bus?: string
}

export interface HotplugNicRequest {
  bridge: string
  model?: string
  /** Host NIC for a bridge-less tap. Mutually exclusive with `bridge`. */
  direct_uplink?: string
  direct_mode?: string
  direct_guest_ips?: string[]
  mac?: string
}

export async function hotplugCpu(vmName: string, req: HotplugCpuRequest): Promise<unknown> {
  return apiPost<unknown>(`${API_BASE_URL}/vms/${vmName}/hotplug/cpu`, req)
}

export async function hotplugMemory(vmName: string, req: HotplugMemoryRequest): Promise<unknown> {
  return apiPost<unknown>(`${API_BASE_URL}/vms/${vmName}/hotplug/memory`, req)
}

export async function hotplugDisk(vmName: string, req: HotplugDiskRequest): Promise<unknown> {
  return apiPost<unknown>(`${API_BASE_URL}/vms/${vmName}/hotplug/disk`, req)
}

export async function hotremoveDisk(vmName: string, deviceId: string): Promise<unknown> {
  const res = await apiFetch(`${API_BASE_URL}/vms/${vmName}/hotplug/disk/${deviceId}`, {
    method: 'DELETE',
  })
  if (!res.ok) throw new Error('Failed to hot-remove disk')
  return res.json()
}

export async function hotplugNic(vmName: string, req: HotplugNicRequest): Promise<unknown> {
  return apiPost<unknown>(`${API_BASE_URL}/vms/${vmName}/hotplug/nic`, req)
}

export async function hotremoveNic(vmName: string, deviceId: string): Promise<unknown> {
  const res = await apiFetch(`${API_BASE_URL}/vms/${vmName}/hotplug/nic/${deviceId}`, {
    method: 'DELETE',
  })
  if (!res.ok) throw new Error('Failed to hot-remove NIC')
  return res.json()
}
