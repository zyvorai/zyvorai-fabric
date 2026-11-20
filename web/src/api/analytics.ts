// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiFetch } from './client'

export interface PerformanceMetrics {
  timestamp: string
  cpu_usage: number
  memory_usage: number
  disk_io_read: number
  disk_io_write: number
  network_rx: number
  network_tx: number
}

export interface VMPerformance {
  vm_name: string
  metrics: PerformanceMetrics[]
}

export interface SystemPerformance {
  timestamp: string
  total_vms: number
  running_vms: number
  total_cpu_usage: number
  total_memory_usage: number
  total_network_rx: number
  total_network_tx: number
}

export interface PerformanceInsight {
  type: 'high_cpu' | 'high_memory' | 'high_disk_io' | 'high_network' | 'underutilized'
  vm_name: string
  resource: string
  value: number
  threshold: number
  severity: 'info' | 'warning' | 'critical'
  recommendation: string
}

export type TimeRange = '1h' | '6h' | '24h' | '7d' | '30d'

const API_BASE = '/api'

export async function getVMPerformance(
  vmName: string,
  timeRange: TimeRange = '24h'
): Promise<VMPerformance> {
  return apiGet<VMPerformance>(`${API_BASE}/analytics/vms/${vmName}?range=${timeRange}`)
}

export async function getSystemPerformance(
  timeRange: TimeRange = '24h'
): Promise<SystemPerformance[]> {
  return apiGet<SystemPerformance[]>(`${API_BASE}/analytics/system?range=${timeRange}`)
}

export async function getPerformanceInsights(): Promise<PerformanceInsight[]> {
  return apiGet<PerformanceInsight[]>(`${API_BASE}/analytics/insights`)
}

export async function getTopVMsByResource(
  resource: 'cpu' | 'memory' | 'network' | 'disk',
  limit: number = 10
): Promise<Array<{ vm_name: string; value: number }>> {
  return apiGet<Array<{ vm_name: string; value: number }>>(`${API_BASE}/analytics/top?resource=${resource}&limit=${limit}`)
}

export async function exportPerformanceReport(
  timeRange: TimeRange,
  format: 'pdf' | 'csv' = 'pdf'
): Promise<Blob> {
  const res = await apiFetch(`${API_BASE}/analytics/export?range=${timeRange}&format=${format}`)
  if (!res.ok) throw new Error('Failed to export performance report')
  return res.blob()
}

export interface ResourceUtilization {
  cpu_utilization: number
  memory_utilization: number
  disk_utilization: number
  network_utilization: number
}

export async function getResourceUtilization(): Promise<ResourceUtilization> {
  return apiGet<ResourceUtilization>(`${API_BASE}/analytics/utilization`)
}
