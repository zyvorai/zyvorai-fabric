// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react'
import { Search, Download, Filter, X, AlertCircle, CheckCircle, FileText } from 'lucide-react'
import {
  listAuditLogs,
  AuditLog,
  AuditLogFilters,
  exportAuditLogs,
  getAuditStats,
  AuditStats
} from '../api/audit'
import { useToastContext } from '../contexts/ToastContext'
import { PageHeader, StatusBadge } from '../components/ui'
import ErrorBanner from '../components/ErrorBanner'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import RelativeTime from '../components/RelativeTime'

export default function AuditLogs() {
  const toast = useToastContext()
  const [logs, setLogs] = useState<AuditLog[]>([])
  const [stats, setStats] = useState<AuditStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [showFilters, setShowFilters] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [filters, setFilters] = useState<AuditLogFilters>({})
  const [exporting, setExporting] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)

  useEffect(() => {
    loadData()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filters])

  const loadData = async () => {
    setLoadError(null)
    const [logsRes, statsRes] = await Promise.allSettled([
      listAuditLogs(filters),
      getAuditStats(),
    ])
    if (logsRes.status === 'fulfilled') {
      setLogs(logsRes.value)
    } else {
      const msg = formatUserError(logsRes.reason)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load audit logs', logsRes.reason)
      setLogs([])
    }
    if (statsRes.status === 'fulfilled') {
      setStats(statsRes.value)
    } else {
      setStats(null)
    }
    setLoading(false)
  }

  const handleSearch = () => {
    setFilters({ ...filters, search: searchQuery })
  }

  const handleClearFilters = () => {
    setFilters({})
    setSearchQuery('')
  }

  const handleExport = async (format: 'json' | 'csv') => {
    setExporting(true)
    try {
      const blob = await exportAuditLogs(filters, format)
      const url = window.URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `audit-logs-${new Date().toISOString()}.${format}`
      document.body.appendChild(a)
      a.click()
      window.URL.revokeObjectURL(url)
      document.body.removeChild(a)
      toast.success(`Audit logs exported as ${format.toUpperCase()}`)
    } catch (_error) {
      toast.error('Failed to export audit logs')
    } finally {
      setExporting(false)
    }
  }

  const filteredLogs = logs.filter((log) => {
    if (!searchQuery) return true
    const query = searchQuery.toLowerCase()
    return (
      log.action.toLowerCase().includes(query) ||
      log.user.toLowerCase().includes(query) ||
      log.resource_name.toLowerCase().includes(query) ||
      log.resource_type.toLowerCase().includes(query)
    )
  })

  const getActionColor = (action: string): string => {
    if (action.includes('create')) return 'text-emerald-600'
    if (action.includes('delete')) return 'text-red-600'
    if (action.includes('update') || action.includes('edit')) return 'text-amber-600'
    if (action.includes('start')) return 'text-emerald-600'
    if (action.includes('stop')) return 'text-red-600'
    return 'text-[var(--zf-link)]'
  }

  if (loading) {
    return <div className="text-center py-8">Loading...</div>
  }

  return (
    <div>
      {loadError && (
        <ErrorBanner
          title="Could not load audit logs"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={loadData}
        />
      )}
      <PageHeader
        title="Audit Logs"
        description="Security and compliance audit trail"
        actions={
          <>
            <button
              onClick={() => setShowFilters(!showFilters)}
              className={`flex items-center gap-2 px-4 py-2 rounded-lg transition ${
                showFilters
                  ? 'bg-[var(--zf-link)] text-white'
                  : 'bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
              }`}
            >
              <Filter className="w-4 h-4" />
              Filters
            </button>
            <div className="relative">
              <button
                onClick={() => document.getElementById('export-menu')?.classList.toggle('hidden')}
                disabled={exporting}
                className="flex items-center gap-2 px-4 py-2 zf-panel-muted hover:bg-white/[0.03] transition disabled:opacity-50"
              >
                <Download className="w-4 h-4" />
                Export
              </button>
              <div
                id="export-menu"
                className="hidden absolute right-0 mt-2 w-48 zf-panel-muted z-10"
              >
                <button
                  onClick={() => { handleExport('json'); document.getElementById('export-menu')?.classList.add('hidden') }}
                  className="w-full px-4 py-2 text-left hover:bg-white/[0.03] rounded-t-lg transition"
                >
                  Export as JSON
                </button>
                <button
                  onClick={() => { handleExport('csv'); document.getElementById('export-menu')?.classList.add('hidden') }}
                  className="w-full px-4 py-2 text-left hover:bg-white/[0.03] rounded-b-lg transition"
                >
                  Export as CSV
                </button>
              </div>
            </div>
          </>
        }
      />

      {/* Statistics */}
      {stats && (
        <div className="grid grid-cols-1 md:grid-cols-4 gap-3 mb-4">
          <div className="zf-panel-muted px-4 py-3">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-[var(--zf-muted)]">Total Logs</p>
                <p className="text-2xl font-bold">{stats.total_logs}</p>
              </div>
              <FileText className="w-8 h-8 text-[var(--zf-muted)]" />
            </div>
          </div>
          <div className="zf-panel-muted px-4 py-3">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-[var(--zf-muted)]">Success Rate</p>
                <p className="text-2xl font-bold text-emerald-600">
                  {stats.total_logs > 0 ? ((stats.by_status.success || 0) / stats.total_logs * 100).toFixed(1) : '0.0'}%
                </p>
              </div>
              <CheckCircle className="w-8 h-8 text-[var(--zf-success)]" />
            </div>
          </div>
          <div className="zf-panel-muted px-4 py-3">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-[var(--zf-muted)]">Recent Failures</p>
                <p className="text-2xl font-bold text-red-600">{stats.recent_failures}</p>
              </div>
              <AlertCircle className="w-8 h-8 text-[var(--zf-danger)]" />
            </div>
          </div>
          <div className="zf-panel-muted p-4">
            <div>
              <p className="text-sm text-[var(--zf-muted)] mb-2">Top Actions</p>
              <div className="space-y-1">
                {Object.entries(stats.by_action)
                  .sort((a, b) => b[1] - a[1])
                  .slice(0, 3)
                  .map(([action, count]) => (
                    <div key={action} className="flex justify-between text-xs">
                      <span className="text-[var(--zf-muted)]">{action}</span>
                      <span className="font-medium">{count}</span>
                    </div>
                  ))}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Search Bar */}
      <div className="mb-6">
        <div className="relative">
          <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 w-5 h-5 text-[var(--zf-muted)]" />
          <input
            type="text"
            placeholder="Search logs by action, user, resource..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleSearch()}
            className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg py-3 pl-10 pr-10 text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:border-[var(--zf-link)]"
          />
          {searchQuery && (
            <button
              onClick={() => setSearchQuery('')}
              className="absolute right-3 top-1/2 transform -translate-y-1/2 text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition"
            >
              <X className="w-5 h-5" />
            </button>
          )}
        </div>
      </div>

      {/* Filters */}
      {showFilters && (
        <div className="mb-6 zf-panel-muted p-4">
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div>
              <label className="block text-sm font-medium mb-2">Status</label>
              <select
                value={filters.status || ''}
                onChange={(e) => setFilters({ ...filters, status: e.target.value as any || undefined })}
                className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-link)]"
              >
                <option value="">All</option>
                <option value="success">Success</option>
                <option value="failed">Failed</option>
              </select>
            </div>
            <div>
              <label className="block text-sm font-medium mb-2">Resource Type</label>
              <select
                value={filters.resource_type || ''}
                onChange={(e) => setFilters({ ...filters, resource_type: e.target.value || undefined })}
                className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-link)]"
              >
                <option value="">All</option>
                <option value="vm">VM</option>
                <option value="network">Network</option>
                <option value="storage">Storage</option>
                <option value="template">Template</option>
                <option value="quota">Quota</option>
                <option value="schedule">Schedule</option>
              </select>
            </div>
            <div className="flex items-end">
              <button
                onClick={handleClearFilters}
                className="w-full px-4 py-2 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] rounded-lg transition"
              >
                Clear Filters
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Logs Table */}
      <div className="zf-panel-muted overflow-hidden">
        <div className="overflow-x-auto">
          {filteredLogs.length === 0 ? (
            <div className="text-center py-12">
              <p className="text-xl text-[var(--zf-muted)] mb-2">No audit logs found</p>
              <p className="text-[var(--zf-muted)]">Try adjusting your filters or search query</p>
            </div>
          ) : (
            <table className="w-full">
              <thead className="bg-[var(--zf-surface)]">
                <tr>
                  <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                    Timestamp
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                    User
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                    Action
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                    Resource
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                    Status
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                    IP Address
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {filteredLogs.map((log) => (
                  <tr key={log.id} className="hover:bg-[var(--zf-surface)]">
                    <td className="px-4 py-3 text-sm text-[var(--zf-muted)]">
                      <RelativeTime date={log.timestamp} />
                    </td>
                    <td className="px-4 py-3 text-sm font-medium">{log.user}</td>
                    <td className="px-4 py-3">
                      <span className={`text-sm font-medium ${getActionColor(log.action)}`}>
                        {log.action}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-sm">
                      <div>
                        <span className="text-[var(--zf-muted)] text-xs">{log.resource_type}</span>
                        <p className="font-medium">{log.resource_name}</p>
                      </div>
                    </td>
                    <td className="px-4 py-3">
                      <StatusBadge status={log.status} />
                      {log.error && (
                        <p className="text-xs text-red-600 mt-1">{log.error}</p>
                      )}
                    </td>
                    <td className="px-4 py-3 text-sm text-[var(--zf-muted)]">
                      {log.ip_address || '-'}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>

      <div className="mt-4 text-sm text-[var(--zf-muted)] text-center">
        Showing {filteredLogs.length} of {logs.length} logs
      </div>
    </div>
  )
}
