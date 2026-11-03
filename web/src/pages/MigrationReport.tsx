// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { FileText } from 'lucide-react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, EmptyState } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface MigrationEntry {
  id: string
  name: string
  vm_name: string
  status: string
  error?: string
  duration?: string
  output_path?: string
}

interface ReportData {
  total: number
  successful: number
  failed: number
  running: number
  avg_duration: string
  migrations: MigrationEntry[]
  timestamp: string
}

function statusColor(status: string): string {
  switch (status.toLowerCase()) {
    case 'completed':
      return 'text-emerald-700 bg-emerald-50 border-emerald-200'
    case 'failed':
      return 'text-red-700 bg-red-50 border-red-200'
    case 'running':
      return 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
    default:
      return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
  }
}

export default function MigrationReport() {
  const toast = useToastContext()
  const [report, setReport] = useState<ReportData | null>(null)
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  const fetchReport = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const resp = await apiFetch('/api/migrations/report')
      if (!resp.ok) {
        const body = await resp.text()
        throw new Error(formatHttpErrorBody(resp.status, resp.statusText, body))
      }
      setReport(await resp.json())
    } catch (err) {
      const msg = formatUserError(err)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load migration report', err)
      setReport(null)
    } finally {
      setLoading(false)
    }
  }, [toast])

  useEffect(() => {
    fetchReport()
  }, [fetchReport])

  const handleCopy = async () => {
    if (!report) return
    const lines = [
      'Zyvor Fabric Migration Report',
      `Generated: ${new Date(report.timestamp).toLocaleString()}`,
      '',
      `Total: ${report.total}`,
      `Successful: ${report.successful}`,
      `Failed: ${report.failed}`,
      `Running: ${report.running}`,
      `Avg Duration: ${report.avg_duration}`,
      '',
      '--- Migrations ---',
      '',
      ...report.migrations.map(
        (m) =>
          `${m.name} | ${m.status} | ${m.vm_name} | ${m.duration || 'N/A'}${m.error ? `\n  Error: ${m.error}` : ''}`,
      ),
    ]
    await navigator.clipboard.writeText(lines.join('\n')).catch(() => {})
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title="Migration Report"
        description="Summary and details for all migration jobs"
        onRefresh={fetchReport}
        refreshing={loading}
        primaryAction={
          report ? (
            <div className="flex items-center gap-2 print:hidden">
              <button
                type="button"
                onClick={handleCopy}
                title="Copy report to clipboard"
                className="zf-btn zf-btn-ghost zf-btn-sm"
              >
                {copied ? 'Copied!' : 'Copy Report'}
              </button>
              <button
                type="button"
                onClick={() => window.print()}
                title="Print report"
                className="zf-btn zf-btn-ghost zf-btn-sm"
              >
                Print
              </button>
            </div>
          ) : undefined
        }
      />

      {loadError && (
        <ErrorBanner
          title="Could not load migration report"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={fetchReport}
        />
      )}

      {loading && !loadError ? (
        <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] flex flex-col items-center justify-center text-[var(--zf-muted)] gap-3">
          <div className="w-6 h-6 border-2 border-[var(--zf-hairline)] border-t-[var(--zf-link)] rounded-full animate-spin" />
          <span className="text-sm">Generating migration report…</span>
        </div>
      ) : report && !loadError ? (
        <div className="flex flex-col gap-4 print:gap-2">
          <div className="grid grid-cols-2 md:grid-cols-5 gap-3">
            <div className="bg-[var(--zf-canvas)] rounded-xl px-4 py-3 border border-[var(--zf-hairline)]">
              <div className="text-xs text-[var(--zf-muted)] mb-1">Total</div>
              <div className="text-2xl font-bold text-[var(--zf-ink)]">{report.total}</div>
            </div>
            <div className="bg-[var(--zf-canvas)] rounded-xl px-4 py-3 border border-emerald-200">
              <div className="text-xs text-[var(--zf-proof-text)] mb-1">Successful</div>
              <div className="text-2xl font-bold text-[var(--zf-proof-text)]">{report.successful}</div>
            </div>
            <div className="bg-[var(--zf-canvas)] rounded-xl px-4 py-3 border border-red-200">
              <div className="text-xs text-[var(--zf-danger-text)] mb-1">Failed</div>
              <div className="text-2xl font-bold text-[var(--zf-danger-text)]">{report.failed}</div>
            </div>
            <div className="bg-[var(--zf-canvas)] rounded-xl px-4 py-3 border border-[var(--zf-hairline)]">
              <div className="text-xs text-[var(--zf-link)] mb-1">Running</div>
              <div className="text-2xl font-bold text-[var(--zf-link)]">{report.running}</div>
            </div>
            <div className="bg-[var(--zf-canvas)] rounded-xl px-4 py-3 border border-[var(--zf-hairline)]">
              <div className="text-xs text-[var(--zf-muted)] mb-1">Avg Duration</div>
              <div className="text-xl font-bold text-[var(--zf-ink)]">{report.avg_duration || '--'}</div>
            </div>
          </div>

          {report.migrations.length === 0 ? (
            <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)]">
              <EmptyState
                icon={<FileText className="w-12 h-12" />}
                title="No migration data"
                description="Run migrations to populate this report"
              />
            </div>
          ) : (
            <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="bg-black/[0.04] text-xs font-semibold uppercase tracking-wider">
                      <th className="text-left px-4 py-3">Name</th>
                      <th className="text-left px-4 py-3">VM</th>
                      <th className="text-left px-4 py-3">Status</th>
                      <th className="text-left px-4 py-3">Duration</th>
                      <th className="text-left px-4 py-3">Output</th>
                      <th className="text-left px-4 py-3">Error</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-[var(--zf-hairline)]">
                    {report.migrations.map((m, idx) => (
                      <tr key={idx} className="hover:bg-black/[0.04] transition-colors">
                        <td className="px-4 py-3 text-[var(--zf-ink)] font-medium">{m.name || m.id}</td>
                        <td className="px-4 py-3 text-[var(--zf-ink)] font-mono text-xs max-w-[200px] truncate">
                          {m.vm_name || '--'}
                        </td>
                        <td className="px-4 py-3">
                          <span
                            className={`px-2 py-0.5 rounded-full text-xs font-medium border ${statusColor(m.status)}`}
                          >
                            {m.status}
                          </span>
                        </td>
                        <td className="px-4 py-3 text-[var(--zf-muted)] text-xs">{m.duration || '--'}</td>
                        <td className="px-4 py-3 text-[var(--zf-muted)] font-mono text-xs max-w-[200px] truncate">
                          {m.output_path || '--'}
                        </td>
                        <td className="px-4 py-3 text-[var(--zf-danger-text)] text-xs max-w-[200px] truncate">
                          {m.error || '--'}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>
      ) : null}
    </div>
  )
}
