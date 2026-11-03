// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PanelLoading } from '../components/Skeleton'
import { useState, useEffect } from 'react'
import { Cpu, Server, MemoryStick, HardDrive, Zap } from 'lucide-react'
import {
  getCpuTopology,
  getNumaTopology,
  getSystemMemory,
  getHugepageStats,
  allocateHugepages,
  getOptimizationRecommendations,
  optimizeVM,
  type CpuTopology,
  type NumaTopology,
  type SystemMemory,
  type HugepageStats,
  type OptimizationRecommendation,
} from '../api/system'
import { useToastContext } from '../contexts/ToastContext'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, Modal } from '../components/ui'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'

export default function SystemResources() {
  const toast = useToastContext()
  const [cpuTopology, setCpuTopology] = useState<CpuTopology | null>(null)
  const [numaTopology, setNumaTopology] = useState<NumaTopology | null>(null)
  const [systemMemory, setSystemMemory] = useState<SystemMemory | null>(null)
  const [hugepages2mb, setHugepages2mb] = useState<HugepageStats | null>(null)
  const [hugepages1gb, setHugepages1gb] = useState<HugepageStats | null>(null)
  const [recommendations, setRecommendations] = useState<OptimizationRecommendation[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'cpu' | 'numa' | 'memory' | 'optimization'>('cpu')

  useEffect(() => {
    loadResources()
  }, [])

  const loadResources = async () => {
    setLoadError(null)
    try {
      const [cpu, numa, memory, hp2mb, hp1gb, recs] = await Promise.all([
        getCpuTopology().catch(() => null),
        getNumaTopology().catch(() => null),
        getSystemMemory().catch(() => null),
        getHugepageStats('Size2MB').catch(() => null),
        getHugepageStats('Size1GB').catch(() => null),
        getOptimizationRecommendations().catch(() => []),
      ])

      setCpuTopology(cpu)
      setNumaTopology(numa)
      setSystemMemory(memory)
      setHugepages2mb(hp2mb)
      setHugepages1gb(hp1gb)
      setRecommendations(recs)

      if (!cpu && !numa && !memory) {
        setLoadError('No system resource data is available from the host.')
      }
    } catch (error) {
      const msg = formatUserError(error)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load system resources', error)
    } finally {
      setLoading(false)
    }
  }

  const formatMemory = (kb: number) => {
    const gb = kb / (1024 * 1024)
    if (gb >= 1) {
      return `${gb.toFixed(2)} GB`
    }
    return `${(kb / 1024).toFixed(2)} MB`
  }

  if (loading) {
    return (
      <div className="p-8 space-y-6">
        <PageHeader title="System Resources" description="Hardware topology and resource allocation" />
        <PanelLoading label="Loading system resources" />
      </div>
    )
  }

  return (
    <div className="p-8 space-y-6">
      <PageHeader
        title="System Resources"
        description="Hardware topology and resource allocation"
        onRefresh={loadResources}
        refreshing={loading}
      />

      {loadError && (
        <ErrorBanner
          title="Could not load system resources"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={loadResources}
        />
      )}

      {/* Statistics Cards */}
      <div className="grid grid-cols-4 gap-6 mb-8">
        <div className="bg-[var(--zf-canvas)] rounded-lg p-6">
          <div className="flex items-center gap-3 mb-2">
            <Cpu className="w-5 h-5 text-[var(--zf-ink)]" />
            <div className="text-[var(--zf-muted)] text-sm">Total CPUs</div>
          </div>
          <div className="text-2xl font-bold">{cpuTopology?.total_cpus || 0}</div>
          <div className="text-sm text-[var(--zf-muted)] mt-1">
            {cpuTopology?.sockets || 0} socket(s) × {cpuTopology?.cores_per_socket || 0} cores
          </div>
        </div>

        <div className="bg-[var(--zf-canvas)] rounded-lg p-6">
          <div className="flex items-center gap-3 mb-2">
            <Server className="w-5 h-5 text-[var(--zf-ink)]" />
            <div className="text-[var(--zf-muted)] text-sm">NUMA Nodes</div>
          </div>
          <div className="text-2xl font-bold">{numaTopology?.nodes.length || 0}</div>
          <div className="text-sm text-[var(--zf-muted)] mt-1">
            {numaTopology ? 'Available' : 'Not available'}
          </div>
        </div>

        <div className="bg-[var(--zf-canvas)] rounded-lg p-6">
          <div className="flex items-center gap-3 mb-2">
            <MemoryStick className="w-5 h-5 text-[var(--zf-ink)]" />
            <div className="text-[var(--zf-muted)] text-sm">Total Memory</div>
          </div>
          <div className="text-2xl font-bold">
            {systemMemory ? formatMemory(systemMemory.total_kb) : 'N/A'}
          </div>
          <div className="text-sm text-[var(--zf-muted)] mt-1">
            {systemMemory ? formatMemory(systemMemory.available_kb) : 'N/A'} available
          </div>
        </div>

        <div className="bg-[var(--zf-canvas)] rounded-lg p-6">
          <div className="flex items-center gap-3 mb-2">
            <HardDrive className="w-5 h-5 text-[var(--zf-ink)]" />
            <div className="text-[var(--zf-muted)] text-sm">Hugepages (2MB)</div>
          </div>
          <div className="text-2xl font-bold">{hugepages2mb?.total || 0}</div>
          <div className="text-sm text-[var(--zf-muted)] mt-1">
            {hugepages2mb?.free || 0} free
          </div>
        </div>
      </div>

      {/* Tabs */}
      <div className="bg-[var(--zf-canvas)] rounded-lg overflow-hidden">
        <div className="border-b border-[var(--zf-hairline)]">
          <div className="flex">
            <button
              onClick={() => setActiveTab('cpu')}
              className={`px-6 py-3 text-sm font-medium ${
                activeTab === 'cpu'
                  ? 'bg-[var(--zf-surface)] border-b-2 border-[var(--zf-ink)] text-[var(--zf-ink)]'
                  : 'text-[var(--zf-muted)] hover:bg-black/[0.04]'
              }`}
            >
              CPU Topology
            </button>
            <button
              onClick={() => setActiveTab('numa')}
              className={`px-6 py-3 text-sm font-medium ${
                activeTab === 'numa'
                  ? 'bg-[var(--zf-surface)] border-b-2 border-[var(--zf-ink)] text-[var(--zf-ink)]'
                  : 'text-[var(--zf-muted)] hover:bg-black/[0.04]'
              }`}
            >
              NUMA Topology
            </button>
            <button
              onClick={() => setActiveTab('memory')}
              className={`px-6 py-3 text-sm font-medium ${
                activeTab === 'memory'
                  ? 'bg-[var(--zf-surface)] border-b-2 border-[var(--zf-ink)] text-[var(--zf-ink)]'
                  : 'text-[var(--zf-muted)] hover:bg-black/[0.04]'
              }`}
            >
              Memory & Hugepages
            </button>
            <button
              onClick={() => setActiveTab('optimization')}
              className={`px-6 py-3 flex items-center gap-2 text-sm font-medium ${
                activeTab === 'optimization'
                  ? 'bg-[var(--zf-surface)] border-b-2 border-[var(--zf-ink)] text-[var(--zf-ink)]'
                  : 'text-[var(--zf-muted)] hover:bg-black/[0.04]'
              }`}
            >
              <Zap className="w-4 h-4" />
              Optimization
              {recommendations.length > 0 && (
                <span className="ml-1 px-2 py-0.5 text-xs bg-[var(--zf-ink)] text-white rounded-full">{recommendations.length}</span>
              )}
            </button>
          </div>
        </div>

        <div className="p-6">
          {activeTab === 'cpu' && <CpuTopologyView topology={cpuTopology} />}
          {activeTab === 'numa' && <NumaTopologyView topology={numaTopology} />}
          {activeTab === 'memory' && (
            <MemoryView
              memory={systemMemory}
              hugepages2mb={hugepages2mb}
              hugepages1gb={hugepages1gb}
              onRefresh={loadResources}
            />
          )}
          {activeTab === 'optimization' && (
            <OptimizationView recommendations={recommendations} onOptimize={async (vmName) => {
              try {
                const result = await optimizeVM(vmName)
                toast.success(`Optimized '${vmName}': ${result.applied.length} settings applied`)
                loadResources()
              } catch (error) {
                toastFailure(toast, `Failed to optimize '${vmName}'`, error)
              }
            }} />
          )}
        </div>
      </div>
    </div>
  )
}

function CpuTopologyView({ topology }: { topology: CpuTopology | null }) {
  if (!topology) {
    return <div className="text-[var(--zf-muted)]">CPU topology information not available</div>
  }

  // Group CPUs by socket
  const cpusBySocket = new Map<number, typeof topology.cpus>()
  topology.cpus.forEach((cpu) => {
    if (!cpusBySocket.has(cpu.socket_id)) {
      cpusBySocket.set(cpu.socket_id, [])
    }
    cpusBySocket.get(cpu.socket_id)!.push(cpu)
  })

  return (
    <div>
      <div className="grid grid-cols-2 gap-6 mb-6">
        <div>
          <div className="text-sm text-[var(--zf-muted)] mb-1">Architecture</div>
          <div className="text-lg font-medium">
            {topology.sockets} Socket(s) × {topology.cores_per_socket} Core(s) ×{' '}
            {topology.threads_per_core} Thread(s)
          </div>
        </div>
        <div>
          <div className="text-sm text-[var(--zf-muted)] mb-1">Online CPUs</div>
          <div className="text-lg font-medium">{topology.online_cpus.length} CPUs</div>
          {topology.offline_cpus.length > 0 && (
            <div className="text-sm text-[var(--zf-muted)]">
              {topology.offline_cpus.length} offline
            </div>
          )}
        </div>
      </div>

      <div className="space-y-6">
        {Array.from(cpusBySocket.entries()).map(([socketId, cpus]) => (
          <div key={socketId} className="bg-[var(--zf-canvas)] rounded-lg p-4">
            <div className="font-medium mb-3">Socket {socketId}</div>
            <div className="grid grid-cols-8 gap-2">
              {cpus.map((cpu) => (
                <div
                  key={cpu.id}
                  className={`p-2 rounded text-center text-sm border ${
                    cpu.online ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-[var(--zf-muted)] bg-[var(--zf-surface)] border-[var(--zf-hairline)]'
                  }`}
                  title={`CPU ${cpu.id}\nCore ${cpu.core_id}\nThread ${cpu.thread_id}\nNUMA ${
                    cpu.numa_node ?? 'N/A'
                  }`}
                >
                  {cpu.id}
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

function NumaTopologyView({ topology }: { topology: NumaTopology | null }) {
  if (!topology) {
    return <div className="text-[var(--zf-muted)]">NUMA topology not available on this system</div>
  }

  const formatMemory = (mb: number) => {
    if (mb >= 1024) {
      return `${(mb / 1024).toFixed(2)} GB`
    }
    return `${mb} MB`
  }

  return (
    <div className="space-y-6">
      {topology.nodes.map((node) => (
        <div key={node.id} className="bg-[var(--zf-canvas)] rounded-lg p-6">
          <div className="flex items-center justify-between mb-4">
            <div>
              <div className="text-xl font-bold mb-1">Node {node.id}</div>
              <div className="text-sm text-[var(--zf-muted)]">{node.cpus.length} CPUs</div>
            </div>
            <div className="text-right">
              <div className="text-lg font-medium">{formatMemory(node.memory_total_mb)}</div>
              <div className="text-sm text-[var(--zf-muted)]">
                {formatMemory(node.memory_free_mb)} free
              </div>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4 mb-4">
            <div>
              <div className="text-sm text-[var(--zf-muted)] mb-1">CPU List</div>
              <div className="font-mono text-sm">{node.cpus.join(', ')}</div>
            </div>
            <div>
              <div className="text-sm text-[var(--zf-muted)] mb-1">Memory Usage</div>
              <div className="flex items-center gap-2">
                <div className="flex-1 bg-[var(--zf-hairline)] rounded-full h-2 overflow-hidden">
                  <div
                    className="h-full bg-[var(--zf-link-fill)]"
                    style={{
                      width: `${
                        ((node.memory_total_mb - node.memory_free_mb) / node.memory_total_mb) *
                        100
                      }%`,
                    }}
                  />
                </div>
                <span className="text-sm">
                  {(
                    ((node.memory_total_mb - node.memory_free_mb) / node.memory_total_mb) *
                    100
                  ).toFixed(0)}
                  %
                </span>
              </div>
            </div>
          </div>

          {(node.hugepages_2mb_total > 0 || node.hugepages_1gb_total > 0) && (
            <div className="grid grid-cols-2 gap-4 pt-4 border-t border-[var(--zf-hairline)]">
              <div>
                <div className="text-sm text-[var(--zf-muted)] mb-1">Hugepages 2MB</div>
                <div className="text-sm">
                  {node.hugepages_2mb_free} / {node.hugepages_2mb_total} free
                </div>
              </div>
              <div>
                <div className="text-sm text-[var(--zf-muted)] mb-1">Hugepages 1GB</div>
                <div className="text-sm">
                  {node.hugepages_1gb_free} / {node.hugepages_1gb_total} free
                </div>
              </div>
            </div>
          )}
        </div>
      ))}

      {topology.distances.length > 0 && (
        <div className="bg-[var(--zf-canvas)] rounded-lg p-6">
          <div className="font-medium mb-3">Inter-Node Distances</div>
          <table className="w-full">
            <thead>
              <tr>
                <th className="p-2 text-left text-sm text-[var(--zf-muted)]">From \ To</th>
                {topology.nodes.map((node) => (
                  <th key={node.id} className="p-2 text-sm text-[var(--zf-muted)]">
                    Node {node.id}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {topology.distances.map((row, fromIdx) => (
                <tr key={fromIdx}>
                  <td className="p-2 text-sm font-medium">Node {fromIdx}</td>
                  {row.map((distance, toIdx) => (
                    <td
                      key={toIdx}
                      className={`p-2 text-sm text-center ${
                        fromIdx === toIdx ? 'bg-[var(--zf-canvas)] font-semibold' : ''
                      }`}
                    >
                      {distance}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

function MemoryView({
  memory,
  hugepages2mb,
  hugepages1gb,
  onRefresh,
}: {
  memory: SystemMemory | null
  hugepages2mb: HugepageStats | null
  hugepages1gb: HugepageStats | null
  onRefresh: () => void
}) {
  const [showAllocateDialog, setShowAllocateDialog] = useState(false)

  if (!memory) {
    return <div className="text-[var(--zf-muted)]">Memory information not available</div>
  }

  const formatMemory = (kb: number) => {
    const gb = kb / (1024 * 1024)
    if (gb >= 1) {
      return `${gb.toFixed(2)} GB`
    }
    return `${(kb / 1024).toFixed(2)} MB`
  }

  const usagePercent = ((memory.total_kb - memory.available_kb) / memory.total_kb) * 100

  return (
    <div>
      <div className="bg-[var(--zf-canvas)] rounded-lg p-6 mb-6">
        <div className="text-lg font-medium mb-4">System Memory</div>
        <div className="grid grid-cols-3 gap-6 mb-4">
          <div>
            <div className="text-sm text-[var(--zf-muted)] mb-1">Total</div>
            <div className="text-xl font-medium">{formatMemory(memory.total_kb)}</div>
          </div>
          <div>
            <div className="text-sm text-[var(--zf-muted)] mb-1">Available</div>
            <div className="text-xl font-medium text-emerald-700">
              {formatMemory(memory.available_kb)}
            </div>
          </div>
          <div>
            <div className="text-sm text-[var(--zf-muted)] mb-1">Usage</div>
            <div className="flex items-center gap-2">
              <div className="flex-1 bg-[var(--zf-hairline)] h-3 overflow-hidden">
                <div
                  className={`h-full ${
                    usagePercent > 90 ? 'bg-[var(--zf-danger)]' : usagePercent > 75 ? 'bg-[var(--zf-warning)]' : 'bg-[var(--zf-link-fill)]'
                  }`}
                  style={{ width: `${usagePercent}%` }}
                />
              </div>
              <span className="text-sm">{usagePercent.toFixed(0)}%</span>
            </div>
          </div>
        </div>

        <div className="grid grid-cols-2 gap-6 pt-4 border-t border-[var(--zf-hairline)]">
          <div>
            <div className="text-sm text-[var(--zf-muted)] mb-1">Buffers</div>
            <div className="text-sm">{formatMemory(memory.buffers_kb)}</div>
          </div>
          <div>
            <div className="text-sm text-[var(--zf-muted)] mb-1">Cached</div>
            <div className="text-sm">{formatMemory(memory.cached_kb)}</div>
          </div>
        </div>
      </div>

      <div className="flex items-center justify-between mb-4">
        <div className="text-lg font-medium">Hugepages</div>
        <button
          onClick={() => setShowAllocateDialog(true)}
          className="zf-btn zf-btn-primary zf-btn-sm"
        >
          Allocate Hugepages
        </button>
      </div>

      <div className="grid grid-cols-2 gap-6">
        <div className="bg-[var(--zf-canvas)] rounded-lg p-6">
          <div className="text-lg font-medium mb-4">2MB Hugepages</div>
          {hugepages2mb ? (
            <div className="space-y-3">
              <div>
                <div className="text-sm text-[var(--zf-muted)] mb-1">Total</div>
                <div className="text-2xl font-bold">{hugepages2mb.total}</div>
                <div className="text-sm text-[var(--zf-muted)]">
                  {(hugepages2mb.total * 2).toFixed(0)} MB
                </div>
              </div>
              <div>
                <div className="text-sm text-[var(--zf-muted)] mb-1">Free</div>
                <div className="text-xl text-emerald-700">{hugepages2mb.free}</div>
              </div>
              <div>
                <div className="text-sm text-[var(--zf-muted)] mb-1">Reserved</div>
                <div className="text-sm">{hugepages2mb.reserved}</div>
              </div>
            </div>
          ) : (
            <div className="text-[var(--zf-muted)]">Not available</div>
          )}
        </div>

        <div className="bg-[var(--zf-canvas)] rounded-lg p-6">
          <div className="text-lg font-medium mb-4">1GB Hugepages</div>
          {hugepages1gb ? (
            <div className="space-y-3">
              <div>
                <div className="text-sm text-[var(--zf-muted)] mb-1">Total</div>
                <div className="text-2xl font-bold">{hugepages1gb.total}</div>
                <div className="text-sm text-[var(--zf-muted)]">{hugepages1gb.total} GB</div>
              </div>
              <div>
                <div className="text-sm text-[var(--zf-muted)] mb-1">Free</div>
                <div className="text-xl text-emerald-700">{hugepages1gb.free}</div>
              </div>
              <div>
                <div className="text-sm text-[var(--zf-muted)] mb-1">Reserved</div>
                <div className="text-sm">{hugepages1gb.reserved}</div>
              </div>
            </div>
          ) : (
            <div className="text-[var(--zf-muted)]">Not available</div>
          )}
        </div>
      </div>

      {showAllocateDialog && (
        <AllocateHugepagesDialog
          onClose={() => setShowAllocateDialog(false)}
          onAllocated={() => {
            setShowAllocateDialog(false)
            onRefresh()
          }}
        />
      )}
    </div>
  )
}

function OptimizationView({
  recommendations,
  onOptimize,
}: {
  recommendations: OptimizationRecommendation[]
  onOptimize: (vmName: string) => void
}) {
  if (recommendations.length === 0) {
    return (
      <div className="text-center py-12">
        <Zap className="w-12 h-12 mx-auto mb-4 text-[var(--zf-muted)]" />
        <p className="text-lg text-[var(--zf-muted)] mb-2">No optimization recommendations</p>
        <p className="text-sm text-[var(--zf-muted)]">All running VMs are configured optimally, or no VMs are running.</p>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <p className="text-sm text-[var(--zf-muted)]">
        Recommendations based on system topology analysis. Click "Apply" to auto-configure optimal settings.
      </p>
      {recommendations.map((rec) => (
        <div key={rec.vm_name} className="bg-[var(--zf-canvas)] rounded-lg p-6">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-lg font-bold">{rec.vm_name}</h3>
            <button
              onClick={() => onOptimize(rec.vm_name)}
              className="zf-btn zf-btn-primary zf-btn-sm"
            >
              <Zap className="w-4 h-4" />
              Apply
            </button>
          </div>
          <div className="space-y-3">
            {rec.recommendations.map((r, idx) => (
              <div key={idx} className="bg-[var(--zf-canvas)] rounded p-4">
                <div className="flex items-center justify-between mb-2">
                  <span className="font-medium text-[var(--zf-link)]">{r.resource}</span>
                  <span className="text-xs text-[var(--zf-muted)]">
                    {r.current_value} &rarr; {r.recommended_value}
                  </span>
                </div>
                <p className="text-sm text-[var(--zf-muted)] mb-1">{r.reason}</p>
                <p className="text-xs text-emerald-700">{r.impact}</p>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}

function AllocateHugepagesDialog({
  onClose,
  onAllocated,
}: {
  onClose: () => void
  onAllocated: () => void
}) {
  const toast = useToastContext()
  const [size, setSize] = useState<'Size2MB' | 'Size1GB'>('Size2MB')
  const [count, setCount] = useState(128)

  const handleAllocate = async () => {
    try {
      await allocateHugepages({ size, count })
      onAllocated()
    } catch (error) {
      toastFailure(toast, 'Failed to allocate hugepages', error)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Allocate Hugepages</h2>

      <div className="mb-4">
        <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Size</label>
        <select aria-label="Size"
          value={size}
          onChange={(e) => setSize(e.target.value as any)}
          className="input-field"
        >
          <option value="Size2MB">2MB</option>
          <option value="Size1GB">1GB</option>
        </select>
      </div>

      <div className="mb-6">
        <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Count</label>
        <input aria-label="Count"
          type="number"
          value={count}
          onChange={(e) => setCount(parseInt(e.target.value) || 0)}
          min="0"
          className="input-field"
        />
        <div className="text-sm text-[var(--zf-muted)] mt-1">
          Total: {size === 'Size2MB' ? (count * 2) / 1024 : count} GB
        </div>
      </div>

      <div className="flex gap-4">
        <button
          onClick={onClose}
          className="zf-btn zf-btn-ghost flex-1"
        >
          Cancel
        </button>
        <button
          onClick={handleAllocate}
          className="zf-btn zf-btn-primary flex-1"
        >
          Allocate
        </button>
      </div>
    </Modal>
  )
}
