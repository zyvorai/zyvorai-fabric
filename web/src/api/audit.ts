// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiFetch } from './client'

export interface AuditLog {
  id: string
  timestamp: string
  user: string
  action: string
  resource_type: string
  resource_name: string
  status: 'success' | 'failed'
  ip_address?: string
  details?: string
  error?: string
}

export interface AuditLogFilters {
  action?: string
  user?: string
  resource_type?: string
  resource_name?: string
  status?: 'success' | 'failed'
  start_time?: string
  end_time?: string
  search?: string
}

const API_BASE = '/api'

export async function listAuditLogs(filters?: AuditLogFilters): Promise<AuditLog[]> {
  const params = new URLSearchParams()
  if (filters) {
    Object.entries(filters).forEach(([key, value]) => {
      if (value) params.append(key, value.toString())
    })
  }

  const url = `${API_BASE}/audit/logs${params.toString() ? `?${params}` : ''}`
  return apiGet<AuditLog[]>(url)
}

export async function getAuditLog(id: string): Promise<AuditLog> {
  return apiGet<AuditLog>(`${API_BASE}/audit/logs/${id}`)
}

export async function exportAuditLogs(filters?: AuditLogFilters, format: 'json' | 'csv' = 'json'): Promise<Blob> {
  const params = new URLSearchParams()
  if (filters) {
    Object.entries(filters).forEach(([key, value]) => {
      if (value) params.append(key, value.toString())
    })
  }
  params.append('format', format)

  const url = `${API_BASE}/audit/logs/export?${params}`
  const res = await apiFetch(url)
  if (!res.ok) throw new Error('Failed to export audit logs')
  return res.blob()
}

export interface AuditStats {
  total_logs: number
  by_action: Record<string, number>
  by_user: Record<string, number>
  by_status: Record<string, number>
  recent_failures: number
}

export async function getAuditStats(): Promise<AuditStats> {
  return apiGet<AuditStats>(`${API_BASE}/audit/stats`)
}
