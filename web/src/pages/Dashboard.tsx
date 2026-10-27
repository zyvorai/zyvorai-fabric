// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState, useCallback } from 'react'
import { listVMs, getMetrics, VM, VMMetrics } from '../api/vm'
import { Activity, Server, Cpu, HardDrive, TrendingUp, ArrowUpRight, Power, Plus } from 'lucide-react'
import { AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts'
import { useWebSocketContext } from '../contexts/WebSocketContext'
import { useToastContext } from '../contexts/ToastContext'
import { SkeletonDashboard } from '../components/Skeleton'
import { StatusBadge } from '../components/ui'
import { Link } from 'react-router'
import ErrorBanner from '../components/ErrorBanner'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { GettingStarted } from '../components/GettingStarted'
import { FabricGraphic } from '../components/FabricGraphic'
import { RadialGauge } from '../components/RadialGauge'
import { useCountUp } from '../hooks/useCountUp'
import { usePlatformInfo } from '../contexts/PlatformInfoContext'
import type { SubsystemPhase } from '../api/capabilities'

interface MetricsPoint { time: string; cpu: number; memory: number }

function subsystemPhaseStyle(phase: SubsystemPhase | undefined): {
  label: string
  className: string
  pill: string
} {
  switch (phase) {
    case 'live':
      return {
        label: 'Live',
        className: 'text-[var(--zf-ink)]',
        pill: 'text-emerald-700 bg-emerald-50 border-emerald-200',
      }
    case 'unreachable':
      return {
        label: 'Unreachable',
        className: 'text-[var(--zf-ink)]',
        pill: 'text-amber-800 bg-amber-50 border-amber-200',
      }
    case 'off':
      return {
        label: 'Off',
        className: 'text-[var(--zf-ink)]',
        pill: 'bg-[var(--zf-canvas)] text-[var(--zf-muted)] border-[var(--zf-hairline)]',
      }
    default:
      return {
        label: 'Checking…',
        className: 'text-[var(--zf-muted)]',
        pill: 'bg-[var(--zf-canvas)] text-[var(--zf-muted)] border-[var(--zf-hairline)]',
      }
  }
}

const CHART_TOOLTIP_STYLE = {
  backgroundColor: 'var(--zf-surface)',
  border: '1px solid var(--zf-hairline)',
  borderRadius: '12px',
  fontSize: '12px',
  color: 'var(--zf-ink)',
}

export default function Dashboard() {
  const [vms, setVMs] = useState<VM[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [refreshError, setRefreshError] = useState<string | null>(null)
  const [metricsHistory, setMetricsHistory] = useState<MetricsPoint[]>([])
  const { subscribe, vmUpdates } = useWebSocketContext()
  const toast = useToastContext()
  const { capabilities, loading: capsLoading } = usePlatformInfo()

  const loadVMs = useCallback(async () => {
    try {
      setVMs(await listVMs())
      setLoadError(null)
      setRefreshError(null)
    } catch (err) {
      const msg = formatUserError(err)
      setVMs((prev) => {
        if (prev.length === 0) {
          setLoadError(msg)
          toastFailure(toast, 'Failed to load virtual machines', err)
        } else {
          setRefreshError(msg)
        }
        return prev
      })
    } finally {
      setLoading(false)
    }
  }, [toast])

  const loadMetrics = useCallback(async () => {
    try {
      const currentVMs = await listVMs()
      const running = currentVMs.filter((vm) => vm.state === 'running')
      if (running.length === 0) {
        setMetricsHistory((prev) => [...prev.slice(-29), { time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' }), cpu: 0, memory: 0 }])
        return
      }
      const results = await Promise.allSettled(running.map((vm) => getMetrics(vm.name).then((m) => ({ vm, m }))))
      const metrics = results
        .filter((r): r is PromiseFulfilledResult<{ vm: VM; m: VMMetrics }> => r.status === 'fulfilled')
        .map((r) => r.value)
      const avgCpu = metrics.length > 0 ? metrics.reduce((s, { m }) => s + m.cpu_usage, 0) / metrics.length : 0
      // memory_usage is raw bytes, not a percentage (unlike cpu_usage) — express it
      // as a percentage of each VM's own allocated memory (`vm.memory`, in MiB) so
      // it's on the same 0-100 scale as the CPU gauge next to it.
      const memPercents = metrics
        .filter(({ vm }) => vm.memory > 0)
        .map(({ vm, m }) => (m.memory_usage / (vm.memory * 1024 * 1024)) * 100)
      const avgMem = memPercents.length > 0 ? memPercents.reduce((s, p) => s + p, 0) / memPercents.length : 0
      setMetricsHistory((prev) => [...prev.slice(-29), { time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' }), cpu: parseFloat(avgCpu.toFixed(1)), memory: parseFloat(avgMem.toFixed(1)) }])
    } catch (err) { toastFailure(toast, 'Failed to load metrics', err) }
  }, [toast])

  useEffect(() => { loadVMs(); loadMetrics(); const i = setInterval(loadMetrics, 10000); return () => clearInterval(i) }, [loadVMs, loadMetrics])
  useEffect(() => { const unsub = subscribe((msg) => { if (['vm_state_changed', 'vm_created', 'vm_deleted'].includes(msg.type)) loadVMs() }); return unsub }, [subscribe, loadVMs])
  useEffect(() => { if (vmUpdates.size > 0) setVMs((prev) => prev.map((vm) => { const u = vmUpdates.get(vm.name); return u ? { ...vm, ...u } : vm })) }, [vmUpdates])

  const stats = {
    total: vms.length,
    running: vms.filter((v) => v.state === 'running').length,
    stopped: vms.filter((v) => v.state === 'stopped').length,
    paused: vms.filter((v) => v.state === 'paused').length,
    totalCPU: vms.reduce((a, v) => a + v.cpus, 0),
    totalMem: vms.reduce((a, v) => a + v.memory, 0),
  }
  const fleetHealthPct = stats.total > 0 ? (stats.running / stats.total) * 100 : 0

  const totalCount = useCountUp(stats.total)
  const runningCount = useCountUp(stats.running)
  const stoppedCount = useCountUp(stats.stopped)
  const memCount = useCountUp(Math.round(stats.totalMem >= 1024 ? stats.totalMem / 1024 : stats.totalMem))

  if (loading) return <SkeletonDashboard />

  const latestCpu = metricsHistory.length > 0 ? metricsHistory[metricsHistory.length - 1].cpu : 0
  const latestMem = metricsHistory.length > 0 ? metricsHistory[metricsHistory.length - 1].memory : 0
  const greeting = (() => {
    const h = new Date().getHours()
    return h < 5 ? 'Good night' : h < 12 ? 'Good morning' : h < 18 ? 'Good afternoon' : 'Good evening'
  })()

  return (
    <div className="space-y-6">
      {/* Hero */}
      <div className="glass-panel relative overflow-hidden px-6 py-6 sm:px-8 sm:py-7">
        <div className="pointer-events-none absolute -right-6 -top-10 w-72 h-72 opacity-70">
          <FabricGraphic ambient />
        </div>
        <div className="relative flex flex-col sm:flex-row sm:items-center gap-6">
          <div className="flex-1 min-w-0">
            <p className="text-xs font-semibold text-[var(--zf-link)] uppercase tracking-[0.04em] mb-1">{greeting}</p>
            <h1 className="text-[32px] sm:text-[40px] font-semibold text-[var(--zf-ink)] tracking-[-0.022em] leading-none">
              Zyvor Fabric
            </h1>
            <p className="text-[17px] text-[var(--zf-secondary)] mt-2 max-w-md tracking-[-0.022em] leading-snug">
              {stats.total === 0
                ? 'Your infrastructure control plane is ready — spin up the first VM to get going.'
                : `Watching ${stats.total} VM${stats.total === 1 ? '' : 's'} across your fabric. ${stats.running} running right now.`}
            </p>
            <div className="flex items-center gap-3 mt-4">
              <Link to="/app/create" className="zf-btn zf-btn-primary">
                <Plus className="w-4 h-4" /> Create VM
              </Link>
              <button onClick={loadVMs} className="zf-btn zf-btn-ghost">
                Refresh
              </button>
            </div>
          </div>
          {stats.total > 0 && (
            <div className="shrink-0 flex items-center gap-3 self-center">
              <RadialGauge
                percent={fleetHealthPct}
                color={fleetHealthPct >= 70 ? 'var(--zf-success)' : fleetHealthPct >= 30 ? 'var(--zf-warning)' : 'var(--zf-danger)'}
                label="fleet up"
              />
            </div>
          )}
        </div>
      </div>

      {loadError && (
        <ErrorBanner
          title="Could not load dashboard"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={loadVMs}
        />
      )}
      {!loadError && refreshError && (
        <ErrorBanner
          title="Could not refresh dashboard"
          headline={refreshError}
          onRetry={loadVMs}
          tone="amber"
        />
      )}

      <div className="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-6 gap-3">
        {(
          [
            {
              title: 'VM driver',
              subtitle: 'VM registration and lifecycle',
              status: capabilities?.vm_driver,
            },
            {
              title: 'Storage',
              subtitle: 'Pools and disk images',
              status: capabilities?.storage,
            },
            {
              title: 'Network security',
              subtitle: 'Firewall, DNS, VPN policies',
              status: capabilities?.network_security,
            },
            {
              title: 'VM dataplane',
              subtitle: 'FluxVM eBPF edge (Network Fabric)',
              status: capabilities?.vm_dataplane,
            },
            {
              title: 'Authentication',
              subtitle: 'JWT and user database',
              status: capabilities?.auth,
            },
            {
              title: 'Events',
              subtitle: 'Live SSE broadcast',
              status: capabilities?.events,
            },
          ] as const
        ).map(({ title, subtitle, status }) => {
          const phase = subsystemPhaseStyle(status?.phase)
          return (
            <div
              key={title}
              className="rounded-[12px] border border-[var(--zf-hairline)] bg-[var(--zf-canvas)] p-4"
            >
              <p className="text-xs font-semibold uppercase tracking-[0.06em] text-[var(--zf-link)]">
                {title}
              </p>
              <p className="mt-2">
                <span
                  className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-semibold border ${phase.pill}`}
                >
                  {capsLoading && !capabilities ? '…' : phase.label}
                </span>
              </p>
              <p className="text-[14px] text-[var(--zf-secondary)] mt-2 leading-snug tracking-[-0.016em]">
                {status?.detail || subtitle}
              </p>
            </div>
          )
        })}
      </div>

      {/* Stat Cards - matching hypersdk exactly */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Total VMs */}
        <div className="stat-card-blue rounded-xl border border-[var(--zf-hairline)] p-5 transition-all hover:scale-[1.02]">
          <div className="flex items-center justify-between mb-3">
            <div className="icon-tile icon-tile-md">
              <Server className="h-5 w-5 text-[var(--zf-ink)]" />
            </div>
            <span className="text-xs font-medium px-2 py-0.5 rounded-full bg-[var(--zf-canvas)] text-[var(--zf-muted)]">total</span>
          </div>
          <div className="text-2xl font-bold text-[var(--zf-ink)] tabular-nums">{totalCount}</div>
          <div className="text-[13px] text-[var(--zf-secondary)] mt-1">Total VMs</div>
        </div>

        {/* Running */}
        <div className="stat-card-green rounded-xl border border-[var(--zf-hairline)] p-5 transition-all hover:scale-[1.02]">
          <div className="flex items-center justify-between mb-3">
            <div className="icon-tile icon-tile-md">
              <Activity className="h-5 w-5 text-[var(--zf-ink)]" />
            </div>
            <span className="text-xs font-medium px-2 py-0.5 rounded-full bg-emerald-50 text-emerald-700">
              {stats.total > 0 ? `${Math.round((stats.running / stats.total) * 100)}%` : '0%'}
            </span>
          </div>
          <div className="text-2xl font-bold text-[var(--zf-ink)] tabular-nums">{runningCount}</div>
          <div className="text-[13px] text-[var(--zf-secondary)] mt-1">Running</div>
        </div>

        {/* Stopped */}
        <div className="stat-card-red rounded-xl border border-[var(--zf-hairline)] p-5 transition-all hover:scale-[1.02]">
          <div className="flex items-center justify-between mb-3">
            <div className="icon-tile icon-tile-md">
              <Power className="h-5 w-5 text-[var(--zf-ink)]" />
            </div>
            <span className="text-xs font-medium px-2 py-0.5 rounded-full bg-red-50 text-red-700">stopped</span>
          </div>
          <div className="text-2xl font-bold text-[var(--zf-ink)] tabular-nums">{stoppedCount}</div>
          <div className="text-[13px] text-[var(--zf-secondary)] mt-1">Stopped</div>
        </div>

        {/* Resources */}
        <div className="stat-card-purple rounded-xl border border-[var(--zf-hairline)] p-5 transition-all hover:scale-[1.02]">
          <div className="flex items-center justify-between mb-3">
            <div className="icon-tile icon-tile-md">
              <Cpu className="h-5 w-5 text-[var(--zf-ink)]" />
            </div>
            <span className="text-xs font-medium px-2 py-0.5 rounded-full bg-[var(--zf-canvas)] text-[var(--zf-muted)]">
              {stats.totalCPU} vCPU
            </span>
          </div>
          <div className="text-2xl font-bold text-[var(--zf-ink)] tabular-nums">
            {memCount}
            <span className="text-sm text-[var(--zf-muted)] font-medium ml-1">{stats.totalMem >= 1024 ? 'GB' : 'MB'}</span>
          </div>
          <div className="text-[13px] text-[var(--zf-secondary)] mt-1">Total Memory</div>
        </div>
      </div>

      {/* Charts */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* CPU Chart */}
        <div className="bg-[var(--zf-canvas)] rounded-xl p-5 border border-[var(--zf-hairline)]">
          <div className="flex items-center gap-3 mb-4">
            <div className="icon-tile icon-tile-sm">
              <TrendingUp className="w-4 h-4 text-[var(--zf-ink)]" />
            </div>
            <h3 className="text-base font-semibold text-[var(--zf-ink)]">CPU Usage</h3>
            <span className={`ml-auto text-lg font-semibold tabular-nums ${latestCpu > 80 ? 'text-red-600' : latestCpu > 50 ? 'text-amber-600' : 'text-[var(--zf-link)]'}`}>
              {metricsHistory.length > 0 ? `${latestCpu}%` : '--'}
            </span>
          </div>
          <ResponsiveContainer width="100%" height={200}>
            <AreaChart data={metricsHistory}>
              <defs>
                <linearGradient id="gradCpu" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="var(--zf-link)" stopOpacity={0.3} />
                  <stop offset="95%" stopColor="var(--zf-link)" stopOpacity={0} />
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="3 3" stroke="var(--zf-hairline)" />
              <XAxis dataKey="time" stroke="var(--zf-muted)" fontSize={11} tickLine={false} axisLine={false} />
              <YAxis stroke="var(--zf-muted)" fontSize={11} domain={[0, 100]} tickLine={false} axisLine={false} width={30} tickFormatter={(v) => `${v}%`} />
              <Tooltip contentStyle={CHART_TOOLTIP_STYLE} formatter={(value) => [`${Number(value ?? 0)}%`, 'CPU']} />
              <Area type="monotone" dataKey="cpu" stroke="var(--zf-link)" strokeWidth={2} fillOpacity={1} fill="url(#gradCpu)" dot={false} />
            </AreaChart>
          </ResponsiveContainer>
        </div>

        {/* Memory Chart */}
        <div className="bg-[var(--zf-canvas)] rounded-xl p-5 border border-[var(--zf-hairline)]">
          <div className="flex items-center gap-3 mb-4">
            <div className="icon-tile icon-tile-sm">
              <HardDrive className="w-4 h-4 text-[var(--zf-ink)]" />
            </div>
            <h3 className="text-base font-semibold text-[var(--zf-ink)]">Memory Usage</h3>
            <span className={`ml-auto text-lg font-semibold tabular-nums ${latestMem > 80 ? 'text-red-600' : latestMem > 50 ? 'text-amber-600' : 'text-emerald-600'}`}>
              {metricsHistory.length > 0 ? `${latestMem}%` : '--'}
            </span>
          </div>
          <ResponsiveContainer width="100%" height={200}>
            <AreaChart data={metricsHistory}>
              <defs>
                <linearGradient id="gradMem" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="var(--zf-ink)" stopOpacity={0.25} />
                  <stop offset="95%" stopColor="var(--zf-ink)" stopOpacity={0} />
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="3 3" stroke="var(--zf-hairline)" />
              <XAxis dataKey="time" stroke="var(--zf-muted)" fontSize={11} tickLine={false} axisLine={false} />
              <YAxis stroke="var(--zf-muted)" fontSize={11} domain={[0, 100]} tickLine={false} axisLine={false} width={30} tickFormatter={(v) => `${v}%`} />
              <Tooltip contentStyle={CHART_TOOLTIP_STYLE} formatter={(value) => [`${Number(value ?? 0)}%`, 'Memory']} />
              <Area type="monotone" dataKey="memory" stroke="var(--zf-ink)" strokeWidth={2} fillOpacity={1} fill="url(#gradMem)" dot={false} />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* VM Table / first-run onboarding */}
      {vms.length === 0 ? (
        <GettingStarted />
      ) : (
      <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
        <div className="px-5 py-4 border-b border-[var(--zf-hairline)] flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="icon-tile icon-tile-sm">
              <Server className="w-4 h-4 text-[var(--zf-ink)]" />
            </div>
            <h2 className="text-base font-semibold text-[var(--zf-ink)]">Virtual Machines</h2>
          </div>
          <div className="flex items-center gap-3">
            <span className="text-xs font-medium text-[var(--zf-muted)] bg-[var(--zf-canvas)] px-2.5 py-1 rounded-full">{vms.length} VMs</span>
            <Link to="/app/vms" className="flex items-center gap-1 text-xs text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] transition-colors">
              View all <ArrowUpRight className="w-3.5 h-3.5" />
            </Link>
          </div>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-[var(--zf-hairline)]">
                <th className="text-left px-5 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Name</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Status</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">CPU</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Memory</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">IP</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]/30">
              {vms.slice(0, 8).map((vm) => (
                <tr key={vm.name} className="table-row-hover transition-colors">
                  <td className="px-5 py-3">
                    <Link to={`/app/vms/${vm.name}`} className="font-medium text-[var(--zf-ink)] hover:text-[var(--zf-link)] transition-colors">{vm.name}</Link>
                  </td>
                  <td className="px-4 py-3"><StatusBadge status={vm.state} /></td>
                  <td className="px-4 py-3 text-[var(--zf-muted)] tabular-nums">{vm.cpus} vCPU</td>
                  <td className="px-4 py-3 text-[var(--zf-muted)] tabular-nums">{vm.memory >= 1024 ? `${(vm.memory / 1024).toFixed(1)} GB` : `${vm.memory} MB`}</td>
                  <td className="px-4 py-3 text-[var(--zf-muted)] font-mono text-xs">{vm.ip || '-'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
      )}
    </div>
  )
}
