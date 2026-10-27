// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PanelLoading } from '../components/Skeleton'
import { useEffect, useState, useCallback, useRef, useMemo } from 'react'
import { useParams, useNavigate, Link } from 'react-router'
import { getVM, getMetrics, deleteVM, addPortForward, removePortForward, getVMLogs, VM, VMMetrics, VMLogEntry } from '../api/vm'
import { listSnapshots, createSnapshotWithRetry, deleteSnapshot, revertSnapshot, VMSnapshot } from '../api/snapshots'
import { listAuditLogs, AuditLog } from '../api/audit'
import {
  Play, Square, RotateCw, Trash2, Info, Activity, HardDrive,
  Network, Camera, Terminal, Cpu, MemoryStick, Pause, Copy, Wifi,
  AlertCircle, Loader2, RefreshCw, Plus, Plug, Usb, Cloud, Settings, Wrench, Shield,
} from 'lucide-react'
import { AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts'
import { useToastContext } from '../contexts/ToastContext'
import { useVMActions } from '../hooks/useVMActions'
import { StatusBadge } from '../components/ui'
import ConfirmDialog from '../components/ConfirmDialog'
import CloneVMDialog from '../components/CloneVMDialog'
import ErrorBanner from '../components/ErrorBanner'
import CopyButton from '../components/CopyButton'
import UndoBar from '../components/UndoBar'
import { useUndoableAction } from '../hooks/useUndoableAction'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { usePermissions } from '../hooks/usePermissions'
import ReadOnlyNotice from '../components/ReadOnlyNotice'
import HotplugTab from './vm-details/HotplugTab'
import DevicesTab from './vm-details/DevicesTab'
import CloudInitTab from './vm-details/CloudInitTab'
import AdvancedTab from './vm-details/AdvancedTab'
import RescueTab from './vm-details/RescueTab'
import DataplanePanel from './vm-details/DataplanePanel'
import { AnsiText } from '../components/AnsiText'
import { AppleTerminalFrame } from '../components/AppleTerminalFrame'
import { isSpinnerNoise } from '../utils/ansi'

type Tab = 'overview' | 'metrics' | 'disks' | 'network' | 'dataplane' | 'snapshots' | 'logs' | 'hotplug' | 'devices' | 'cloudinit' | 'advanced' | 'rescue'

export default function VMDetails() {
  const { name } = useParams<{ name: string }>()
  const navigate = useNavigate()
  const toast = useToastContext()
  const { canWrite } = usePermissions()
  const [vm, setVM] = useState<VM | null>(null)
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<Tab>('overview')
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false)
  const [showCloneDialog, setShowCloneDialog] = useState(false)

  useEffect(() => {
    if (name) {
      loadVM()
      const interval = setInterval(loadVM, 5000)
      return () => clearInterval(interval)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [name])

  const loadVM = async () => {
    if (!name) return
    setLoadError(null)
    try {
      const data = await getVM(name)
      setVM(data)
    } catch (error) {
      const msg = formatUserError(error)
      setLoadError(msg)
      setVM((prev) => {
        if (prev == null) toastFailure(toast, `Failed to load VM '${name}'`, error)
        return prev
      })
    } finally {
      setLoading(false)
    }
  }

  const { handleStart, handleStop, handleRestart, handlePause, handleResume } = useVMActions(name ?? '', loadVM)
  const { pending: pendingDelete, run: runDelete, undo: undoDelete } = useUndoableAction(8)

  const confirmDelete = useCallback(() => {
    if (!name) return
    setShowDeleteConfirm(false)
    runDelete(`Deleting VM '${name}'…`, async () => {
      try {
        await deleteVM(name)
        toast.success(`VM '${name}' deleted successfully`)
        navigate('/app/vms')
      } catch (error) {
        toastFailure(toast, `Failed to delete VM '${name}'`, error)
      }
    })
  }, [name, toast, navigate, runDelete])

  if (loading) {
    return (
      <div className="space-y-6 animate-pulse">
        <div className="h-5 w-24 bg-[var(--zf-surface)] rounded" />
        <div className="flex items-center justify-between">
          <div className="space-y-2">
            <div className="h-8 w-48 bg-[var(--zf-surface)] rounded" />
            <div className="h-4 w-32 bg-[var(--zf-surface)] rounded" />
          </div>
          <div className="flex gap-2">
            <div className="h-9 w-20 bg-[var(--zf-surface)] rounded-lg" />
            <div className="h-9 w-20 bg-[var(--zf-surface)] rounded-lg" />
          </div>
        </div>
        <div className="h-10 bg-[var(--zf-surface)] rounded" />
        <div className="grid grid-cols-2 gap-4">
          <div className="h-48 bg-[var(--zf-surface)] rounded-xl" />
          <div className="h-48 bg-[var(--zf-surface)] rounded-xl" />
        </div>
      </div>
    )
  }

  if (!vm) {
    return (
      <div className="space-y-4">
        {loadError && (
          <ErrorBanner
            title="Could not load VM"
            headline={loadError}
            hints={hintsForError(loadError, 'vm')}
            onRetry={loadVM}
            tone="red"
          />
        )}
        <div className="text-center py-16">
          <div className="text-[var(--zf-muted)] text-6xl font-bold mb-3">?</div>
          <p className="text-[var(--zf-muted)] mb-4">{loadError ? 'VM unavailable' : 'VM not found'}</p>
          <Link to="/app/vms" className="text-sm text-[var(--zf-link)] hover:text-[var(--zf-link-hover)]">
            Back to Virtual Machines
          </Link>
        </div>
      </div>
    )
  }

  const tabs: { id: Tab; label: string; icon: typeof Info }[] = [
    { id: 'overview', label: 'Overview', icon: Info },
    { id: 'metrics', label: 'Metrics', icon: Activity },
    { id: 'disks', label: 'Disks', icon: HardDrive },
    { id: 'network', label: 'Network', icon: Network },
    { id: 'dataplane', label: 'Dataplane', icon: Shield },
    { id: 'snapshots', label: 'Snapshots', icon: Camera },
    { id: 'hotplug', label: 'Hotplug', icon: Plug },
    { id: 'devices', label: 'Devices', icon: Usb },
    { id: 'cloudinit', label: 'Cloud-init', icon: Cloud },
    { id: 'rescue', label: 'Rescue', icon: Wrench },
    { id: 'advanced', label: 'Advanced', icon: Settings },
    { id: 'logs', label: 'Logs', icon: Terminal },
  ]

  return (
    <div className="space-y-6 min-w-0 max-w-full">
      {!canWrite && <ReadOnlyNotice />}

      {/* Header */}
      <div className="flex items-start justify-between gap-4 min-w-0 flex-wrap">
        <div>
          <div className="flex items-center gap-3 mb-1 flex-wrap">
            <h1 className="text-[26px] sm:text-[32px] leading-tight font-semibold tracking-[-0.022em] text-[var(--zf-ink)]">{vm.name}</h1>
            <StatusBadge status={vm.state} />
            <CopyButton text={vm.name} label="Copy name" successMessage="VM name copied" />
            <CopyButton
              text={`/api/v1/vms/${vm.name}`}
              label="API path"
              successMessage="API path copied"
            />
          </div>
          <div className="flex items-center gap-3 text-sm text-[var(--zf-muted)]">
            <span className="flex items-center gap-1.5">
              <Cpu className="w-3.5 h-3.5" />
              {vm.cpus} vCPU{vm.cpus !== 1 ? 's' : ''}
            </span>
            <span className="flex items-center gap-1.5">
              <MemoryStick className="w-3.5 h-3.5" />
              {vm.memory >= 1024 ? `${(vm.memory / 1024).toFixed(1)} GB` : `${vm.memory} MB`}
            </span>
            {vm.ip && (
              <span className="flex items-center gap-1.5 font-mono text-xs">
                <Wifi className="w-3.5 h-3.5" />
                {vm.ip}
              </span>
            )}
          </div>
        </div>

        <div className="flex items-center gap-2">
          {canWrite && (
            <>
              {vm.state === 'stopped' || vm.state === 'failed' ? (
                <ActionBtn onClick={handleStart} color="green" icon={Play} label="Start" />
              ) : vm.state === 'paused' ? (
                <ActionBtn onClick={handleResume} color="green" icon={Play} label="Resume" />
              ) : (
                <>
                  <ActionBtn onClick={handleStop} color="red" icon={Square} label="Stop" />
                  <ActionBtn onClick={handlePause} color="yellow" icon={Pause} label="Pause" />
                  <ActionBtn onClick={handleRestart} color="blue" icon={RotateCw} label="Restart" />
                </>
              )}
              <div className="w-px h-6 bg-[var(--zf-hairline)] mx-1" />
              <ActionBtn onClick={() => setShowCloneDialog(true)} color="purple" icon={Copy} label="Clone" />
            </>
          )}
          <Link
            to={`/app/vms/${vm.name}/console`}
            className="zf-btn zf-btn-ghost zf-btn-sm"
          >
            <Terminal className="w-3.5 h-3.5" />
            Console
          </Link>
          {canWrite && (
            <button
              onClick={() => setShowDeleteConfirm(true)}
              className="p-1.5 rounded-lg text-[var(--zf-muted)] hover:text-[var(--zf-danger)] hover:bg-red-50 transition-colors"
              title="Delete VM"
            >
              <Trash2 className="w-4 h-4" />
            </button>
          )}
        </div>
      </div>

      {/* Tabs — auto-fill grid wraps into rows; never forces horizontal page scroll */}
      <div className="border-b border-[var(--zf-hairline)] min-w-0 max-w-full">
        <nav
          className="grid w-full min-w-0 gap-x-0.5 gap-y-0 -mb-px"
          style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(7.25rem, 1fr))' }}
          aria-label="VM sections"
        >
          {tabs.map((tab) => {
            const Icon = tab.icon
            const isActive = activeTab === tab.id
            return (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveTab(tab.id)}
                className={`flex items-center justify-center gap-1.5 px-2 py-2 text-xs font-medium rounded-t-lg transition-colors relative whitespace-nowrap ${
                  isActive
                    ? 'text-[var(--zf-link)]'
                    : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
                }`}
              >
                <Icon className="w-3.5 h-3.5 shrink-0" />
                {tab.label}
                {isActive && (
                  <div className="absolute bottom-0 left-1 right-1 h-0.5 bg-[var(--zf-link-fill)] rounded-full" />
                )}
              </button>
            )
          })}
        </nav>
      </div>

      {/* Tab Content */}
      <div className="animate-fade-in">
        {activeTab === 'overview' && <OverviewTab vm={vm} onRetryStart={handleStart} />}
        {activeTab === 'metrics' && <MetricsTab vm={vm} />}
        {activeTab === 'disks' && <DisksTab vm={vm} />}
        {activeTab === 'network' && <NetworkTab vm={vm} onUpdated={loadVM} onOpenDataplane={() => setActiveTab('dataplane')} />}
        {activeTab === 'dataplane' && <DataplanePanel vmName={vm.name} />}
        {activeTab === 'snapshots' && <SnapshotsTab vm={vm} />}
        {activeTab === 'hotplug' && <HotplugTab vm={vm} />}
        {activeTab === 'devices' && <DevicesTab vm={vm} />}
        {activeTab === 'cloudinit' && <CloudInitTab vm={vm} />}
        {activeTab === 'rescue' && <RescueTab vm={vm} />}
        {activeTab === 'advanced' && <AdvancedTab vm={vm} />}
        {activeTab === 'logs' && <LogsTab vm={vm} />}
      </div>

      {showDeleteConfirm && (
        <ConfirmDialog
          title="Delete Virtual Machine"
          message={`Are you sure you want to delete VM '${name}'? You'll have a few seconds to undo before it's gone for good.`}
          confirmLabel="Delete"
          variant="danger"
          onConfirm={confirmDelete}
          onCancel={() => setShowDeleteConfirm(false)}
        />
      )}
      <UndoBar pending={pendingDelete} onUndo={undoDelete} />
      {showCloneDialog && name && (
        <CloneVMDialog
          vmName={name}
          onClose={() => setShowCloneDialog(false)}
          onSuccess={loadVM}
        />
      )}
    </div>
  )
}

