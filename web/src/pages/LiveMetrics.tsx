// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useRef, useCallback } from 'react'
import { Cpu, Database, HardDrive, Network, Pause, Play } from 'lucide-react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { hintsForError } from '../utils/daemonHints'

interface MetricPoint { timestamp: number; cpu: number; memory: number; disk_read: number; disk_write: number; net_rx: number; net_tx: number }

const MAX_POINTS = 60

function MiniChart({ data, color, max }: { data: number[]; color: string; max?: number }) {
  if (data.length === 0) return <svg viewBox="0 0 100 40" className="w-full h-10" />
  const height = 40
  const maxVal = max || Math.max(...data, 1)
  const points = data.map((v, i) => {
    const x = (i / (MAX_POINTS - 1)) * 100
    const y = height - (v / maxVal) * height
    return `${x},${y}`
  }).join(' ')

  return (
    <svg viewBox={`0 0 100 ${height}`} preserveAspectRatio="none" className="w-full h-10">
      <polyline points={points} fill="none" stroke={color} strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
    </svg>
  )
}

function fmtB(bytes: number): string {
  if (!bytes || bytes === 0) return '0 B'
  const u = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(Math.abs(bytes)) / Math.log(1024))
  return (bytes / Math.pow(1024, i)).toFixed(1) + ' ' + u[i]
}

