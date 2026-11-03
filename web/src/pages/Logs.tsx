// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef, useState, useCallback } from 'react'
import { Filter, Download, Trash2 } from 'lucide-react'
import { apiGet } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'
import { AppleTerminalFrame, TERM_LEVEL_COLOR } from '../components/AppleTerminalFrame'
import { AnsiText } from '../components/AnsiText'

interface LogEntry {
  id?: string
  timestamp: string
  level: string
  source: string
  message: string
  action?: string
  resource_type?: string
  detail?: string
}

export default function Logs() {
  const toast = useToastContext()
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [loadError, setLoadError] = useState<string | null>(null)
  const [filter, setFilter] = useState('')
  const [levelFilter, setLevelFilter] = useState<string>('ALL')
  const [autoScroll, setAutoScroll] = useState(true)
  const [loading, setLoading] = useState(true)
  const logContainerRef = useRef<HTMLDivElement>(null)

  const loadLogs = useCallback(async (silent = false) => {
    if (!silent) setLoadError(null)
    try {
      const data = await apiGet<any[]>('/api/audit/logs')
      const entries: LogEntry[] = data.map(entry => ({
        id: entry.id,
        timestamp: entry.timestamp || entry.created || new Date().toISOString(),
        level: (entry.level || entry.severity || 'INFO').toUpperCase(),
        source: entry.source || entry.user || 'Zyvor Fabric',
        message: entry.detail || entry.message || `${entry.action || ''} ${entry.resource_type || ''}`.trim(),
        action: entry.action,
        resource_type: entry.resource_type,
        detail: entry.detail,
      }))
      setLogs(entries)
      setLoadError(null)
    } catch (error) {
      const msg = formatUserError(error)
      if (!silent || logs.length === 0) {
        setLoadError(msg)
        if (!silent) toastFailure(toast, 'Failed to load logs', error)
      }
    } finally {
      if (!silent) setLoading(false)
    }
  }, [toast, logs.length])

  useEffect(() => {
    void loadLogs(false)
    const interval = setInterval(() => void loadLogs(true), 5000)
    return () => clearInterval(interval)
  }, [loadLogs])

  const filteredLogs = logs.filter(log => {
    const matchesText = log.message.toLowerCase().includes(filter.toLowerCase()) ||
                        log.source.toLowerCase().includes(filter.toLowerCase())
    const matchesLevel = levelFilter === 'ALL' || log.level === levelFilter
    return matchesText && matchesLevel
  })

  useEffect(() => {
    if (autoScroll && logContainerRef.current) {
      logContainerRef.current.scrollTop = logContainerRef.current.scrollHeight
    }
  }, [autoScroll, logs])

  const clearLogs = () => {
    setLogs([])
  }

  const exportLogs = () => {
    const content = filteredLogs.map(log =>
      `[${log.timestamp}] ${log.level.padEnd(5)} ${log.source.padEnd(10)} ${log.message}`
    ).join('\n')
    const blob = new Blob([content], { type: 'text/plain' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `zyvor-fabric-logs-${new Date().toISOString()}.txt`
    a.click()
    URL.revokeObjectURL(url)
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title="Logs"
        description="Live audit and system event stream"
        onRefresh={() => void loadLogs()}
        actions={<span className="text-sm text-[var(--zf-muted)]">{filteredLogs.length} entries</span>}
      />
      {loadError && (
        <ErrorBanner
          title="Could not load logs"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={loadLogs}
        />
      )}

      {/* Controls */}
      <div className="bg-[var(--zf-canvas)] rounded-lg p-4 border border-[var(--zf-hairline)]">
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
          {/* Search */}
          <div className="md:col-span-2">
            <div className="relative">
              <Filter className="absolute left-3 top-1/2 transform -translate-y-1/2 w-5 h-5 text-[var(--zf-muted)]" />
              <input
                type="text"
                placeholder="Filter logs..."
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                className="input-field pl-10"
              />
            </div>
          </div>

          {/* Level Filter */}
          <div>
            <select
              aria-label="Log level"
              value={levelFilter}
              onChange={(e) => setLevelFilter(e.target.value)}
              className="input-field"
            >
              <option value="ALL">All Levels</option>
              <option value="INFO">INFO</option>
              <option value="WARN">WARN</option>
              <option value="ERROR">ERROR</option>
              <option value="DEBUG">DEBUG</option>
            </select>
          </div>

          {/* Actions */}
          <div className="flex gap-2">
            <button
              onClick={exportLogs}
              className="zf-btn zf-btn-primary flex-1"
            >
              <Download className="w-4 h-4" />
              Export
            </button>
            <button
              onClick={clearLogs}
              className="zf-btn zf-btn-danger"
              aria-label="Clear logs"
              title="Clear logs"
            >
              <Trash2 className="w-4 h-4" />
            </button>
          </div>
        </div>

        {/* Auto-scroll toggle */}
        <div className="mt-4 flex items-center gap-2">
          <input
            type="checkbox"
            id="autoScroll"
            checked={autoScroll}
            onChange={(e) => setAutoScroll(e.target.checked)}
            className="w-4 h-4 rounded border-[var(--zf-hairline)]"
          />
          <label htmlFor="autoScroll" className="text-sm text-[var(--zf-muted)]">
            Auto-scroll to new logs
          </label>
        </div>
      </div>

      <AppleTerminalFrame
        title="zyvor-fabricd — audit log"
        live={!loading}
        bodyRef={logContainerRef}
        bodyClassName="h-[600px] overflow-y-auto px-3 py-2"
        empty={filteredLogs.length === 0}
        emptyMessage={loading ? 'Loading logs…' : 'No logs to display'}
      >
        {filteredLogs.map((log, index) => (
            <div
              key={log.id || index}
              className="flex items-start gap-3 py-1 hover:bg-white/[0.04] rounded px-1 -mx-1"
            >
              <span className="text-white/30 text-xs whitespace-nowrap tabular-nums shrink-0">
                {log.timestamp.length > 19 ? log.timestamp.slice(0, 19).replace('T', ' ') : log.timestamp}
              </span>
              <span
                className="font-semibold text-xs whitespace-nowrap shrink-0 w-14"
                style={{ color: TERM_LEVEL_COLOR[log.level] || '#f5f5f7' }}
              >
                {log.level}
              </span>
              <span className="text-white/40 text-xs whitespace-nowrap shrink-0 max-w-[8rem] truncate">
                [{log.source}]
              </span>
              <span className="flex-1 min-w-0 break-words whitespace-pre-wrap">
                <AnsiText text={log.message} />
              </span>
            </div>
          ))}
      </AppleTerminalFrame>
    </div>
  )
}