function ActionBtn({ onClick, color, icon: Icon, label }: {
  onClick: () => void
  color: string
  icon: typeof Play
  label: string
}) {
  const colors: Record<string, string> = {
    green: 'bg-emerald-50 text-emerald-700 hover:bg-emerald-100',
    red: 'bg-red-50 text-[var(--zf-danger)] hover:bg-red-100',
    yellow: 'bg-amber-50 text-amber-800 hover:bg-amber-100',
    blue: 'bg-[var(--zf-link)]/15 text-[var(--zf-link)] hover:bg-[var(--zf-link)]/25',
    purple: 'bg-black/[0.04] text-[var(--zf-ink)] hover:bg-black/[0.06]',
  }

  return (
    <button
      onClick={onClick}
      className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium transition-colors ${colors[color]}`}
    >
      <Icon className="w-3.5 h-3.5" />
      {label}
    </button>
  )
}

function InfoRow({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-center justify-between py-2.5 border-b border-[var(--zf-hairline)]/50 last:border-b-0">
      <dt className="text-sm text-[var(--zf-muted)]">{label}</dt>
      <dd className={`text-sm text-[var(--zf-ink)] ${mono ? 'font-mono text-xs' : ''}`}>{value}</dd>
    </div>
  )
}

function OverviewTab({ vm, onRetryStart }: { vm: VM; onRetryStart: () => void }) {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      {vm.state === 'failed' && vm.last_error && (
        <div className="md:col-span-2">
          <ErrorBanner
            title="This VM failed to start"
            headline={vm.last_error}
            tone="red"
            onRetry={onRetryStart}
            retryLabel="Try starting again"
          />
        </div>
      )}
      <div className="zf-panel p-5">
        <h3 className="text-sm font-medium text-[var(--zf-muted)] mb-3">Configuration</h3>
        <dl>
          <InfoRow label="Name" value={vm.name} />
          <InfoRow label="State" value={vm.state} />
          <InfoRow label="Image" value={vm.image} mono />
          {vm.ip && <InfoRow label="IP Address" value={vm.ip} mono />}
          {vm.pid && <InfoRow label="PID" value={String(vm.pid)} mono />}
        </dl>
      </div>

      <div className="zf-panel p-5">
        <h3 className="text-sm font-medium text-[var(--zf-muted)] mb-3">Resources</h3>
        <dl>
          <InfoRow label="vCPUs" value={`${vm.cpus}`} />
          <InfoRow label="Memory" value={vm.memory >= 1024 ? `${(vm.memory / 1024).toFixed(1)} GB` : `${vm.memory} MB`} />
        </dl>

        {vm.tags && vm.tags.length > 0 && (
          <div className="mt-4 pt-3 border-t border-[var(--zf-hairline)]">
            <span className="text-sm text-[var(--zf-muted)] block mb-2">Tags</span>
            <div className="flex flex-wrap gap-1.5">
              {vm.tags.map((tag) => (
                <span key={tag} className="px-2 py-0.5 rounded text-xs font-medium bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] text-[var(--zf-muted)]">
                  {tag}
                </span>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

interface MetricsPoint { time: string; cpu: number; memory: number; disk_read: number; disk_write: number; net_rx: number; net_tx: number }

function MetricsTab({ vm }: { vm: VM }) {
  const [history, setHistory] = useState<MetricsPoint[]>([])
  const [latest, setLatest] = useState<VMMetrics | null>(null)

  useEffect(() => {
    if (vm.state !== 'running') return
    const load = async () => {
      try {
        const m = await getMetrics(vm.name)
        setLatest(m)
        // memory_usage is raw bytes, not a percentage (unlike cpu_usage) —
        // express it as a percentage of this VM's own allocated memory
        // (vm.memory, in MiB) so it's on the same 0-100 chart scale as CPU.
        const memoryPct = vm.memory > 0 ? (m.memory_usage / (vm.memory * 1024 * 1024)) * 100 : 0
        setHistory((prev) => [...prev.slice(-29), {
          time: new Date().toLocaleTimeString(),
          cpu: parseFloat(m.cpu_usage.toFixed(1)),
          memory: parseFloat(memoryPct.toFixed(1)),
          disk_read: m.disk_usage,
          disk_write: m.disk_usage,
          net_rx: m.network_rx,
          net_tx: m.network_tx,
        }])
      } catch { /* running VM may not have metrics yet */ }
    }
    load()
    const interval = setInterval(load, 5000)
    return () => clearInterval(interval)
  }, [vm.name, vm.state])

  if (vm.state !== 'running') {
    return (
      <div className="zf-panel p-8 text-center">
        <Activity className="w-10 h-10 text-[var(--zf-muted)] mx-auto mb-3" />
        <p className="text-[var(--zf-muted)] text-sm">Metrics are only available for running VMs</p>
      </div>
    )
  }

  const tooltipStyle = {
    backgroundColor: 'var(--zf-surface)',
    border: '1px solid var(--zf-hairline)',
    borderRadius: '0.5rem',
    fontSize: '12px',
    color: 'var(--zf-ink)',
  }

  return (
    <div className="space-y-4">
      {/* Quick stats */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <MetricStat label="CPU" value={latest ? `${latest.cpu_usage.toFixed(1)}%` : '--'} color="blue" />
        <MetricStat label="Memory" value={latest && vm.memory > 0 ? `${((latest.memory_usage / (vm.memory * 1024 * 1024)) * 100).toFixed(1)}%` : '--'} color="emerald" />
        <MetricStat label="Net RX" value={latest ? `${(latest.network_rx / 1024).toFixed(1)} KB/s` : '--'} color="purple" />
        <MetricStat label="Net TX" value={latest ? `${(latest.network_tx / 1024).toFixed(1)} KB/s` : '--'} color="orange" />
      </div>

      {/* Charts */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <div className="zf-panel p-5">
          <h3 className="text-sm font-medium text-[var(--zf-ink)] mb-3">CPU Usage</h3>
          <ResponsiveContainer width="100%" height={180}>
            <AreaChart data={history} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
              <defs>
                <linearGradient id="cpuGrad" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="var(--zf-link)" stopOpacity={0.3} />
                  <stop offset="100%" stopColor="var(--zf-link)" stopOpacity={0} />
                </linearGradient>
              </defs>
              <CartesianGrid stroke="var(--zf-hairline)" strokeDasharray="3 3" vertical={false} />
              <XAxis
                dataKey="time"
                tick={{ fill: 'var(--zf-muted)', fontSize: 11 }}
                tickLine={false}
                axisLine={false}
              />
              <YAxis
                domain={[0, 100]}
                ticks={[0, 25, 50, 75, 100]}
                tick={{ fill: 'var(--zf-ink)', fontSize: 11, fontWeight: 500 }}
                tickLine={false}
                axisLine={false}
                width={40}
                tickFormatter={(v) => `${v}%`}
              />
              <Tooltip contentStyle={tooltipStyle} formatter={(v) => [`${Number(v ?? 0)}%`, 'CPU']} />
              <Area type="monotone" dataKey="cpu" stroke="var(--zf-link)" strokeWidth={1.5} fill="url(#cpuGrad)" dot={false} />
            </AreaChart>
          </ResponsiveContainer>
        </div>
        <div className="zf-panel p-5">
          <h3 className="text-sm font-medium text-[var(--zf-ink)] mb-3">Memory Usage</h3>
          <ResponsiveContainer width="100%" height={180}>
            <AreaChart data={history} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
              <defs>
                <linearGradient id="memGrad" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="var(--zf-success)" stopOpacity={0.3} />
                  <stop offset="100%" stopColor="var(--zf-success)" stopOpacity={0} />
                </linearGradient>
              </defs>
              <CartesianGrid stroke="var(--zf-hairline)" strokeDasharray="3 3" vertical={false} />
              <XAxis
                dataKey="time"
                tick={{ fill: 'var(--zf-muted)', fontSize: 11 }}
                tickLine={false}
                axisLine={false}
              />
              <YAxis
                domain={[0, 100]}
                ticks={[0, 25, 50, 75, 100]}
                tick={{ fill: 'var(--zf-ink)', fontSize: 11, fontWeight: 500 }}
                tickLine={false}
                axisLine={false}
                width={40}
                tickFormatter={(v) => `${v}%`}
              />
              <Tooltip contentStyle={tooltipStyle} formatter={(v) => [`${Number(v ?? 0)}%`, 'Memory']} />
              <Area type="monotone" dataKey="memory" stroke="var(--zf-success)" strokeWidth={1.5} fill="url(#memGrad)" dot={false} />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      </div>
    </div>
  )
}

function MetricStat({ label, value, color }: { label: string; value: string; color: string }) {
  const textMap: Record<string, string> = {
    blue: 'text-[var(--zf-link)]',
    emerald: 'text-emerald-700',
    purple: 'text-[var(--zf-ink)]',
    orange: 'text-amber-800',
  }
  return (
    <div className="zf-panel px-4 py-3">
      <div className="text-xs text-[var(--zf-muted)] mb-1">{label}</div>
      <div className={`text-xl font-bold tabular-nums ${textMap[color] || 'text-[var(--zf-ink)]'}`}>{value}</div>
    </div>
  )
}

function DisksTab({ vm }: { vm: VM }) {
  const rootImage = vm.image
  const format = rootImage?.endsWith('.raw') ? 'raw' : rootImage?.endsWith('.qcow2') ? 'qcow2' : 'image'

  const disks: { name: string; path: string; size: string; format: string; bus: string }[] = []

  if (rootImage) {
    disks.push({
      name: 'vda',
      path: rootImage,
      size: '--',
      format,
      bus: 'virtio',
    })
  }

  if (disks.length === 0) {
    return (
      <div className="zf-panel p-8 text-center">
        <HardDrive className="w-10 h-10 text-[var(--zf-muted)] mx-auto mb-3" />
        <p className="text-[var(--zf-muted)] text-sm">No disk information available</p>
      </div>
    )
  }

  return (
    <div className="zf-panel overflow-hidden">
      <table className="w-full text-sm">
        <thead>
          <tr className="text-left text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider border-b border-[var(--zf-hairline)]">
            <th className="py-3 px-5">Device</th>
            <th className="py-3 px-4">Path</th>
            <th className="py-3 px-4">Size</th>
            <th className="py-3 px-4">Format</th>
            <th className="py-3 px-4">Bus</th>
          </tr>
        </thead>
        <tbody>
          {disks.map((disk) => (
            <tr key={disk.name} className="border-t border-[var(--zf-hairline)]/50 hover:bg-black/[0.03] transition-colors">
              <td className="py-3 px-5 font-medium text-[var(--zf-ink)]">{disk.name}</td>
              <td className="py-3 px-4 font-mono text-xs text-[var(--zf-muted)] max-w-[300px] truncate">{disk.path}</td>
              <td className="py-3 px-4 text-[var(--zf-muted)]">{disk.size}</td>
              <td className="py-3 px-4">
                <span className="px-2 py-0.5 text-xs font-medium rounded text-[var(--zf-muted)] bg-[var(--zf-canvas)] border border-[var(--zf-hairline)]">
                  {disk.format.toUpperCase()}
                </span>
              </td>
              <td className="py-3 px-4 text-[var(--zf-muted)]">{disk.bus}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function NetworkTab({ vm, onUpdated, onOpenDataplane }: { vm: VM; onUpdated: () => void; onOpenDataplane: () => void }) {
  return <NetworkTabContent vm={vm} onUpdated={onUpdated} onOpenDataplane={onOpenDataplane} />
}

function DataplaneTeaser({ onOpen }: { onOpen: () => void }) {
  return (
    <div className="zf-panel p-4 flex flex-wrap items-center justify-between gap-3">
      <div className="flex items-start gap-2">
        <Shield className="w-4 h-4 text-[var(--zf-link)] mt-0.5" />
        <div>
          <p className="text-sm font-medium text-[var(--zf-ink)]">VM edge dataplane (FluxVM)</p>
          <p className="text-xs text-[var(--zf-muted)] mt-0.5">
            Status, allowlists, Mbps/PPS limits, counters, and sampled flows.
          </p>
        </div>
      </div>
      <button type="button" onClick={onOpen} className="zf-btn zf-btn-primary zf-btn-sm">
        Open Dataplane
      </button>
    </div>
  )
}

function NetworkTabContent({
  vm,
  onUpdated,
  onOpenDataplane,
}: {
  vm: VM
  onUpdated: () => void
  onOpenDataplane: () => void
}) {
  interface NetworkInterface {
    name: string
    mac: string
    ip: string
    model: string
    state: string
  }

  const interfaces: NetworkInterface[] = []

  const ipAddr = vm.ip || ''
  const operState = vm.state === 'running' ? 'up' : 'down'

  if (ipAddr || vm.state === 'running' || vm.network_tap) {
    interfaces.push({
      name: 'eth0',
      mac: '--',
      ip: ipAddr || '--',
      model: 'virtio-net',
      state: operState,
    })
  }

  const interfacesSection = interfaces.length === 0 ? (
    <div className="zf-panel p-8 text-center">
      <Network className="w-10 h-10 text-[var(--zf-muted)] mx-auto mb-3" />
      <p className="text-[var(--zf-muted)] text-sm">No network information available</p>
      <p className="text-[var(--zf-muted)] text-xs mt-2">VM is not running or has no guest IP recorded</p>
    </div>
  ) : (
    <div className="zf-panel overflow-hidden">
      <table className="w-full text-sm">
        <thead>
          <tr className="text-left text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider border-b border-[var(--zf-hairline)]">
            <th className="py-3 px-5">Interface</th>
            <th className="py-3 px-4">MAC Address</th>
            <th className="py-3 px-4">IP Address</th>
            <th className="py-3 px-4">Model</th>
            <th className="py-3 px-4">State</th>
          </tr>
        </thead>
        <tbody>
          {interfaces.map((iface) => (
            <tr key={iface.name} className="border-t border-[var(--zf-hairline)]/50 hover:bg-black/[0.03] transition-colors">
              <td className="py-3 px-5 font-medium text-[var(--zf-ink)]">{iface.name}</td>
              <td className="py-3 px-4 font-mono text-xs text-[var(--zf-muted)]">{iface.mac}</td>
              <td className="py-3 px-4 font-mono text-xs text-[var(--zf-ink)]">{iface.ip}</td>
              <td className="py-3 px-4 text-[var(--zf-muted)]">{iface.model}</td>
              <td className="py-3 px-4">
                <StatusBadge status={iface.state === 'up' ? 'running' : 'stopped'} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )

  return (
    <div className="space-y-6">
      {interfacesSection}
      <PortForwardsSection vm={vm} onUpdated={onUpdated} />
      <DataplaneTeaser onOpen={onOpenDataplane} />
    </div>
  )
}

function PortForwardsSection({ vm, onUpdated }: { vm: VM; onUpdated: () => void }) {
  const toast = useToastContext()
  const { canWrite } = usePermissions()
  const [showForm, setShowForm] = useState(false)
  const [hostPort, setHostPort] = useState('')
  const [guestPort, setGuestPort] = useState('22')
  const [protocol, setProtocol] = useState<'tcp' | 'udp'>('tcp')
  const [submitting, setSubmitting] = useState(false)
  const [formError, setFormError] = useState('')
  const [removingPort, setRemovingPort] = useState<number | null>(null)

  const forwards = vm.port_forwards ?? []

  const handleRemove = async (hostPort: number) => {
    setRemovingPort(hostPort)
    try {
      await removePortForward(vm.name, hostPort)
      toast.success(
        vm.state === 'running'
          ? `Port ${hostPort} no longer exposed — VM restarted to apply it`
          : `Port ${hostPort} will no longer be exposed on next start`,
      )
      onUpdated()
    } catch (err) {
      toastFailure(toast, 'Failed to remove port forward', err)
    } finally {
      setRemovingPort(null)
    }
  }

  const handleAdd = async () => {
    const h = parseInt(hostPort)
    const g = parseInt(guestPort)
    if (!hostPort || !Number.isInteger(h) || h < 1 || h > 65535) {
      setFormError('Host port must be between 1 and 65535')
      return
    }
    if (!guestPort || !Number.isInteger(g) || g < 1 || g > 65535) {
      setFormError('Guest port must be between 1 and 65535')
      return
    }
    setSubmitting(true)
    setFormError('')
    try {
      await addPortForward(vm.name, { hostPort: h, guestPort: g, protocol })
      toast.success(
        vm.state === 'running'
          ? `Port ${h} → ${g}/${protocol} exposed — VM restarted to apply it`
          : `Port ${h} → ${g}/${protocol} will be exposed on next start`,
      )
      setShowForm(false)
      setHostPort('')
      setGuestPort('22')
      onUpdated()
    } catch (err) {
      setFormError(formatUserError(err))
      toastFailure(toast, 'Failed to expose port', err)
    } finally {
      setSubmitting(false)
    }
  }

  if (vm.network_tap) {
    return (
      <div className="zf-panel p-5">
        <h3 className="text-sm font-semibold text-[var(--zf-ink)] mb-1">Bridged Networking</h3>
        <p className="text-xs text-[var(--zf-muted)]">
          {vm.network_static_ip
            ? "This VM's IP was configured statically via cloud-init — see the interface table above once it's booted. No port forwards needed."
            : "This VM has its own real, externally-reachable IP via DHCP — see the interface table above once it's booted and leased an address. No port forwards needed."}
        </p>
      </div>
    )
  }

  return (
    <div className="zf-panel overflow-hidden">
      <div className="p-5 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <div>
          <h3 className="text-sm font-semibold text-[var(--zf-ink)]">Exposed Ports</h3>
          <p className="text-xs text-[var(--zf-muted)] mt-0.5">
            This VM uses NAT networking — forwards here are the only way to reach it (e.g. SSH) from outside the host.
          </p>
        </div>
        {canWrite && !showForm && (
          <button
            onClick={() => setShowForm(true)}
            className="zf-btn zf-btn-ghost zf-btn-sm"
          >
            <Plug className="w-3.5 h-3.5" />
            Expose Port
          </button>
        )}
      </div>

      {showForm && (
        <div className="p-5 border-b border-[var(--zf-hairline)] bg-[var(--zf-canvas)] space-y-3">
          <div className="flex items-center gap-2">
            <input
              type="number"
              value={hostPort}
              onChange={(e) => setHostPort(e.target.value)}
              placeholder="Host port"
              min={1}
              max={65535}
              className="input-field w-28 py-1.5"
            />
            <span className="text-[var(--zf-muted)] text-sm">→</span>
            <input
              type="number"
              value={guestPort}
              onChange={(e) => setGuestPort(e.target.value)}
              placeholder="Guest port"
              min={1}
              max={65535}
              className="input-field w-28 py-1.5"
            />
            <select
              value={protocol}
              onChange={(e) => setProtocol(e.target.value as 'tcp' | 'udp')}
              className="input-field w-auto py-1.5"
            >
              <option value="tcp">TCP</option>
              <option value="udp">UDP</option>
            </select>
          </div>
          {vm.state === 'running' && (
            <p className="text-xs text-amber-700/80">
              This VM is running — adding a forward requires restarting it to apply.
            </p>
          )}
          {formError && <p className="text-[var(--zf-danger)] text-sm">{formError}</p>}
          <div className="flex gap-2">
            <button
              onClick={handleAdd}
              disabled={submitting}
              className="zf-btn zf-btn-primary zf-btn-sm"
            >
              {submitting ? 'Applying…' : 'Add'}
            </button>
            <button
              onClick={() => { setShowForm(false); setFormError('') }}
              className="zf-btn zf-btn-ghost zf-btn-sm"
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {forwards.length === 0 ? (
        <div className="p-8 text-center text-[var(--zf-muted)] text-sm">No ports exposed.</div>
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="text-left text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider border-b border-[var(--zf-hairline)]">
              <th className="py-3 px-5">Host Port</th>
              <th className="py-3 px-4">Guest Port</th>
              <th className="py-3 px-4">Protocol</th>
              <th className="py-3 px-4"></th>
            </tr>
          </thead>
          <tbody>
            {forwards.map((f, i) => (
              <tr key={i} className="border-t border-[var(--zf-hairline)] hover:bg-black/[0.03] transition-colors">
                <td className="py-3 px-5 font-mono text-[var(--zf-ink)]">{f.host_port}</td>
                <td className="py-3 px-4 font-mono text-[var(--zf-ink)]">{f.guest_port}</td>
                <td className="py-3 px-4">
                  <span className="px-2 py-0.5 rounded text-xs font-medium bg-[var(--zf-link)]/10 text-[var(--zf-link)] border border-[var(--zf-link)]/20 uppercase">
                    {f.protocol}
                  </span>
                </td>
                <td className="py-3 px-4 text-right">
                  {canWrite && (
                    <button
                      onClick={() => handleRemove(f.host_port)}
                      disabled={removingPort === f.host_port}
                      title="Remove port forward"
                      className="p-1.5 rounded-md text-[var(--zf-muted)] hover:text-[var(--zf-danger)] hover:bg-red-50 disabled:opacity-50 transition-colors"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  )
}

function SnapshotsTab({ vm }: { vm: VM }) {
  const toast = useToastContext()
  const [snapshots, setSnapshots] = useState<VMSnapshot[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [showCreateForm, setShowCreateForm] = useState(false)
  const [newName, setNewName] = useState('')
  const [newDescription, setNewDescription] = useState('')
  // Default Disk — matches API default and avoids Full (vmstate) dumps that
  // routinely exceed host load / HTTP budgets on lab hardware.
  const [newType, setNewType] = useState<'Disk' | 'Full'>('Disk')
  const [actionInProgress, setActionInProgress] = useState<string | null>(null)

  const loadSnapshots = useCallback(async () => {
    try {
      const data = await listSnapshots(vm.name)
      setSnapshots(data)
      setError(null)
    } catch (err) {
      setError(formatUserError(err))
    } finally {
      setLoading(false)
    }
  }, [vm.name])

  useEffect(() => {
    loadSnapshots()
  }, [loadSnapshots])

  const handleCreate = async () => {
    if (!newName.trim()) return
    setCreating(true)
    try {
      await createSnapshotWithRetry(
        vm.name,
        {
          name: newName.trim(),
          description: newDescription.trim() || undefined,
          snapshot_type: newType,
        },
      )
      toast.success(
        newType === 'Full'
          ? `Full snapshot '${newName}' created`
          : `Snapshot '${newName}' created`,
      )
      setShowCreateForm(false)
      setNewName('')
      setNewDescription('')
      setNewType('Disk')
      await loadSnapshots()
    } catch (err) {
      toastFailure(toast, 'Failed to create snapshot', err)
    } finally {
      setCreating(false)
    }
  }

  const handleDelete = async (snap: VMSnapshot) => {
    setActionInProgress(snap.id)
    try {
      await deleteSnapshot(vm.name, snap.id)
      toast.success(`Snapshot '${snap.name}' deleted`)
      await loadSnapshots()
    } catch (err) {
      toastFailure(toast, 'Failed to delete snapshot', err)
    } finally {
      setActionInProgress(null)
    }
  }

  const handleRevert = async (snap: VMSnapshot) => {
    setActionInProgress(snap.id)
    try {
      await revertSnapshot(vm.name, snap.id)
      toast.success(`Reverted to snapshot '${snap.name}'`)
    } catch (err) {
      toastFailure(toast, 'Failed to revert snapshot', err)
    } finally {
      setActionInProgress(null)
    }
  }

  const formatSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
  }

  if (loading) {
    return (
      <PanelLoading label="Loading snapshots" />
    )
  }

  if (error) {
    return (
      <div className="zf-panel p-8 text-center">
        <AlertCircle className="w-6 h-6 text-[var(--zf-danger)] mx-auto mb-2" />
        <p className="text-[var(--zf-danger)] text-sm mb-3">{error}</p>
        <button
          onClick={() => { setLoading(true); loadSnapshots() }}
          className="zf-btn zf-btn-ghost zf-btn-sm mx-auto"
        >
          <RefreshCw className="w-3.5 h-3.5" />
          Retry
        </button>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <div className="flex justify-between items-center">
        <button
          onClick={() => { setLoading(true); loadSnapshots() }}
          className="flex items-center gap-1.5 px-3 py-1.5 text-[var(--zf-muted)] hover:text-[var(--zf-ink)] text-sm transition-colors"
        >
          <RefreshCw className="w-3.5 h-3.5" />
          Refresh
        </button>
        <button
          onClick={() => setShowCreateForm(true)}
          className="zf-btn zf-btn-primary zf-btn-sm"
        >
          <Plus className="w-3.5 h-3.5" />
          Create Snapshot
        </button>
      </div>

      {showCreateForm && (
        <div className="zf-panel p-5 space-y-3">
          <h3 className="text-sm font-medium text-[var(--zf-ink)]">New Snapshot</h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            <div>
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Name</label>
              <input
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="snapshot-name"
                className="input-field py-1.5"
              />
            </div>
            <div>
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Type</label>
              <select
                value={newType}
                onChange={(e) => setNewType(e.target.value as 'Disk' | 'Full')}
                className="input-field py-1.5"
              >
                <option value="Disk">Disk Only</option>
                <option value="Full">Full (disk + memory — slower)</option>
              </select>
              {newType === 'Full' && (
                <p className="text-xs text-[var(--zf-muted)] mt-1">
                  Full snapshots pause the guest briefly and can take several minutes under host load.
                  Prefer Disk Only for routine checkpoints.
                </p>
              )}
              {creating && (
                <p className="text-xs text-[var(--zf-muted)] mt-1">
                  {newType === 'Full'
                    ? 'Creating full snapshot — this may take a few minutes…'
                    : 'Creating snapshot…'}
                </p>
              )}
            </div>
          </div>
          <div>
            <label className="block text-xs text-[var(--zf-muted)] mb-1">Description (optional)</label>
            <input
              type="text"
              value={newDescription}
              onChange={(e) => setNewDescription(e.target.value)}
              placeholder="Description of this snapshot"
              className="input-field py-1.5"
            />
          </div>
          <div className="flex justify-end gap-2">
            <button
              onClick={() => { setShowCreateForm(false); setNewName(''); setNewDescription('') }}
              className="zf-btn zf-btn-ghost zf-btn-sm"
            >
              Cancel
            </button>
            <button
              onClick={handleCreate}
              disabled={creating || !newName.trim()}
              className="zf-btn zf-btn-primary zf-btn-sm"
            >
              {creating && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
              {creating ? 'Creating...' : 'Create'}
            </button>
          </div>
        </div>
      )}

      {snapshots.length === 0 ? (
        <div className="zf-panel p-8 text-center">
          <Camera className="w-10 h-10 text-[var(--zf-muted)] mx-auto mb-3" />
          <p className="text-[var(--zf-muted)] text-sm">No snapshots found</p>
          <p className="text-[var(--zf-muted)] text-xs mt-1">Create a snapshot to save the current state of this VM</p>
        </div>
      ) : (
        <div className="zf-panel overflow-hidden">
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider border-b border-[var(--zf-hairline)]">
                <th className="py-3 px-5">Name</th>
                <th className="py-3 px-4">Type</th>
                <th className="py-3 px-4">Created</th>
                <th className="py-3 px-4">Size</th>
                <th className="py-3 px-4">Actions</th>
              </tr>
            </thead>
            <tbody>
              {snapshots.map((snap) => (
                <tr key={snap.id} className="border-t border-[var(--zf-hairline)]/50 hover:bg-black/[0.03] transition-colors group">
                  <td className="py-3 px-5">
                    <div className="font-medium text-[var(--zf-ink)]">{snap.name}</div>
                    {snap.description && (
                      <div className="text-xs text-[var(--zf-muted)] mt-0.5">{snap.description}</div>
                    )}
                  </td>
                  <td className="py-3 px-4">
                    <span className="px-2 py-0.5 text-xs font-medium rounded text-[var(--zf-muted)] bg-[var(--zf-canvas)] border border-[var(--zf-hairline)]">
                      {snap.snapshot_type === 'Disk' ? 'disk-only' : 'full'}
                    </span>
                  </td>
                  <td className="py-3 px-4 text-[var(--zf-muted)]">{new Date(snap.created).toLocaleString()}</td>
                  <td className="py-3 px-4 text-[var(--zf-muted)] tabular-nums">{formatSize(snap.size_bytes)}</td>
                  <td className="py-3 px-4">
                    <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                      <button
                        onClick={() => handleRevert(snap)}
                        disabled={actionInProgress === snap.id}
                        className="px-2.5 py-1 bg-[var(--zf-link)]/15 text-[var(--zf-link)] hover:bg-[var(--zf-link)]/25 disabled:opacity-50 rounded text-xs font-medium transition-colors"
                      >
                        {actionInProgress === snap.id ? 'Working...' : 'Restore'}
                      </button>
                      <button
                        onClick={() => handleDelete(snap)}
                        disabled={actionInProgress === snap.id}
                        className="px-2.5 py-1 bg-red-50 text-[var(--zf-danger)] hover:bg-red-100 disabled:opacity-50 rounded text-xs font-medium transition-colors"
                      >
                        Delete
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

const LOG_PRIORITY_DOT: Record<string, string> = {
  emerg: 'bg-red-500', alert: 'bg-red-500', crit: 'bg-red-500', err: 'bg-red-400', error: 'bg-red-400',
  warning: 'bg-amber-400', warn: 'bg-amber-400', notice: 'bg-sky-400', info: 'bg-zinc-500',
  debug: 'bg-zinc-600',
}

function LogsTab({ vm }: { vm: VM }) {
  const [logs, setLogs] = useState<VMLogEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [grep, setGrep] = useState('')
  const [autoRefresh, setAutoRefresh] = useState(vm.state === 'starting' || vm.state === 'running')
  const [showActivity, setShowActivity] = useState(false)
  const [activity, setActivity] = useState<AuditLog[]>([])
  const termRef = useRef<HTMLDivElement>(null)
  const stickBottom = useRef(true)

  const loadLogs = useCallback(async (opts?: { silent?: boolean }) => {
    if (!opts?.silent) setLoading(true)
    try {
      const data = await getVMLogs(vm.name, { lines: 500, grep: grep || undefined })
      setLogs(data.entries)
      setError(null)
    } catch (err) {
      setError(formatUserError(err))
    } finally {
      setLoading(false)
    }
  }, [vm.name, grep])

  useEffect(() => {
    loadLogs()
  }, [loadLogs])

  useEffect(() => {
    if (!autoRefresh) return
    const id = setInterval(() => loadLogs({ silent: true }), 3000)
    return () => clearInterval(id)
  }, [autoRefresh, loadLogs])

  useEffect(() => {
    if (!showActivity) return
    listAuditLogs({ resource_name: vm.name, resource_type: 'vm' }).then(setActivity).catch(() => {})
  }, [showActivity, vm.name])

  const displayLogs = useMemo(
    () => logs.filter((log) => !isSpinnerNoise(log.message)),
    [logs],
  )

  useEffect(() => {
    const el = termRef.current
    if (!el || !stickBottom.current) return
    el.scrollTop = el.scrollHeight
  }, [displayLogs])

  const onTermScroll = () => {
    const el = termRef.current
    if (!el) return
    stickBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 48
  }

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <div className={`w-2 h-2 rounded-full ${
            vm.state === 'running' ? 'bg-emerald-500 animate-pulse'
              : vm.state === 'starting' ? 'bg-amber-500 animate-pulse'
              : vm.state === 'failed' ? 'bg-red-500' : 'bg-[var(--zf-hairline)]'
          }`} />
          <span className="text-sm text-[var(--zf-muted)]">
            {vm.state === 'starting' ? 'Booting — watching console output live' : `VM is ${vm.state}`}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <input
            type="text"
            value={grep}
            onChange={(e) => setGrep(e.target.value)}
            placeholder="Filter…"
            className="input-field py-1.5 w-40"
          />
          <button
            onClick={() => setAutoRefresh((v) => !v)}
            className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm transition-colors ${
              autoRefresh ? 'bg-[var(--zf-link)]/20 text-[var(--zf-link)] border border-[var(--zf-link)]/30' : 'bg-[var(--zf-surface)] border border-[var(--zf-hairline)] text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
            }`}
          >
            <span className={`w-1.5 h-1.5 rounded-full ${autoRefresh ? 'bg-[var(--zf-link-fill)] animate-pulse' : 'bg-[var(--zf-hairline)]'}`} />
            Live
          </button>
          <button
            onClick={() => loadLogs()}
            className="flex items-center gap-1.5 px-3 py-1.5 text-[var(--zf-muted)] hover:text-[var(--zf-ink)] text-sm transition-colors"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
            Refresh
          </button>
        </div>
      </div>

      {error && (
        <div className="zf-panel p-8 text-center">
          <AlertCircle className="w-6 h-6 text-[var(--zf-danger)] mx-auto mb-2" />
          <p className="text-[var(--zf-danger)] text-sm mb-3">{error}</p>
          <button
            onClick={() => loadLogs()}
            className="zf-btn zf-btn-ghost zf-btn-sm mx-auto"
          >
            <RefreshCw className="w-3.5 h-3.5" />
            Retry
          </button>
        </div>
      )}

      {!error && loading && displayLogs.length === 0 && (
        <AppleTerminalFrame title={`${vm.name} — console`} empty emptyMessage="Loading console output..." />
      )}

      {!error && !loading && displayLogs.length === 0 && (
        <AppleTerminalFrame
          title={`${vm.name} — console`}
          empty
          emptyMessage={
            vm.state === 'stopped'
              ? 'No console output captured yet — it appears here once this VM has booted at least once.'
              : 'No console output yet — this can take a few seconds right after boot.'
          }
        />
      )}

      {displayLogs.length > 0 && (
        <AppleTerminalFrame
          title={`${vm.name} — console`}
          live={autoRefresh}
          bodyRef={termRef}
          onBodyScroll={onTermScroll}
          bodyClassName="max-h-[36rem] overflow-y-auto px-3 py-2"
        >
          {displayLogs.map((log, i) => {
            let ts = ''
            try {
              ts = new Date(log.timestamp).toLocaleTimeString(undefined, { hour12: false })
            } catch { /* ignore */ }
            const pri = log.priority?.toLowerCase() || 'info'
            return (
              <div key={i} className="flex gap-2.5 py-[1px] hover:bg-white/[0.04] rounded px-1 -mx-1">
                <span className="text-white/25 shrink-0 tabular-nums w-[4.5rem] text-right select-none">
                  {ts}
                </span>
                <span
                  className={`mt-[6px] w-1.5 h-1.5 rounded-full shrink-0 ${LOG_PRIORITY_DOT[pri] || 'bg-zinc-500'}`}
                  title={pri}
                />
                <span className="min-w-0 break-words whitespace-pre-wrap">
                  <AnsiText text={log.message} />
                </span>
              </div>
            )
          })}
        </AppleTerminalFrame>
      )}

      <div>
        <button
          onClick={() => setShowActivity((v) => !v)}
          className="text-xs text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition-colors"
        >
          {showActivity ? '▾' : '▸'} Activity (create/start/stop history)
        </button>
        {showActivity && (
          activity.length === 0 ? (
            <div className="zf-panel mt-2">
              <p className="text-[var(--zf-muted)] text-sm p-4">No activity recorded for this VM.</p>
            </div>
          ) : (
            <AppleTerminalFrame
              title={`${vm.name} — activity`}
              className="mt-2"
              bodyClassName="max-h-64 overflow-y-auto px-3 py-2"
            >
              {activity.map((log) => (
                <div key={log.id} className="flex gap-3 py-0.5">
                  <span className="text-white/40 shrink-0 tabular-nums">
                    {new Date(log.timestamp).toLocaleTimeString()}
                  </span>
                  <span
                    className="shrink-0 w-14 uppercase"
                    style={{ color: log.status === 'success' ? '#64d2ff' : '#ff453a' }}
                  >
                    {log.status === 'success' ? 'INFO' : 'ERROR'}
                  </span>
                  <span className="text-white/50 shrink-0 w-16">{log.action}</span>
                  <span className="min-w-0 break-words">
                    {log.details || `${log.action} by ${log.user}`}
                    {log.error && <span className="text-[#ff453a] ml-2">({log.error})</span>}
                  </span>
                </div>
              ))}
            </AppleTerminalFrame>
          )
        )}
      </div>
    </div>
  )
}