export default function LiveMetrics() {
  const [history, setHistory] = useState<MetricPoint[]>([])
  const [paused, setPaused] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [refreshError, setRefreshError] = useState<string | null>(null)
  const pausedRef = useRef(false)
  // net_rx_bytes/net_tx_bytes from the API are cumulative counters (raw
  // /proc/net/dev totals since the interface came up), not a per-second
  // rate -- track the previous raw sample so we can derive an actual
  // rate from the delta instead of displaying the ever-growing total.
  const prevNetRef = useRef<{ rx: number; tx: number; t: number } | null>(null)

  const fetchMetrics = useCallback(async () => {
    if (pausedRef.current) return
    try {
      const res = await apiFetch('/api/system/metrics')
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const data = await res.json()
      const now = Date.now()
      const rawRx = data.net_rx_bytes ?? 0
      const rawTx = data.net_tx_bytes ?? 0
      const prev = prevNetRef.current
      const elapsedSec = prev ? (now - prev.t) / 1000 : 0
      const netRxRate = prev && elapsedSec > 0 ? Math.max(0, rawRx - prev.rx) / elapsedSec : 0
      const netTxRate = prev && elapsedSec > 0 ? Math.max(0, rawTx - prev.tx) / elapsedSec : 0
      prevNetRef.current = { rx: rawRx, tx: rawTx, t: now }
      const point: MetricPoint = {
        timestamp: now,
        cpu: data.cpu_percent ?? data.cpu ?? 0,
        memory: data.memory_percent ?? data.memory ?? 0,
        disk_read: data.disk_read_bytes ?? 0,
        disk_write: data.disk_write_bytes ?? 0,
        net_rx: netRxRate,
        net_tx: netTxRate,
      }
      setHistory((prev) => {
        setLoadError(null)
        setRefreshError(null)
        return [...prev.slice(-(MAX_POINTS - 1)), point]
      })
    } catch (err) {
      const msg = formatUserError(err)
      setHistory((prev) => {
        if (prev.length === 0) setLoadError(msg)
        else setRefreshError(msg)
        return prev
      })
    }
  }, [])

  useEffect(() => {
    pausedRef.current = paused
  }, [paused])

  useEffect(() => {
    fetchMetrics()
    const interval = setInterval(fetchMetrics, 1000)
    return () => clearInterval(interval)
  }, [fetchMetrics])

  const latest = history[history.length - 1] || { cpu: 0, memory: 0, disk_read: 0, disk_write: 0, net_rx: 0, net_tx: 0 }
  const cpuData = history.map(h => h.cpu)
  const memData = history.map(h => h.memory)
  const diskReadData = history.map(h => h.disk_read)
  const diskWriteData = history.map(h => h.disk_write)
  const netRxData = history.map(h => h.net_rx)
  const netTxData = history.map(h => h.net_tx)

  const metrics = [
    { label: 'CPU Usage', value: `${latest.cpu.toFixed(1)}%`, data: cpuData, color: 'var(--zf-link)', icon: <Cpu className="w-4 h-4 text-[var(--zf-link)]" />, max: 100 },
    { label: 'Memory Usage', value: `${latest.memory.toFixed(1)}%`, data: memData, color: '#a855f7', icon: <Database className="w-4 h-4 text-purple-400" />, max: 100 },
    { label: 'Disk Read', value: `${fmtB(latest.disk_read)}/s`, data: diskReadData, color: '#06b6d4', icon: <HardDrive className="w-4 h-4 text-cyan-400" /> },
    { label: 'Disk Write', value: `${fmtB(latest.disk_write)}/s`, data: diskWriteData, color: 'var(--zf-warning)', icon: <HardDrive className="w-4 h-4 text-amber-600" /> },
    { label: 'Network RX', value: `${fmtB(latest.net_rx)}/s`, data: netRxData, color: 'var(--zf-success)', icon: <Network className="w-4 h-4 text-emerald-600" /> },
    { label: 'Network TX', value: `${fmtB(latest.net_tx)}/s`, data: netTxData, color: 'var(--zf-danger)', icon: <Network className="w-4 h-4 text-red-600" /> },
  ]

  return (
    <div className="space-y-6">
      <PageHeader
        title="Live Metrics"
        description={`Real-time system performance (1s interval, ${MAX_POINTS}s window)`}
        actions={
        <div className="flex items-center gap-3">
          <button onClick={() => setPaused(!paused)} title={paused ? 'Resume metrics streaming' : 'Pause metrics streaming'} className={`zf-btn zf-btn-sm ${paused ? 'zf-btn-primary' : 'zf-btn-ghost'}`}>
            {paused ? <Play className="w-4 h-4" /> : <Pause className="w-4 h-4" />}
            {paused ? 'Resume' : 'Pause'}
          </button>
          <div className="flex items-center gap-2">
            <span className={`w-2 h-2 rounded-full ${paused ? 'bg-amber-500' : loadError || refreshError ? 'bg-red-500' : 'bg-emerald-500 animate-pulse'}`} />
            <span className="text-xs text-[var(--zf-muted)]">{paused ? 'Paused' : loadError || refreshError ? 'Error' : 'Streaming'}</span>
          </div>
        </div>
        }
      />

      {loadError && (
        <ErrorBanner
          title="Could not load live metrics"
          headline={loadError}
          hints={hintsForError(loadError)}
        />
      )}

      {refreshError && !loadError && (
        <div className="bg-amber-50 rounded-lg border border-amber-200 px-4 py-2 text-xs text-amber-800">
          Refresh failed: {refreshError} — showing last known data
        </div>
      )}

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {metrics.map(m => (
          <div key={m.label} className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-4 card-glow transition-all hover:scale-[1.01]">
            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center gap-2">
                {m.icon}
                <span className="text-xs text-[var(--zf-muted)]">{m.label}</span>
              </div>
              <span className="text-lg font-bold text-[var(--zf-ink)] tabular-nums">{m.value}</span>
            </div>
            <div className="bg-[var(--zf-canvas)] rounded-lg p-2">
              <MiniChart data={m.data} color={m.color} max={m.max} />
            </div>
            <div className="flex justify-between mt-1.5 text-xs text-[var(--zf-muted)]">
              <span>{MAX_POINTS}s ago</span>
              <span>now</span>
            </div>
          </div>
        ))}
      </div>

      {history.length === 0 && !loadError && (
        <div className="bg-[var(--zf-surface)] rounded-xl p-10 border border-[var(--zf-hairline)] text-center text-[var(--zf-muted)] text-sm">Waiting for metrics data...</div>
      )}
    </div>
  )
}
