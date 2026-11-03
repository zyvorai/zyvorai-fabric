// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Clock, Upload, AlertTriangle, Rocket, XCircle, Filter } from 'lucide-react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface TimelineEntry {
  id: string
  timestamp: string
  type: 'action' | 'alert' | 'deploy' | 'error'
  description: string
  source: 'audit' | 'alert'
}

type FilterType = 'all' | 'action' | 'alert' | 'deploy' | 'error'

const typeConfig: Record<TimelineEntry['type'], { icon: typeof Clock; color: string; bg: string; border: string }> = {
  action: { icon: Upload, color: 'text-[var(--zf-link)]', bg: 'bg-[var(--zf-canvas)]', border: 'border-[var(--zf-hairline)]' },
  alert: { icon: AlertTriangle, color: 'text-amber-800', bg: 'bg-amber-50', border: 'border-amber-200' },
  deploy: { icon: Rocket, color: 'text-emerald-700', bg: 'bg-emerald-50', border: 'border-emerald-200' },
  error: { icon: XCircle, color: 'text-red-700', bg: 'bg-red-50', border: 'border-red-200' },
}

function classifyEntry(entry: Record<string, unknown>): TimelineEntry['type'] {
  const status = String(entry.status || '').toLowerCase()
  const action = String(entry.action || entry.type || '').toLowerCase()
  if (status === 'failed' || status === 'error') return 'error'
  if (action.includes('deploy') || action.includes('create')) return 'deploy'
  return 'action'
}

function formatTimestamp(ts: string): string {
  try {
    const d = new Date(ts)
    const now = new Date()
    const diff = now.getTime() - d.getTime()
    if (diff < 60000) return 'Just now'
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`
    return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
  } catch {
    return ts
  }
}

export default function Timeline() {
  const toast = useToastContext()
  const [entries, setEntries] = useState<TimelineEntry[]>([])
  const [filter, setFilter] = useState<FilterType>('all')
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [refreshError, setRefreshError] = useState<string | null>(null)
  const [lastRefresh, setLastRefresh] = useState<Date>(new Date())

  const fetchData = useCallback(async () => {
    try {
      const [auditRes, alertsRes] = await Promise.allSettled([
        apiFetch('/api/audit/logs'),
        apiFetch('/api/system/alerts'),
      ])

      const merged: TimelineEntry[] = []

      if (auditRes.status === 'fulfilled' && auditRes.value.ok) {
        const data = await auditRes.value.json()
        const items = Array.isArray(data) ? data : data.entries || data.logs || []
        items.forEach((item: Record<string, unknown>, idx: number) => {
          merged.push({
            id: `audit-${idx}-${item.id || idx}`,
            timestamp: String(item.timestamp || item.created_at || new Date().toISOString()),
            type: classifyEntry(item),
            description: String(item.description || item.action || item.message || `Action #${idx + 1}`),
            source: 'audit',
          })
        })
      }

      if (alertsRes.status === 'fulfilled' && alertsRes.value.ok) {
        const data = await alertsRes.value.json()
        const items = Array.isArray(data) ? data : data.alerts || []
        items.forEach((item: Record<string, unknown>, idx: number) => {
          const severity = String(item.severity || '').toLowerCase()
          merged.push({
            id: `alert-${idx}-${item.id || idx}`,
            timestamp: String(item.timestamp || item.created_at || new Date().toISOString()),
            type: severity === 'critical' || severity === 'error' ? 'error' : 'alert',
            description: String(item.message || item.description || item.title || `Alert #${idx + 1}`),
            source: 'alert',
          })
        })
      }

      merged.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime())
      setEntries(merged)
      setLoadError(null)
      setRefreshError(null)
      setLastRefresh(new Date())
    } catch (err) {
      const msg = formatUserError(err)
      setEntries((prev) => {
        if (prev.length === 0) {
          setLoadError(msg)
          toastFailure(toast, 'Failed to load timeline', err)
        } else {
          setRefreshError(msg)
        }
        return prev
      })
    } finally {
      setLoading(false)
    }
  }, [toast])

  useEffect(() => {
    fetchData()
    const interval = setInterval(fetchData, 10000)
    return () => clearInterval(interval)
  }, [fetchData])

  const filtered = filter === 'all' ? entries : entries.filter((e) => e.type === filter)

  const filters: { value: FilterType; label: string }[] = [
    { value: 'all', label: 'All' },
    { value: 'action', label: 'Actions' },
    { value: 'alert', label: 'Alerts' },
    { value: 'deploy', label: 'Deploys' },
    { value: 'error', label: 'Errors' },
  ]

  return (
    <div className="max-w-3xl mx-auto space-y-6">
      <PageHeader
        title="Activity Timeline"
        onRefresh={() => void fetchData()}
        refreshing={loading}
        description={`Updated ${formatTimestamp(lastRefresh.toISOString())}`}
      />
      {loadError && (
        <ErrorBanner
          title="Could not load timeline"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={() => void fetchData()}
        />
      )}
      {refreshError && !loadError && (
        <div className="bg-amber-50 rounded-lg border border-amber-200 px-4 py-2 text-xs text-amber-800">
          {refreshError} — showing last known data
        </div>
      )}

      <div className="flex items-center gap-2">
        <Filter className="w-4 h-4 text-[var(--zf-muted)]" />
        {filters.map((f) => (
          <button
            key={f.value}
            onClick={() => setFilter(f.value)}
            className={`px-3 py-1.5 text-xs font-medium rounded-lg transition-colors ${
              filter === f.value
                ? 'bg-[var(--zf-ink)] text-[var(--zf-canvas)] border border-[var(--zf-ink)]'
                : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)] bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] hover:border-[var(--zf-hairline)]'
            }`}
          >
            {f.label}
          </button>
        ))}
      </div>

      {loading && !loadError ? (
        <div className="flex items-center justify-center py-16">
          <div className="w-6 h-6 border-2 border-[var(--zf-ink)] border-t-transparent rounded-full animate-spin" />
        </div>
      ) : filtered.length === 0 ? (
        <div className="text-center py-16 text-[var(--zf-muted)]">
          <Clock className="w-10 h-10 mx-auto mb-3 opacity-50" />
          <p className="text-sm">No activity found{filter !== 'all' ? ` for "${filter}" filter` : ''}.</p>
        </div>
      ) : (
        <div className="relative">
          <div className="absolute left-[19px] top-2 bottom-2 w-0.5 bg-[var(--zf-hairline)]" />

          <div className="space-y-1">
            {filtered.map((entry) => {
              const config = typeConfig[entry.type]
              const Icon = config.icon
              return (
                <div key={entry.id} className="relative flex items-start gap-4 py-3 pl-0 group">
                  <div className={`relative z-10 flex-shrink-0 w-10 h-10 rounded-full ${config.bg} border ${config.border} flex items-center justify-center`}>
                    <Icon className={`w-4 h-4 ${config.color}`} />
                  </div>

                  <div className="flex-1 min-w-0 pt-1">
                    <p className="text-sm text-[var(--zf-ink)] leading-snug">{entry.description}</p>
                    <div className="flex items-center gap-3 mt-1">
                      <span className="text-xs text-[var(--zf-muted)]">{formatTimestamp(entry.timestamp)}</span>
                      <span className={`text-xs px-1.5 py-0.5 rounded ${config.bg} ${config.color} capitalize`}>
                        {entry.type}
                      </span>
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
        </div>
      )}
    </div>
  )
}
