// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface ReadinessCheck {
  name: string
  status: string
  message: string
  detail?: string
}

function StatusIcon({ status }: { status: string }) {
  if (status === 'ok') {
    return (
      <span className="flex items-center justify-center w-6 h-6 rounded-full bg-emerald-50 text-emerald-700 text-xs">
        &#10003;
      </span>
    )
  }
  if (status === 'warning') {
    return (
      <span className="flex items-center justify-center w-6 h-6 rounded-full bg-amber-50 text-amber-800 text-xs">
        &#9888;
      </span>
    )
  }
  return (
    <span className="flex items-center justify-center w-6 h-6 rounded-full bg-red-50 text-red-700 text-xs">
      &#10007;
    </span>
  )
}

export default function MigrationReadiness() {
  const toast = useToastContext()
  const [checks, setChecks] = useState<ReadinessCheck[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)

  const fetchReadiness = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const resp = await apiFetch('/api/migrations/readiness')
      if (!resp.ok) {
        const body = await resp.text()
        throw new Error(formatHttpErrorBody(resp.status, resp.statusText, body))
      }
      const data = await resp.json()
      setChecks(data.checks || [])
    } catch (err) {
      const msg = formatUserError(err)
      setLoadError(msg)
      toastFailure(toast, 'Failed to check migration readiness', err)
    } finally {
      setLoading(false)
    }
  }, [toast])

  useEffect(() => {
    fetchReadiness()
  }, [fetchReadiness])

  const hasIssues = checks.some((c) => c.status !== 'ok')
  const errorCount = checks.filter((c) => c.status === 'error').length
  const warningCount = checks.filter((c) => c.status === 'warning').length

  return (
    <div className="space-y-6">
      <PageHeader
        title="Migration Readiness"
        description="Pre-flight checks before starting a migration"
        onRefresh={fetchReadiness}
        refreshing={loading}
      />

      {loadError && (
        <ErrorBanner
          title="Could not run readiness checks"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={fetchReadiness}
        />
      )}

      {loading && !loadError ? (
        <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] flex flex-col items-center justify-center text-[var(--zf-muted)] gap-3">
          <div className="w-6 h-6 border-2 border-[var(--zf-hairline)] border-t-[var(--zf-link)] rounded-full animate-spin" />
          <span className="text-sm">Checking migration readiness…</span>
        </div>
      ) : !loadError ? (
        <div className="flex flex-col gap-4">
          <div
            className={`rounded-xl p-4 border flex items-center justify-between ${
              hasIssues ? 'bg-amber-50 border-amber-200' : 'bg-emerald-50 border-emerald-200'
            }`}
          >
            <div className="flex items-center gap-3">
              <div
                className={`w-8 h-8 rounded-full flex items-center justify-center ${
                  hasIssues ? 'bg-amber-100 text-amber-800' : 'bg-emerald-100 text-emerald-700'
                }`}
              >
                {hasIssues ? '\u26A0' : '\u2713'}
              </div>
              <div>
                {/* the banner background is a fixed pale tint in both themes, so its text is a fixed dark colour too */}
                <div className="text-sm font-semibold text-slate-900">
                  {hasIssues ? 'Issues Found' : 'Ready for Migration'}
                </div>
                <div className="text-xs text-slate-700">
                  {hasIssues
                    ? `${errorCount} error${errorCount !== 1 ? 's' : ''}, ${warningCount} warning${warningCount !== 1 ? 's' : ''}`
                    : `All ${checks.length} checks passed`}
                </div>
              </div>
            </div>
          </div>

          <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] divide-y divide-[var(--zf-hairline)]">
            {checks.map((check, idx) => (
              <div key={idx} className="flex items-start gap-3 p-4">
                <StatusIcon status={check.status} />
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium text-[var(--zf-ink)]">{check.name}</div>
                  <div className="text-xs text-[var(--zf-muted)] mt-0.5">{check.message}</div>
                  {check.detail && (
                    <div className="text-xs text-[var(--zf-muted)] mt-1 font-mono truncate">{check.detail}</div>
                  )}
                </div>
                <span
                  className={`px-2 py-0.5 rounded-full text-xs font-medium border ${
                    check.status === 'ok'
                      ? 'text-emerald-700 bg-emerald-50 border-emerald-200'
                      : check.status === 'warning'
                        ? 'text-amber-800 bg-amber-50 border-amber-200'
                        : 'text-red-700 bg-red-50 border-red-200'
                  }`}
                >
                  {check.status}
                </span>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  )
}
