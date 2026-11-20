// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { useState, useEffect, useCallback } from 'react'
import { History } from 'lucide-react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, EmptyState, Card, StatusBadge } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface HistoryEntry {
  id: string
  name: string
  vm_name: string
  status: string
  error?: string
  started_at?: string
  completed_at?: string
  duration?: string
  output_path?: string
}

function formatTime(dateStr?: string): string {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

export default function MigrationHistory() {
  const toast = useToastContext()
  const [history, setHistory] = useState<HistoryEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)

  const fetchHistory = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const resp = await apiFetch('/api/migrations/history')
      if (!resp.ok) {
        const body = await resp.text()
        throw new Error(formatHttpErrorBody(resp.status, resp.statusText, body))
      }
      const data = await resp.json()
      setHistory(data.history || data.migrations || [])
    } catch (err) {
      const msg = formatUserError(err)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load migration history', err)
    } finally {
      setLoading(false)
    }
  }, [toast])

  useEffect(() => {
    fetchHistory()
  }, [fetchHistory])

  return (
    <div className="space-y-6">
      <PageHeader
        title="Migration History"
        description="Completed and failed migration jobs"
        onRefresh={fetchHistory}
        refreshing={loading}
      />

      {loadError && (
        <ErrorBanner
          title="Could not load migration history"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={fetchHistory}
        />
      )}

      {loading && !loadError ? (
        <Card>
          <div className="p-10 flex flex-col items-center justify-center text-[var(--zf-muted)] gap-3">
            <div className="w-6 h-6 border-2 border-[var(--zf-hairline)] border-t-[var(--zf-ink)] rounded-full animate-spin" />
            <span className="text-sm">Loading migration history…</span>
          </div>
        </Card>
      ) : !loadError && history.length === 0 ? (
        <Card>
          <EmptyState
            icon={<History className="w-12 h-12" />}
            title="No migration history yet"
            description="Completed and failed migrations will appear here"
          />
        </Card>
      ) : !loadError ? (
        <Card className="overflow-hidden">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-[var(--zf-hairline)]">
                <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Name</th>
                <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">VM</th>
                <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Status</th>
                <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Started</th>
                <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Duration</th>
                <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Output</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]/30">
              {history.map((entry, idx) => (
                <tr key={`${entry.id}-${idx}`} className="hover:bg-black/[0.04] transition-colors">
                  <td className="p-3">
                    <div className="font-medium text-[var(--zf-ink)]">{entry.name || entry.id}</div>
                  </td>
                  <td className="p-3 text-[var(--zf-muted)] text-xs truncate max-w-xs">{entry.vm_name || '-'}</td>
                  <td className="p-3">
                    <StatusBadge status={entry.status} />
                    {entry.error && (
                      <div className="text-xs text-[var(--zf-danger)] mt-1 truncate max-w-xs" title={entry.error}>
                        {entry.error}
                      </div>
                    )}
                  </td>
                  <td className="p-3 text-[var(--zf-muted)] whitespace-nowrap text-xs">{formatTime(entry.started_at)}</td>
                  <td className="p-3 text-[var(--zf-ink)] whitespace-nowrap text-xs">{entry.duration || '-'}</td>
                  <td className="p-3 text-[var(--zf-muted)] text-xs truncate max-w-xs">{entry.output_path || '-'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      ) : null}
    </div>
  )
}
