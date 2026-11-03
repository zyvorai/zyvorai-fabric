// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Box } from 'lucide-react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, EmptyState, Card, CardBody } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return (bytes / Math.pow(1024, i)).toFixed(1) + ' ' + units[i]
}

function stateColor(state: string): string {
  const s = (state || '').toLowerCase()
  if (s === 'running') return 'text-emerald-700 bg-emerald-50 border-emerald-200'
  if (s === 'exited' || s === 'stopped') return 'text-red-700 bg-red-50 border-red-200'
  if (s === 'paused') return 'text-amber-800 bg-amber-50 border-amber-200'
  if (s === 'restarting') return 'text-[var(--zf-link)] bg-[var(--zf-link)]/10 border-[var(--zf-link)]/20'
  return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
}

function ProgressBar({ pct, color }: { pct: number; color: string }) {
  const clamped = Math.min(100, Math.max(0, pct))
  return (
    <div className="h-1.5 rounded-full bg-[var(--zf-canvas)] w-full">
      <div
        className={`h-full rounded-full transition-all duration-500 ${color}`}
        style={{ width: `${clamped}%` }}
      />
    </div>
  )
}

export default function Containers() {
  const toast = useToastContext()
  const [data, setData] = useState<any>(null)
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [refreshError, setRefreshError] = useState<string | null>(null)

  const fetchData = useCallback(async () => {
    try {
      const res = await apiFetch('/api/system/containers')
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const json = await res.json()
      setData(json)
      setLoadError(null)
      setRefreshError(null)
    } catch (err) {
      const msg = formatUserError(err)
      setData((prev: any) => {
        if (prev == null) {
          setLoadError(msg)
          toastFailure(toast, 'Failed to load containers', err)
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
    const interval = setInterval(fetchData, 3000)
    return () => clearInterval(interval)
  }, [fetchData])

  const containers: any[] = data?.containers || []
  const summary = data?.summary
  const totalContainers = summary?.total ?? containers.length
  const runningCount = summary?.running ?? containers.filter((c: any) => (c.state || '').toLowerCase() === 'running').length
  const totalCpu = summary?.total_cpu ?? containers.reduce((acc: number, c: any) => acc + (c.cpu_percent ?? 0), 0)
  const totalMem = summary?.total_memory ?? containers.reduce((acc: number, c: any) => acc + (c.memory_bytes ?? 0), 0)

  return (
    <div className="space-y-6">
      <PageHeader
        title="Containers"
        description="Container runtime monitoring"
        onRefresh={fetchData}
        refreshing={loading}
      />

      {loadError && (
        <ErrorBanner
          title="Could not load containers"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={fetchData}
        />
      )}

      {refreshError && !loadError && (
        <div className="rounded-lg text-xs text-amber-800 bg-amber-50 border border-amber-200 px-4 py-2">
          Refresh failed: {refreshError} — showing last known data
        </div>
      )}

      {loading && !data && !loadError ? (
        <div className="flex items-center justify-center py-20">
          <div className="w-8 h-8 border-2 border-[var(--zf-hairline)] border-t-[var(--zf-ink)] rounded-full animate-spin" />
        </div>
      ) : !loadError ? (
        <>

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        <Card><CardBody className="px-4 py-3">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{totalContainers}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Total Containers</div>
        </CardBody></Card>
        <Card><CardBody className="px-4 py-3">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{runningCount}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Running</div>
        </CardBody></Card>
        <Card><CardBody className="px-4 py-3">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{totalCpu.toFixed(1)}%</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Total CPU</div>
        </CardBody></Card>
        <Card><CardBody className="px-4 py-3">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{formatBytes(totalMem)}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Total Memory</div>
        </CardBody></Card>
      </div>

      {containers.length === 0 ? (
        <Card>
          <EmptyState icon={<Box className="w-10 h-10" />} title="No containers" description="No container workloads detected on this host" />
        </Card>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {containers.map((c: any, i: number) => (
            <div key={c.id || i} className="zf-panel p-4">
              <div className="flex items-center justify-between mb-3">
                <span className="text-sm font-semibold text-[var(--zf-ink)] truncate mr-2">{c.name || c.id?.slice(0, 12) || 'unknown'}</span>
                <span className={`text-xs font-medium px-2 py-0.5 rounded-full border ${stateColor(c.state)}`}>
                  {c.state || 'unknown'}
                </span>
              </div>

              {c.image && (
                <div className="text-xs text-[var(--zf-muted)] truncate mb-3 font-mono">{c.image}</div>
              )}

              <div className="mb-2">
                <div className="flex items-center justify-between mb-1">
                  <span className="text-xs text-[var(--zf-muted)]">CPU</span>
                  <span className="text-xs font-medium text-[var(--zf-ink)]">{(c.cpu_percent ?? 0).toFixed(1)}%</span>
                </div>
                <ProgressBar pct={c.cpu_percent ?? 0} color="bg-[var(--zf-ink)]" />
              </div>

              <div className="mb-2">
                <div className="flex items-center justify-between mb-1">
                  <span className="text-xs text-[var(--zf-muted)]">Memory</span>
                  <span className="text-xs font-medium text-[var(--zf-ink)]">
                    {c.memory_bytes ? formatBytes(c.memory_bytes) : `${(c.memory_percent ?? 0).toFixed(1)}%`}
                  </span>
                </div>
                <ProgressBar pct={c.memory_percent ?? (c.memory_limit ? (c.memory_bytes / c.memory_limit) * 100 : 0)} color="bg-[var(--zf-secondary)]" />
              </div>

              {(c.net_rx !== undefined || c.net_tx !== undefined) && (
                <div className="flex gap-4 text-xs text-[var(--zf-muted)] mt-2 pt-2 border-t border-[var(--zf-hairline)]/60">
                  <span>RX: {formatBytes(c.net_rx ?? 0)}</span>
                  <span>TX: {formatBytes(c.net_tx ?? 0)}</span>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
        </>
      ) : null}
    </div>
  )
}
