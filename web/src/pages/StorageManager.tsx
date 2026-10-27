// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Database } from 'lucide-react'
import { apiFetch } from '../api/client'
import PageLoadBanner from '../components/PageLoadBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody } from '../utils/apiError'
import { usePageLoader } from '../hooks/usePageLoader'

interface StoragePool { name: string; state: string; type: string; capacity: number; allocation: number; available: number }
interface Volume { name: string; path: string; capacity: number; allocation: number; format: string }

function fmtB(bytes: number): string { if (!bytes) return '0 B'; const u = ['B', 'KB', 'MB', 'GB', 'TB']; const i = Math.floor(Math.log(bytes) / Math.log(1024)); return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${u[i]}` }
function pct(used: number, total: number): number { return total > 0 ? Math.min(100, (used / total) * 100) : 0 }
function barColor(p: number): string { if (p > 90) return 'bg-[var(--zf-danger)]'; if (p > 70) return 'bg-[var(--zf-warning)]'; return 'bg-[var(--zf-ink)]' }

const typeBadge: Record<string, string> = { dir: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]', logical: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]', netfs: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]', disk: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]', iscsi: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]', rbd: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]', zfs: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]' }

export default function StorageManager() {
  const [pools, setPools] = useState<StoragePool[]>([])
  const [selectedPool, setSelectedPool] = useState<string | null>(null)
  const [volumes, setVolumes] = useState<Volume[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load storage pools')
  const [volumeError, setVolumeError] = useState<string | null>(null)

  const fetchPools = useCallback(() => {
    return run(async () => {
      const res = await apiFetch('/api/storage/pools')
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const data = await res.json()
      setPools(Array.isArray(data) ? data : data.pools || [])
    })
  }, [run])

  const fetchVolumes = useCallback(async (poolName: string) => {
    setVolumeError(null)
    try {
      const res = await apiFetch(`/api/storage/pools/${poolName}/volumes`)
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const data = await res.json()
      setVolumes(Array.isArray(data) ? data : data.volumes || [])
    } catch (err) {
      setVolumeError(err instanceof Error ? err.message : 'Failed to load volumes')
      setVolumes([])
    }
  }, [])

  useEffect(() => { fetchPools() }, [fetchPools])
  useEffect(() => { if (selectedPool) fetchVolumes(selectedPool) }, [selectedPool, fetchVolumes])

  if (loading) return <div className="flex items-center justify-center h-64 text-[var(--zf-muted)]"><div className="animate-spin w-6 h-6 border-2 border-[var(--zf-ink)] border-t-transparent rounded-full mr-3" />Loading storage...</div>

  return (
    <div className="space-y-6">
      <PageLoadBanner title="Could not load storage pools" headline={loadError} onRetry={() => void fetchPools()} />

      <PageHeader
        title="Storage Manager"
        description="Storage pools and volumes"
        onRefresh={() => void fetchPools()}
        refreshing={loading}
      />
      {volumeError && (
        <div className="bg-amber-50 border border-amber-200 rounded-xl p-4 text-sm text-amber-800">{volumeError}</div>
      )}

      {pools.length === 0 ? (
        <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] text-center text-[var(--zf-muted)]"><Database className="w-10 h-10 mx-auto mb-3 opacity-50" /><p className="text-sm">No storage pools found</p></div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {pools.map(pool => {
            const usage = pct(pool.allocation, pool.capacity)
            const isSelected = selectedPool === pool.name
            return (
              <div key={pool.name} role="button" tabIndex={0} onClick={() => setSelectedPool(pool.name)} onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setSelectedPool(pool.name) } }} className={`bg-[var(--zf-canvas)] rounded-xl border p-5 cursor-pointer transition-all hover:scale-[1.01] card-glow ${isSelected ? 'border-[var(--zf-ink)] ring-1 ring-[var(--zf-ink)]/10' : 'border-[var(--zf-hairline)]'}`}>
                <div className="flex items-center justify-between mb-3">
                  <h3 className="text-sm font-semibold text-[var(--zf-ink)]">{pool.name}</h3>
                  <div className="flex items-center gap-2">
                    <span className={`px-2 py-0.5 rounded-full text-xs font-medium border ${typeBadge[pool.type] || 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'}`}>{pool.type}</span>
                    <span className={`px-2 py-0.5 rounded-full text-xs font-medium border ${pool.state === 'running' ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'}`}>{pool.state}</span>
                  </div>
                </div>
                <div className="mb-2"><div className="flex justify-between text-xs text-[var(--zf-muted)] mb-1"><span>{fmtB(pool.allocation)} used</span><span>{fmtB(pool.capacity)}</span></div><div className="h-2 rounded-full bg-[var(--zf-canvas)]"><div className={`h-full rounded-full transition-all ${barColor(usage)}`} style={{ width: `${usage}%` }} /></div></div>
                <div className="text-xs text-[var(--zf-muted)]">{fmtB(pool.available)} available ({usage.toFixed(1)}% used)</div>
              </div>
            )
          })}
        </div>
      )}

      {selectedPool && volumes.length > 0 && (
        <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
          <div className="px-5 py-4 border-b border-[var(--zf-hairline)] flex items-center justify-between">
            <h3 className="text-base font-semibold text-[var(--zf-ink)]">Volumes in {selectedPool}</h3>
            <span className="text-xs font-medium text-[var(--zf-muted)] bg-[var(--zf-canvas)] px-2.5 py-1 rounded-full">{volumes.length}</span>
          </div>
          <table className="w-full text-sm">
            <thead><tr className="border-b border-[var(--zf-hairline)]">
              <th className="text-left px-5 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase">Name</th>
              <th className="text-left px-5 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase">Format</th>
              <th className="text-left px-5 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase">Capacity</th>
              <th className="text-left px-5 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase">Allocation</th>
              <th className="text-left px-5 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase">Path</th>
            </tr></thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]/30">
              {volumes.map(vol => (
                <tr key={vol.name} className="hover:bg-black/[0.04] transition-colors">
                  <td className="px-5 py-3 text-[var(--zf-ink)] font-medium">{vol.name}</td>
                  <td className="px-5 py-3"><span className="px-2 py-0.5 rounded-full text-xs font-medium bg-black/[0.06] text-[var(--zf-muted)]">{vol.format}</span></td>
                  <td className="px-5 py-3 text-[var(--zf-ink)] text-xs font-mono">{fmtB(vol.capacity)}</td>
                  <td className="px-5 py-3 text-[var(--zf-ink)] text-xs font-mono">{fmtB(vol.allocation)}</td>
                  <td className="px-5 py-3 text-[var(--zf-muted)] text-xs font-mono truncate max-w-[250px]" title={vol.path}>{vol.path}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {selectedPool && volumes.length === 0 && <div className="bg-[var(--zf-canvas)] rounded-xl p-8 border border-[var(--zf-hairline)] text-center text-[var(--zf-muted)] text-sm">No volumes in {selectedPool}</div>}
    </div>
  )
}
