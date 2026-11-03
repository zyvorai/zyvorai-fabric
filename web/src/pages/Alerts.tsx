// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PanelLoading } from '../components/Skeleton'
import { useState, useEffect, useCallback } from 'react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

function severityBorderClass(severity: string): string {
  switch (severity) {
    case 'critical':
      return 'border-l-[var(--zf-danger)]'
    case 'warning':
      return 'border-l-[var(--zf-warning)]'
    default:
      return 'border-l-[var(--zf-link)]'
  }
}

function severityBadgeClasses(severity: string): string {
  switch (severity) {
    case 'critical':
      return 'text-red-700 bg-red-50 border border-red-200'
    case 'warning':
      return 'text-amber-800 bg-amber-50 border border-amber-200'
    default:
      return 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border border-[var(--zf-hairline)]'
  }
}

export default function Alerts() {
  const toast = useToastContext()
  const [alerts, setAlerts] = useState<any[]>([])
  const [rules, setRules] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [refreshError, setRefreshError] = useState<string | null>(null)

  const loadAll = useCallback(async (silent = false) => {
    try {
      const res = await apiFetch('/api/system/alerts')
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const data = await res.json()
      setAlerts(Array.isArray(data) ? data : data.alerts ?? [])

      try {
        const rulesRes = await apiFetch('/api/system/alerts/rules')
        if (rulesRes.ok) {
          const rulesData = await rulesRes.json()
          setRules(Array.isArray(rulesData) ? rulesData : rulesData.rules ?? [])
        }
      } catch {
        setRules([])
      }
      setLoadError(null)
      setRefreshError(null)
    } catch (err) {
      const msg = formatUserError(err)
      setAlerts((prev) => {
        if (!silent && prev.length === 0) {
          setLoadError(msg)
          toastFailure(toast, 'Failed to load alerts', err)
        } else if (silent || prev.length > 0) {
          setRefreshError(msg)
        }
        return prev
      })
    } finally {
      if (!silent) setLoading(false)
    }
  }, [toast])

  useEffect(() => {
    void loadAll(false)
    const interval = setInterval(() => void loadAll(true), 5000)
    return () => clearInterval(interval)
  }, [loadAll])

  const activeCount = alerts.length
  const criticalCount = alerts.filter((a: any) => a.severity === 'critical').length
  const warningCount = alerts.filter((a: any) => a.severity === 'warning').length

  if (loading && alerts.length === 0 && rules.length === 0 && !loadError) {
    return (
      <div className="space-y-6">
        <PageHeader title="Alerts" description="System alerts and notification rules" />
        <PanelLoading label="Loading alerts" />
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title="Alerts"
        description="System alerts and notification rules"
        onRefresh={loadAll}
        refreshing={loading}
      />
      {loadError && (
        <ErrorBanner
          title="Could not load alerts"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={loadAll}
        />
      )}
      {refreshError && !loadError && (
        <div className="bg-amber-50 rounded-lg border border-amber-200 px-4 py-2 text-xs text-amber-800">
          {refreshError} — showing last known data
        </div>
      )}

      {!loadError && (
        <>
          <div className="grid grid-cols-3 gap-3">
            <div className="bg-[var(--zf-canvas)] rounded-xl px-4 py-3 border border-[var(--zf-hairline)]">
              <div className="text-xs text-[var(--zf-muted)] mb-1">Active Alerts</div>
              <div className="text-2xl font-bold text-[var(--zf-ink)]">{activeCount}</div>
            </div>
            <div className="bg-[var(--zf-canvas)] rounded-xl px-4 py-3 border border-[var(--zf-hairline)]">
              <div className="text-xs text-[var(--zf-muted)] mb-1">Critical</div>
              <div className="text-2xl font-bold text-red-700">{criticalCount}</div>
            </div>
            <div className="bg-[var(--zf-canvas)] rounded-xl px-4 py-3 border border-[var(--zf-hairline)]">
              <div className="text-xs text-[var(--zf-muted)] mb-1">Warning</div>
              <div className="text-2xl font-bold text-amber-800">{warningCount}</div>
            </div>
          </div>

          <div>
            <h2 className="text-lg font-semibold text-[var(--zf-ink)] mb-3">Active Alerts</h2>
            {alerts.length === 0 ? (
              <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] flex flex-col items-center justify-center text-[var(--zf-muted)] gap-3">
                <span className="text-sm">No active alerts</span>
              </div>
            ) : (
              <div className="flex flex-col gap-3">
                {alerts.map((alert: any, idx: number) => (
                  <div
                    key={alert.id ?? idx}
                    className={`bg-[var(--zf-canvas)] rounded-xl p-4 border border-[var(--zf-hairline)] border-l-4 ${severityBorderClass(alert.severity)}`}
                  >
                    <div className="flex items-center gap-2 mb-2">
                      <span
                        className={`rounded-full px-2.5 py-0.5 text-xs font-medium ${severityBadgeClasses(alert.severity)}`}
                      >
                        {alert.severity}
                      </span>
                      <span className="text-xs text-[var(--zf-muted)]">
                        {alert.timestamp ? new Date(alert.timestamp).toLocaleString() : ''}
                      </span>
                    </div>
                    <div className="text-sm font-semibold text-[var(--zf-ink)] mb-1">
                      {alert.title ?? alert.name ?? 'Alert'}
                    </div>
                    <div className="text-sm text-[var(--zf-muted)]">{alert.message}</div>
                    {alert.value !== undefined && (
                      <div className="text-xs text-[var(--zf-muted)] mt-2">Value: {alert.value}</div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>

          {rules.length > 0 && (
            <div>
              <h2 className="text-lg font-semibold text-[var(--zf-ink)] mb-3">Alert Rules</h2>
              <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-[var(--zf-hairline)]">
                      <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">
                        Name
                      </th>
                      <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">
                        Condition
                      </th>
                      <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">
                        Threshold
                      </th>
                      <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">
                        Severity
                      </th>
                      <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">
                        Enabled
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {rules.map((rule: any, idx: number) => (
                      <tr
                        key={rule.name ?? idx}
                        className="border-b border-[var(--zf-hairline)]/60 hover:bg-black/[0.04] transition-colors"
                      >
                        <td className="px-4 py-3 text-[var(--zf-ink)] font-medium">{rule.name}</td>
                        <td className="px-4 py-3 text-[var(--zf-ink)]">{rule.condition}</td>
                        <td className="px-4 py-3 text-[var(--zf-ink)]">{rule.threshold}</td>
                        <td className="px-4 py-3">
                          <span
                            className={`rounded-full px-2.5 py-0.5 text-xs font-medium ${severityBadgeClasses(rule.severity)}`}
                          >
                            {rule.severity}
                          </span>
                        </td>
                        <td className="px-4 py-3">
                          <span
                            className={`text-xs font-medium ${rule.enabled ? 'text-emerald-600' : 'text-[var(--zf-muted)]'}`}
                          >
                            {rule.enabled ? 'Yes' : 'No'}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </>
      )}
    </div>
  )
}
