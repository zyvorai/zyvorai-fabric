// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Zap, CheckCircle, AlertTriangle, Loader2, TrendingDown, TrendingUp, Minus } from 'lucide-react'
import { apiFetch } from '../api/client'
import PageLoadBanner from '../components/PageLoadBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody } from '../utils/apiError'
import { usePageLoader } from '../hooks/usePageLoader'
import { useToastContext } from '../contexts/ToastContext'
import { toastFailure } from '../utils/toastError'

interface Recommendation {
  resource: string
  current_value: string
  recommended_value: string
  reason: string
  impact: string
}

interface VMOptimization {
  vm_name: string
  recommendations: Recommendation[]
}

interface OptimizationResult {
  vm_name: string
  applied: string[]
  skipped: string[]
}

function impactColor(impact: string): string {
  switch (impact?.toLowerCase()) {
    case 'high': return 'text-red-700 bg-red-50 border-red-200'
    case 'medium': return 'text-amber-800 bg-amber-50 border-amber-200'
    case 'low': return 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
    default: return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
  }
}

function impactIcon(impact: string) {
  switch (impact?.toLowerCase()) {
    case 'high': return <TrendingUp className="w-3.5 h-3.5" />
    case 'medium': return <Minus className="w-3.5 h-3.5" />
    default: return <TrendingDown className="w-3.5 h-3.5" />
  }
}

export default function ResourceOptimizer() {
  const toast = useToastContext()
  const [optimizations, setOptimizations] = useState<VMOptimization[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load recommendations')
    const [applying, setApplying] = useState<string | null>(null)
  const [results, setResults] = useState<Record<string, OptimizationResult>>({})

  const fetchRecommendations = useCallback(() => {
    return run(async () => {
      const res = await apiFetch('/api/system/optimization/recommendations')
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const data = await res.json()
      setOptimizations(Array.isArray(data) ? data : data.recommendations || [])
    })
  }, [run])

  useEffect(() => { fetchRecommendations() }, [fetchRecommendations])

  const applyOptimization = async (vmName: string) => {
    setApplying(vmName)
    try {
      const res = await apiFetch(`/api/vms/${vmName}/optimize`, { method: 'POST' })
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const result = await res.json()
      setResults(prev => ({ ...prev, [vmName]: result }))
    } catch (err) {
      toastFailure(toast, `Failed to optimize ${vmName}`, err)
    } finally { setApplying(null) }
  }

  const totalRecs = optimizations.reduce((sum, o) => sum + o.recommendations.length, 0)
  const highImpact = optimizations.reduce((sum, o) => sum + o.recommendations.filter(r => r.impact?.toLowerCase() === 'high').length, 0)
  const medImpact = optimizations.reduce((sum, o) => sum + o.recommendations.filter(r => r.impact?.toLowerCase() === 'medium').length, 0)

  if (loading && optimizations.length === 0 && !loadError) {
    return (
      <div className="space-y-6">
        <PageHeader title="Resource Optimizer" description="Right-sizing recommendations for your VMs" />
        <div className="flex items-center justify-center h-64 text-[var(--zf-muted)]">
          <div className="animate-spin w-6 h-6 border-2 border-[var(--zf-ink)] border-t-transparent rounded-full mr-3" />
          Analyzing resources…
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title="Resource Optimizer"
        description="Right-sizing recommendations for your VMs"
        onRefresh={() => void fetchRecommendations()}
        refreshing={loading}
      />
      <PageLoadBanner title="Could not load recommendations" headline={loadError} onRetry={() => void fetchRecommendations()} />

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        <div className="stat-card-blue rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{optimizations.length}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">VMs Analyzed</div>
        </div>
        <div className="stat-card-purple rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow-purple transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{totalRecs}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Recommendations</div>
        </div>
        <div className="stat-card-red rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{highImpact}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">High Impact</div>
        </div>
        <div className="stat-card-orange rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{medImpact}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Medium Impact</div>
        </div>
      </div>

      {optimizations.length === 0 ? (
        <div className="bg-emerald-50 border border-emerald-200 rounded-xl p-10 text-center">
          <CheckCircle className="w-10 h-10 text-emerald-600 mx-auto mb-3" />
          <p className="text-sm text-emerald-600 font-medium">All VMs are optimally configured</p>
          <p className="text-xs text-[var(--zf-muted)] mt-1">No right-sizing recommendations at this time</p>
        </div>
      ) : (
        <div className="space-y-4">
          {optimizations.map((opt) => {
            const result = results[opt.vm_name]
            return (
              <div key={opt.vm_name} className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
                <div className="px-5 py-4 border-b border-[var(--zf-hairline)] flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <h3 className="text-base font-semibold text-[var(--zf-ink)]">{opt.vm_name}</h3>
                    <span className="text-xs font-medium text-[var(--zf-muted)] bg-[var(--zf-canvas)] px-2.5 py-1 rounded-full">{opt.recommendations.length} recommendations</span>
                  </div>
                  <button
                    onClick={() => applyOptimization(opt.vm_name)}
                    disabled={applying === opt.vm_name || !!result}
                    className="zf-btn zf-btn-primary zf-btn-sm"
                  >
                    {applying === opt.vm_name ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Zap className="w-3.5 h-3.5" />}
                    {result ? 'Applied' : applying === opt.vm_name ? 'Applying...' : 'Auto-Optimize'}
                  </button>
                </div>

                <div className="divide-y divide-[var(--zf-hairline)]/30">
                  {opt.recommendations.map((rec, idx) => (
                    <div key={idx} className="px-5 py-3 flex items-start gap-4">
                      <div className={`flex items-center gap-1 px-2 py-1 rounded-lg border text-xs font-medium ${impactColor(rec.impact)}`}>
                        {impactIcon(rec.impact)}
                        {rec.impact}
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className="text-sm font-medium text-[var(--zf-ink)] capitalize">{rec.resource}</div>
                        <div className="text-xs text-[var(--zf-muted)] mt-0.5">{rec.reason}</div>
                        <div className="flex items-center gap-3 mt-2">
                          <div className="bg-[var(--zf-canvas)] rounded-lg px-3 py-1.5">
                            <div className="text-xs text-[var(--zf-muted)]">Current</div>
                            <div className="text-xs font-semibold text-[var(--zf-ink)]">{rec.current_value}</div>
                          </div>
                          <span className="text-[var(--zf-muted)]">&rarr;</span>
                          <div className="bg-emerald-50 border border-emerald-200 rounded-lg px-3 py-1.5">
                            <div className="text-xs text-emerald-700">Recommended</div>
                            <div className="text-xs font-semibold text-emerald-700">{rec.recommended_value}</div>
                          </div>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>

                {result && (
                  <div className="px-5 py-3 bg-[var(--zf-canvas)] border-t border-[var(--zf-hairline)]">
                    <div className="flex items-center gap-2 text-xs">
                      {result.applied.length > 0 && <span className="text-emerald-700"><CheckCircle className="w-3.5 h-3.5 inline mr-1" />{result.applied.length} applied</span>}
                      {result.skipped.length > 0 && <span className="text-amber-700"><AlertTriangle className="w-3.5 h-3.5 inline mr-1" />{result.skipped.length} skipped</span>}
                    </div>
                  </div>
                )}
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
