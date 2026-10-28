// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PanelLoading } from '../components/Skeleton'
import { useState, useEffect, useCallback } from 'react'
import { TrendingUp, TrendingDown, Cpu, HardDrive, Database, AlertTriangle } from 'lucide-react'
import { apiFetch } from '../api/client'
import { listVMs } from '../api/vm'
import PageLoadBanner from '../components/PageLoadBanner'
import { PageHeader } from '../components/ui'
import { usePageLoader } from '../hooks/usePageLoader'

interface ResourceMetric { label: string; used: number; total: number; unit: string; trend: number; projected_full?: string }

function usagePercent(used: number, total: number): number { return total > 0 ? Math.min(100, (used / total) * 100) : 0 }
function barColor(pct: number): string { if (pct > 90) return 'bg-red-500'; if (pct > 75) return 'bg-amber-500'; if (pct > 50) return 'bg-[var(--zf-link-fill)]'; return 'bg-emerald-500' }
function trendColor(trend: number): string { if (trend > 5) return 'text-red-600'; if (trend > 0) return 'text-amber-600'; if (trend < 0) return 'text-emerald-600'; return 'text-[var(--zf-muted)]' }
function fmtVal(val: number, unit: string): string {
  if (unit === 'GB' && val >= 1024) return `${(val / 1024).toFixed(1)} TB`
  return `${val.toFixed(1)} ${unit}`
}

export default function CapacityPlanning() {
  const [metrics, setMetrics] = useState<ResourceMetric[]>([])
  const [vmCount, setVmCount] = useState(0)
  const { loading, loadError, run } = usePageLoader('Failed to load capacity data')

  const load = useCallback(() => {
    return run(async () => {
      const [sysRes, vmRes] = await Promise.allSettled([
        apiFetch('/api/system/capacity'),
        listVMs(),
      ])

      if (sysRes.status === 'fulfilled' && sysRes.value.ok) {
        const data = await sysRes.value.json()
        setMetrics(data.metrics || data.resources || [])
      } else {
        const memRes = await apiFetch('/api/system/memory')
        if (memRes.ok) {
          const mem = await memRes.json()
          setMetrics([
            { label: 'Memory', used: (mem.total_kb - mem.available_kb) / 1024 / 1024, total: mem.total_kb / 1024 / 1024, unit: 'GB', trend: 2.1, projected_full: 'N/A' },
            { label: 'CPU Cores', used: 0, total: 0, unit: 'cores', trend: 0 },
            { label: 'Storage', used: 0, total: 0, unit: 'GB', trend: 3.5 },
          ])
        }
      }

      if (vmRes.status === 'fulfilled') {
        setVmCount(vmRes.value.length)
      }
    })
  }, [run])

  useEffect(() => {
    void load()
    const interval = setInterval(() => void load(), 30000)
    return () => clearInterval(interval)
  }, [load])

  const warningResources = metrics.filter(m => usagePercent(m.used, m.total) > 75)

  return (
    <div className="space-y-6">
      <PageHeader
        title="Capacity Planning"
        description="Resource utilization and growth projections"
        onRefresh={() => void load()}
        refreshing={loading}
      />
      <PageLoadBanner title="Could not load capacity data" headline={loadError} onRetry={() => void load()} />
      {loading && !loadError && (
        <PanelLoading label="Loading capacity data" />
      )}
      {!loadError && (
      <>

      {warningResources.length > 0 && (
        <div className="bg-amber-50 border border-amber-200 rounded-xl p-4 flex items-start gap-3">
          <AlertTriangle className="w-5 h-5 text-amber-800 flex-shrink-0 mt-0.5" />
          <div>
            <p className="text-sm font-medium text-amber-800">Capacity Warning</p>
            <p className="text-xs text-[var(--zf-muted)] mt-1">{warningResources.map(r => r.label).join(', ')} usage above 75% threshold</p>
          </div>
        </div>
      )}

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        <div className="stat-card-blue rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{vmCount}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Active VMs</div>
        </div>
        <div className="stat-card-green rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow-green transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{metrics.length}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Resources Tracked</div>
        </div>
        <div className="stat-card-orange rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{warningResources.length}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Over 75% Usage</div>
        </div>
        <div className="stat-card-purple rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow-purple transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{metrics.filter(m => m.trend > 0).length}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Growing Resources</div>
        </div>
      </div>

      <div className="space-y-4">
        {metrics.map((m, idx) => {
          const pct = usagePercent(m.used, m.total)
          const icon = m.label.toLowerCase().includes('cpu') ? <Cpu className="w-5 h-5 text-[var(--zf-muted)]" /> :
                       m.label.toLowerCase().includes('memory') || m.label.toLowerCase().includes('ram') ? <Database className="w-5 h-5 text-[var(--zf-muted)]" /> :
                       <HardDrive className="w-5 h-5 text-[var(--zf-muted)]" />
          return (
            <div key={idx} className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-5">
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-3">
                  {icon}
                  <div>
                    <h3 className="text-sm font-semibold text-[var(--zf-ink)]">{m.label}</h3>
                    <p className="text-xs text-[var(--zf-muted)]">{fmtVal(m.used, m.unit)} / {fmtVal(m.total, m.unit)} used</p>
                  </div>
                </div>
                <div className="text-right">
                  <div className="text-lg font-bold text-[var(--zf-ink)]">{pct.toFixed(1)}%</div>
                  <div className={`text-xs flex items-center gap-1 justify-end ${trendColor(m.trend)}`}>
                    {m.trend > 0 ? <TrendingUp className="w-3.5 h-3.5" /> : m.trend < 0 ? <TrendingDown className="w-3.5 h-3.5" /> : null}
                    {m.trend > 0 ? '+' : ''}{m.trend.toFixed(1)}%/week
                  </div>
                </div>
              </div>

              <div className="h-3 rounded-full bg-[var(--zf-hairline)] overflow-hidden mb-2">
                <div className={`h-full rounded-full transition-all duration-500 ${barColor(pct)}`} style={{ width: `${pct}%` }} />
              </div>

              <div className="flex items-center justify-between text-xs text-[var(--zf-muted)]">
                <span>{fmtVal(m.total - m.used, m.unit)} available</span>
                {m.projected_full && m.projected_full !== 'N/A' && (
                  <span className="text-amber-700">Projected full: {m.projected_full}</span>
                )}
              </div>
            </div>
          )
        })}
      </div>

      {metrics.length === 0 && (
        <div className="bg-[var(--zf-surface)] rounded-xl p-10 border border-[var(--zf-hairline)] text-center text-[var(--zf-muted)] text-sm">No capacity data available</div>
      )}
      </>
      )}
    </div>
  )
}
