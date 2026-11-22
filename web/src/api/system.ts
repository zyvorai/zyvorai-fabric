// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost, apiPostVoid, apiPutVoid, apiDelete } from './client'
import { API_BASE_URL } from './config'

export interface CpuCore {
  id: number
  socket_id: number
  core_id: number
  thread_id: number
  online: boolean
  numa_node: number | null
}

export interface CpuTopology {
  total_cpus: number
  sockets: number
  cores_per_socket: number
  threads_per_core: number
  cpus: CpuCore[]
  online_cpus: number[]
  offline_cpus: number[]
}

export interface NumaNode {
  id: number
  cpus: number[]
  memory_total_mb: number
  memory_free_mb: number
  hugepages_2mb_total: number
  hugepages_2mb_free: number
  hugepages_1gb_total: number
  hugepages_1gb_free: number
}

export interface NumaTopology {
  nodes: NumaNode[]
  distances: number[][]
}

export interface NumaPlacement {
  numa_node: number
  cpu_affinity: number[]
}

export interface MemoryStats {
  current_bytes: number
  max_bytes: number
  swap_current_bytes: number
  swap_max_bytes: number
  limit_bytes: number
  usage_percent: number
}

export interface HugepageStats {
  size: 'Size2MB' | 'Size1GB'
  total: number
  free: number
  reserved: number
  surplus: number
}

export interface SystemMemory {
  total_kb: number
  free_kb: number
  available_kb: number
  buffers_kb: number
  cached_kb: number
}

export interface CpuPinning {
  type: 'Auto' | 'NumaNode' | 'Socket' | 'Explicit'
  value?: number | CpuPin[]
}

export interface CpuPin {
  vcpu_id: number
  physical_cpu: number
}

export interface SetCpuPinningRequest {
  pinning: CpuPinning
}

export interface SetMemoryLimitRequest {
  limit_bytes: number
  swap_limit_bytes?: number
}

export interface AllocateHugepagesRequest {
  size: 'Size2MB' | 'Size1GB'
  count: number
}

// Get CPU topology
export async function getCpuTopology(): Promise<CpuTopology> {
  return apiGet<CpuTopology>(`${API_BASE_URL}/system/cpu/topology`)
}

// Get NUMA topology
export async function getNumaTopology(): Promise<NumaTopology> {
  return apiGet<NumaTopology>(`${API_BASE_URL}/system/numa/topology`)
}

// Get NUMA node details
export async function getNumaNode(nodeId: number): Promise<NumaNode> {
  return apiGet<NumaNode>(`${API_BASE_URL}/system/numa/nodes/${nodeId}`)
}

// Get recommended NUMA placement for a VM
export async function getNumaPlacement(memoryMb: number, cpus: number): Promise<NumaPlacement> {
  return apiGet<NumaPlacement>(
    `${API_BASE_URL}/system/numa/placement?memory_mb=${memoryMb}&cpus=${cpus}`
  )
}

// Set CPU pinning for a VM
export async function setCpuPinning(vmName: string, request: SetCpuPinningRequest): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/cpu/pin`, request)
}

// Remove CPU pinning from a VM
export async function removeCpuPinning(vmName: string): Promise<void> {
  return apiDelete(`${API_BASE_URL}/vms/${vmName}/cpu/pin`)
}

// Get CPU affinity for a VM
export async function getCpuAffinity(vmName: string): Promise<number[]> {
  return apiGet<number[]>(`${API_BASE_URL}/vms/${vmName}/cpu/affinity`)
}

// Set memory limit for a VM
export async function setMemoryLimit(vmName: string, request: SetMemoryLimitRequest): Promise<void> {
  return apiPutVoid(`${API_BASE_URL}/vms/${vmName}/memory/limit`, request)
}

// Get memory usage for a VM
export async function getMemoryUsage(vmName: string): Promise<MemoryStats> {
  return apiGet<MemoryStats>(`${API_BASE_URL}/vms/${vmName}/memory/usage`)
}

// Enable/disable memory ballooning for a VM
export async function setMemoryBallooning(vmName: string, enabled: boolean): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/memory/balloon`, { enabled })
}

// Get hugepage info
export async function getHugepageStats(size: 'Size2MB' | 'Size1GB'): Promise<HugepageStats> {
  return apiGet<HugepageStats>(`${API_BASE_URL}/system/memory/hugepages?size=${size}`)
}

// Allocate hugepages
export async function allocateHugepages(request: AllocateHugepagesRequest): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/system/memory/hugepages`, request)
}

// Get system memory info
export async function getSystemMemory(): Promise<SystemMemory> {
  return apiGet<SystemMemory>(`${API_BASE_URL}/system/memory`)
}

// Optimization types
export interface ResourceRecommendation {
  resource: string
  current_value: string
  recommended_value: string
  reason: string
  impact: string
}

export interface OptimizationRecommendation {
  vm_name: string
  recommendations: ResourceRecommendation[]
}

export interface OptimizationResult {
  vm_name: string
  applied: string[]
  skipped: string[]
}

// Get optimization recommendations
export async function getOptimizationRecommendations(): Promise<OptimizationRecommendation[]> {
  return apiGet<OptimizationRecommendation[]>(`${API_BASE_URL}/system/optimization/recommendations`)
}

// Auto-optimize a VM
export async function optimizeVM(vmName: string): Promise<OptimizationResult> {
  return apiPost<OptimizationResult>(`${API_BASE_URL}/vms/${vmName}/optimize`)
}
