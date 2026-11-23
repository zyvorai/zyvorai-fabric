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

// --- Export and integrity (GET /api/audit/export, GET /api/audit/verify) ---

export interface AuditExportParams {
  from?: string
  to?: string
  actor?: string
  action?: string
}

export type AuditExportFormat = 'ndjson' | 'csv'

export interface AuditChainBreak {
  reason: string
  seq?: number | null
  id?: string | null
  detail: string
}

export interface AuditVerifyReport {
  status: 'ok' | 'broken' | 'not_chained'
  ok: boolean
  chain_enabled: boolean
  total: number
  chained: number
  legacy_unchained: number
  malformed: string[]
  head?: { seq: number; hash: string } | null
  anchor?: { seq: number; hash: string } | null
  first_break?: AuditChainBreak | null
  checked_at: string
}

export function auditExportQuery(params: AuditExportParams, format: AuditExportFormat): string {
  const q = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    const t = typeof v === 'string' ? v.trim() : ''
    if (t) q.append(k, t)
  }
  q.append('format', format)
  return q.toString()
}

/** Download matching events as NDJSON or CSV. Rejects with the daemon's message (e.g. a 413 "narrow the filters"). */
export async function exportAuditEvents(params: AuditExportParams, format: AuditExportFormat): Promise<Blob> {
  const res = await apiFetch(`${API_BASE}/audit/export?${auditExportQuery(params, format)}`)
  if (!res.ok) {
    let msg = `Export failed (${res.status})`
    try {
      const body = await res.json()
      if (body && typeof body.error === 'string') msg = body.error
    } catch {
      /* keep the generic message */
    }
    throw new Error(msg)
  }
  return res.blob()
}

export async function verifyAuditChain(): Promise<AuditVerifyReport> {
  return apiGet<AuditVerifyReport>(`${API_BASE}/audit/verify`)
}
