// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { apiFetch } from '../api/client'
import { listVMs } from '../api/vm'
import ErrorBanner from '../components/ErrorBanner'
import PageLoadBanner from '../components/PageLoadBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { hintsForError } from '../utils/daemonHints'
import { usePageLoader } from '../hooks/usePageLoader'

interface VMOption { name: string; state?: string }
interface HealthCheck { name: string; status: string; message: string; detail?: string }
interface HealthCheckResult { vm: string; overall: string; checks: HealthCheck[]; timestamp: string }

function StatusIcon({ status }: { status: string }) {
  if (status === 'pass') return <span className="flex items-center justify-center w-6 h-6 rounded-full bg-emerald-50 text-emerald-700 text-xs">&#10003;</span>
  if (status === 'warning') return <span className="flex items-center justify-center w-6 h-6 rounded-full bg-amber-50 text-amber-800 text-xs">&#9888;</span>
  return <span className="flex items-center justify-center w-6 h-6 rounded-full bg-red-50 text-red-700 text-xs">&#10007;</span>
}

export default function VMHealthCheck() {
  const [vms, setVMs] = useState<VMOption[]>([])
  const [selectedVM, setSelectedVM] = useState('')
  const [result, setResult] = useState<HealthCheckResult | null>(null)
  const [checking, setChecking] = useState(false)
  const [checkError, setCheckError] = useState<string | null>(null)
  const { loading: loadingVMs, loadError, run } = usePageLoader('Failed to load VMs')

  const fetchVMs = useCallback(() => {
    return run(async () => {
      setVMs((await listVMs()).map((vm) => ({ name: vm.name, state: vm.state })))
    })
  }, [run])

  useEffect(() => {
    void fetchVMs()
  }, [fetchVMs])

  const runHealthCheck = async () => {
    if (!selectedVM) return
    setChecking(true)
    setCheckError(null)
    setResult(null)
    try {
      const res = await apiFetch(`/api/vms/${encodeURIComponent(selectedVM)}/healthcheck`)
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      setResult(await res.json())
    } catch (err) {
      setCheckError(formatUserError(err))
    } finally {
      setChecking(false)
    }
  }

  if (loadingVMs && vms.length === 0 && !loadError) {
    return (
      <div className="space-y-6">
        <PageHeader title="VM Health Check" description="Run health verification checks on a VM" />
        <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] flex flex-col items-center justify-center text-[var(--zf-muted)] gap-3">
          <div className="w-6 h-6 border-2 border-[var(--zf-hairline)] border-t-[var(--zf-link)] rounded-full animate-spin" />
          <span className="text-sm">Loading VMs…</span>
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="VM Health Check"
        description="Run health verification checks on a VM"
        onRefresh={() => void fetchVMs()}
        refreshing={loadingVMs}
      />
      <PageLoadBanner title="Could not load VMs" headline={loadError} onRetry={() => void fetchVMs()} />

      {!loadError && (
        <>
          <div className="bg-[var(--zf-canvas)] rounded-xl p-4 border border-[var(--zf-hairline)]">
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4 items-end">
              <div className="md:col-span-2">
                <label className="block text-xs font-medium text-[var(--zf-muted)] mb-1.5">Select VM</label>
                <select aria-label="Select VM"
                  value={selectedVM}
                  onChange={(e) => setSelectedVM(e.target.value)}
                  className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] focus:outline-none focus:ring-1 focus:ring-[var(--zf-ink)]"
                >
                  <option value="">Select a VM…</option>
                  {vms.map((vm) => (
                    <option key={vm.name} value={vm.name}>
                      {vm.name} {vm.state ? `(${vm.state})` : ''}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <button
                  onClick={runHealthCheck}
                  disabled={!selectedVM || checking}
                  className="w-full zf-btn zf-btn-primary"
                >
                  {checking ? 'Checking…' : 'Run Health Check'}
                </button>
              </div>
            </div>
          </div>

          {checkError && (
            <ErrorBanner
              title="Health check failed"
              headline={checkError}
              hints={hintsForError(checkError, 'vm')}
              onRetry={runHealthCheck}
            />
          )}

          {checking && (
            <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] flex flex-col items-center justify-center text-[var(--zf-muted)] gap-3">
              <div className="w-6 h-6 border-2 border-[var(--zf-hairline)] border-t-[var(--zf-link)] rounded-full animate-spin" />
              <span className="text-sm">Running health checks on {selectedVM}…</span>
            </div>
          )}

          {result && !checking && (
            <div className="flex flex-col gap-4">
              <div
                className={`rounded-xl p-4 border flex items-center gap-3 ${result.overall === 'healthy' ? 'bg-emerald-50 border-emerald-200' : 'bg-red-50 border-red-200'}`}
              >
                <div
                  className={`w-8 h-8 rounded-full flex items-center justify-center ${result.overall === 'healthy' ? 'bg-emerald-100 text-emerald-700' : 'bg-red-100 text-red-700'}`}
                >
                  {result.overall === 'healthy' ? '\u2713' : '\u26A0'}
                </div>
                <div>
                  <div className="text-sm font-semibold text-[var(--zf-ink)]">
                    {result.overall === 'healthy' ? 'Healthy' : 'Issues Found'}
                  </div>
                  <div className="text-xs text-[var(--zf-muted)]">
                    {result.checks.filter((c) => c.status === 'pass').length} of {result.checks.length} checks passed for{' '}
                    {result.vm}
                  </div>
                </div>
              </div>

              <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] divide-y divide-[var(--zf-hairline)]">
                {result.checks.map((check, idx) => (
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
                      className={`px-2 py-0.5 rounded-full text-xs font-medium border ${check.status === 'pass' ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : check.status === 'warning' ? 'text-amber-800 bg-amber-50 border-amber-200' : 'text-red-700 bg-red-50 border-red-200'}`}
                    >
                      {check.status}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {!result && !checkError && !checking && (
            <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] flex flex-col items-center justify-center text-[var(--zf-muted)] gap-2">
              <span className="text-sm">Select a VM and run a health check</span>
            </div>
          )}
        </>
      )}
    </div>
  )
}
