// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { GitCompare } from 'lucide-react'
import { apiFetch } from '../api/client'
import { listVMs } from '../api/vm'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface VMOption {
  name: string
  state?: string
}

interface CompareField {
  label: string
  source: string
  target: string
  match: boolean
}

interface CompareResult {
  source_name: string
  target_name: string
  fields: CompareField[]
  timestamp: string
}

export default function VMCompare() {
  const toast = useToastContext()
  const [vms, setVMs] = useState<VMOption[]>([])
  const [sourceVM, setSourceVM] = useState('')
  const [targetVM, setTargetVM] = useState('')
  const [result, setResult] = useState<CompareResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [loadingVMs, setLoadingVMs] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [compareError, setCompareError] = useState<string | null>(null)

  const fetchVMs = useCallback(async () => {
    setLoadingVMs(true)
    setLoadError(null)
    try {
      setVMs((await listVMs()).map((vm) => ({ name: vm.name, state: vm.state })))
    } catch (err) {
      const msg = formatUserError(err)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load VMs', err)
    } finally {
      setLoadingVMs(false)
    }
  }, [toast])

  useEffect(() => {
    fetchVMs()
  }, [fetchVMs])

  const handleCompare = async () => {
    if (!sourceVM || !targetVM) return
    setLoading(true)
    setCompareError(null)
    setResult(null)
    try {
      const res = await apiFetch(
        `/api/vms/compare?source=${encodeURIComponent(sourceVM)}&target=${encodeURIComponent(targetVM)}`,
      )
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      setResult(await res.json())
    } catch (err) {
      const msg = formatUserError(err)
      setCompareError(msg)
      toastFailure(toast, 'VM comparison failed', err)
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title="VM Comparison"
        description="Compare two VM configurations side-by-side"
        onRefresh={fetchVMs}
        refreshing={loadingVMs}
      />

      {loadError && (
        <ErrorBanner
          title="Could not load virtual machines"
          headline={loadError}
          hints={hintsForError(loadError, 'vm')}
          onRetry={fetchVMs}
        />
      )}

      {compareError && (
        <ErrorBanner
          title="Comparison failed"
          headline={compareError}
          hints={hintsForError(compareError, 'vm')}
        />
      )}

      {loadingVMs && !loadError ? (
        <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] flex flex-col items-center justify-center text-[var(--zf-muted)] gap-3">
          <div className="w-6 h-6 border-2 border-[var(--zf-hairline)] border-t-[var(--zf-link)] rounded-full animate-spin" />
          <span className="text-sm">Loading VM list…</span>
        </div>
      ) : !loadError ? (
        <>
          <div className="bg-[var(--zf-canvas)] rounded-xl p-4 border border-[var(--zf-hairline)]">
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4 items-end">
              <div>
                <label className="block text-xs font-medium text-[var(--zf-muted)] mb-1.5">Source VM</label>
                <select aria-label="Source VM"
                  value={sourceVM}
                  onChange={(e) => setSourceVM(e.target.value)}
                  className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] focus:outline-none focus:ring-1 focus:ring-[var(--zf-ink)]"
                >
                  <option value="">Select source VM…</option>
                  {vms.map((vm) => (
                    <option key={vm.name} value={vm.name}>
                      {vm.name} {vm.state ? `(${vm.state})` : ''}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className="block text-xs font-medium text-[var(--zf-muted)] mb-1.5">Target VM</label>
                <select aria-label="Target VM"
                  value={targetVM}
                  onChange={(e) => setTargetVM(e.target.value)}
                  className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] focus:outline-none focus:ring-1 focus:ring-[var(--zf-ink)]"
                >
                  <option value="">Select target VM…</option>
                  {vms
                    .filter((v) => v.name !== sourceVM)
                    .map((vm) => (
                      <option key={vm.name} value={vm.name}>
                        {vm.name} {vm.state ? `(${vm.state})` : ''}
                      </option>
                    ))}
                </select>
              </div>
              <button
                type="button"
                onClick={handleCompare}
                disabled={loading || !sourceVM || !targetVM}
                className="zf-btn zf-btn-primary"
              >
                <GitCompare className="w-4 h-4" />
                {loading ? 'Comparing…' : 'Compare'}
              </button>
            </div>
          </div>

          {result && (
            <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
              <div className="px-4 py-3 border-b border-[var(--zf-hairline)] flex items-center justify-between">
                <h3 className="text-sm font-semibold text-[var(--zf-ink)]">
                  {result.source_name} vs {result.target_name}
                </h3>
                <span className="text-xs text-[var(--zf-muted)]">
                  {new Date(result.timestamp).toLocaleString()}
                </span>
              </div>
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-[var(--zf-hairline)] bg-[var(--zf-canvas)]">
                    <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase">Field</th>
                    <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase">Source</th>
                    <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase">Target</th>
                    <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase w-20">Match</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-[var(--zf-hairline)]/30">
                  {result.fields.map((field) => (
                    <tr key={field.label} className="hover:bg-black/[0.04]">
                      <td className="p-3 text-[var(--zf-ink)] font-medium">{field.label}</td>
                      <td className="p-3 text-[var(--zf-muted)] font-mono text-xs">{field.source}</td>
                      <td className="p-3 text-[var(--zf-muted)] font-mono text-xs">{field.target}</td>
                      <td className="p-3">
                        <span
                          className={`text-xs font-medium ${field.match ? 'text-emerald-700' : 'text-amber-800'}`}
                        >
                          {field.match ? 'Yes' : 'No'}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      ) : null}
    </div>
  )
}
