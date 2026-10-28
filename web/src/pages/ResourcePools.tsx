// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Plus, ChevronRight, ChevronDown, Trash2, X, ArrowRightLeft } from 'lucide-react'
import {
  listPools,
  createPool,
  deletePool,
  getPoolSummary,
  checkAdmission,
  assignVm,
  unassignVm,
  moveVm,
  sharesValue,
  sharesLabel,
  type ResourcePool,
  type ResourcePoolSummary,
  type AdmissionControlResult,
  type SharesLevel,
} from '../api/resourcePools'
import { listVMs, VM } from '../api/vm'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import { PageHeader } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'
import { toastFailure } from '../utils/toastError'

export default function ResourcePools() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [pools, setPools] = useState<ResourcePool[]>([])
  const [summaries, setSummaries] = useState<Map<string, ResourcePoolSummary>>(new Map())
  const [vms, setVMs] = useState<VM[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load resource pools')
  const [expandedPools, setExpandedPools] = useState<Set<string>>(new Set())
  const [showCreatePool, setShowCreatePool] = useState(false)
  const [showAdmissionTest, setShowAdmissionTest] = useState<string | null>(null)
  const [admissionResult, setAdmissionResult] = useState<AdmissionControlResult | null>(null)
  const [assignTargetPool, setAssignTargetPool] = useState<string | null>(null)
  const [moveTarget, setMoveTarget] = useState<{ poolId: string; vmName: string } | null>(null)

  const loadData = useCallback(() => {
    return run(async () => {
      const [data, vmList] = await Promise.all([listPools(), listVMs()])
      setPools(data)
      setVMs(vmList)
      const sums = new Map<string, ResourcePoolSummary>()
      for (const pool of data) {
        try {
          const s = await getPoolSummary(pool.id)
          sums.set(pool.id, s)
        } catch { /* skip */ }
      }
      setSummaries(sums)
    })
  }, [run])

  const handleAssign = async (poolId: string, vmName: string) => {
    try {
      await assignVm(poolId, vmName)
      toast.success(`'${vmName}' assigned to pool`)
      setAssignTargetPool(null)
      loadData()
    } catch (err) { toastFailure(toast, 'Failed to assign VM', err) }
  }

  const handleUnassign = async (poolId: string, vmName: string) => {
    const ok = await confirm('Unassign VM', `Remove '${vmName}' from this resource pool?`, { variant: 'danger', confirmLabel: 'Unassign' })
    if (!ok) return
    try {
      await unassignVm(poolId, vmName)
      toast.success(`'${vmName}' unassigned`)
      loadData()
    } catch (err) { toastFailure(toast, 'Failed to unassign VM', err) }
  }

  const handleMove = async (fromPoolId: string, vmName: string, toPoolId: string) => {
    try {
      await moveVm(vmName, fromPoolId, toPoolId)
      toast.success(`'${vmName}' moved`)
      setMoveTarget(null)
      loadData()
    } catch (err) { toastFailure(toast, 'Failed to move VM', err) }
  }

  useEffect(() => {
    void loadData()
  }, [loadData])

  const togglePool = (id: string) => {
    setExpandedPools(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const handleDelete = async (id: string) => {
    const ok = await confirm('Delete Resource Pool', 'Delete this resource pool?', { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return
    try {
      await deletePool(id)
      toast.success('Resource pool deleted')
      loadData()
    } catch { toast.error('Failed to delete resource pool') }
  }

  const handleAdmissionTest = async (poolId: string, cpu: number, memoryMb: number) => {
    try {
      const result = await checkAdmission(poolId, { cpu, memory_mb: memoryMb })
      setAdmissionResult(result)
    } catch { toast.error('Admission test failed') }
  }

  const rootPools = pools.filter(p => !p.parent_id)
  const getChildren = (parentId: string) => pools.filter(p => p.parent_id === parentId)

  const renderPool = (pool: ResourcePool, depth: number) => {
    const children = getChildren(pool.id)
    const isExpanded = expandedPools.has(pool.id)
    const summary = summaries.get(pool.id)
    const cpuPct = summary?.cpu_limit_mhz ? (summary.cpu_used_mhz / summary.cpu_limit_mhz * 100) : 0
    const memPct = summary?.memory_limit_mb ? (summary.memory_used_mb / summary.memory_limit_mb * 100) : 0

    return (
      <div key={pool.id}>
        <div
          className="flex items-center justify-between p-3 hover:bg-[var(--zf-surface)] cursor-pointer border-b border-[var(--zf-hairline)]"
          style={{ paddingLeft: `${depth * 24 + 16}px` }}
          onClick={() => togglePool(pool.id)}
        >
          <div className="flex items-center gap-3 min-w-0 flex-1">
            {children.length > 0 ? (
              isExpanded ? <ChevronDown className="w-4 h-4 shrink-0" /> : <ChevronRight className="w-4 h-4 shrink-0" />
            ) : <div className="w-4 shrink-0" />}
            <span className="font-medium truncate">{pool.name}</span>
            <span className="text-xs text-[var(--zf-muted)] truncate">
              {pool.vms.length} VMs | CPU: {sharesLabel(pool.cpu_shares)} | Mem: {sharesLabel(pool.memory_shares)}
            </span>
          </div>
          <div className="flex items-center gap-4 shrink-0" onClick={e => e.stopPropagation()}>
            <div className="flex items-center gap-2 text-sm">
              <span className="text-[var(--zf-muted)]">CPU</span>
              <div className="w-20 bg-[var(--zf-surface)] rounded-full h-2">
                <div className={`h-2 rounded-full ${cpuPct > 80 ? 'bg-[var(--zf-danger)]' : 'bg-[var(--zf-link-fill)]'}`}
                  style={{ width: `${Math.min(cpuPct, 100)}%` }} />
              </div>
            </div>
            <div className="flex items-center gap-2 text-sm">
              <span className="text-[var(--zf-muted)]">Mem</span>
              <div className="w-20 bg-[var(--zf-surface)] rounded-full h-2">
                <div className={`h-2 rounded-full ${memPct > 80 ? 'bg-[var(--zf-danger)]' : 'bg-[var(--zf-ink)]'}`}
                  style={{ width: `${Math.min(memPct, 100)}%` }} />
              </div>
            </div>
            <button onClick={() => setShowAdmissionTest(pool.id)} className="text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] text-xs px-2 py-1">
              Test Admission
            </button>
            <button onClick={() => handleDelete(pool.id)} className="text-red-600 hover:text-red-800 p-1" aria-label="Delete" title="Delete">
              <Trash2 className="w-4 h-4" />
            </button>
          </div>
        </div>

        {isExpanded && (
          <>
            {summary && (
              <div className="grid grid-cols-4 gap-3 p-3 bg-[var(--zf-surface)]" style={{ paddingLeft: `${depth * 24 + 40}px` }}>
                <div className="text-xs"><span className="text-[var(--zf-muted)]">Reservation:</span> {pool.cpu_reservation_mhz} MHz / {pool.memory_reservation_mb} MB</div>
                <div className="text-xs"><span className="text-[var(--zf-muted)]">Limit:</span> {pool.cpu_limit_mhz ?? 'Unlimited'} MHz / {pool.memory_limit_mb ?? 'Unlimited'} MB</div>
                <div className="text-xs"><span className="text-[var(--zf-muted)]">Used:</span> {summary.cpu_used_mhz} MHz / {summary.memory_used_mb} MB</div>
                <div className="text-xs"><span className="text-[var(--zf-muted)]">Children:</span> {summary.child_pool_count} pools</div>
              </div>
            )}
            <div className="p-3 bg-[var(--zf-surface)] border-t border-[var(--zf-hairline)]" style={{ paddingLeft: `${depth * 24 + 40}px` }}>
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Assigned VMs</span>
                <button onClick={() => setAssignTargetPool(pool.id)} className="text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] text-xs flex items-center gap-1">
                  <Plus className="w-3.5 h-3.5" /> Assign VM
                </button>
              </div>
              {pool.vms.length === 0 ? (
                <p className="text-xs text-[var(--zf-muted)]">No VMs assigned.</p>
              ) : (
                <div className="space-y-1">
                  {pool.vms.map(vmName => (
                    <div key={vmName} className="flex items-center justify-between text-sm bg-[var(--zf-canvas)] rounded px-3 py-1.5">
                      <span className="truncate">{vmName}</span>
                      <div className="flex items-center gap-3 shrink-0">
                        {pools.length > 1 && (
                          <button onClick={() => setMoveTarget({ poolId: pool.id, vmName })} className="text-[var(--zf-muted)] hover:text-[var(--zf-link)]" title="Move to another pool">
                            <ArrowRightLeft className="w-3.5 h-3.5" />
                          </button>
                        )}
                        <button onClick={() => handleUnassign(pool.id, vmName)} className="text-[var(--zf-muted)] hover:text-red-600" title="Unassign">
                          <X className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
            {children.map(child => renderPool(child, depth + 1))}
          </>
        )}
      </div>
    )
  }


  return (
    <div className="p-6">
      <PageHeader
        onRefresh={() => void loadData()}
        refreshing={loading}
        title="Resource Pools"
        actions={
          <button onClick={() => setShowCreatePool(true)} className="zf-btn zf-btn-primary">
            <Plus className="w-4 h-4" /> Create Pool
          </button>
        }
      />

      <PageLoadBanner title="Could not load resource pools" headline={loadError} onRetry={() => void loadData()} />

      {/* Stats */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mb-4">
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Total Pools</div>
          <div className="text-2xl font-bold">{pools.length}</div>
        </div>
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Total CPU Shares</div>
          <div className="text-2xl font-bold">{pools.reduce((s, p) => s + sharesValue(p.cpu_shares), 0)}</div>
        </div>
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Total VMs</div>
          <div className="text-2xl font-bold">{pools.reduce((s, p) => s + p.vms.length, 0)}</div>
        </div>
      </div>

      {/* Tree */}
      <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg">
        {rootPools.length === 0 ? (
          <div className="text-center py-12 text-[var(--zf-muted)]">No resource pools configured.</div>
        ) : (
          rootPools.map(pool => renderPool(pool, 0))
        )}
      </div>

      {/* Create Pool Modal */}
      {showCreatePool && (
        <CreatePoolModal
          pools={pools}
          onClose={() => setShowCreatePool(false)}
          onCreated={() => { setShowCreatePool(false); loadData() }}
        />
      )}

      {/* Admission Test Modal */}
      {showAdmissionTest && (
        <AdmissionTestModal
          poolId={showAdmissionTest}
          result={admissionResult}
          onTest={handleAdmissionTest}
          onClose={() => { setShowAdmissionTest(null); setAdmissionResult(null) }}
        />
      )}

      {/* Assign VM Modal */}
      {assignTargetPool && (
        <AssignVmModal
          vms={vms.filter(v => !pools.some(p => p.vms.includes(v.name)))}
          onAssign={vmName => handleAssign(assignTargetPool, vmName)}
          onClose={() => setAssignTargetPool(null)}
        />
      )}

      {/* Move VM Modal */}
      {moveTarget && (
        <MoveVmModal
          vmName={moveTarget.vmName}
          pools={pools.filter(p => p.id !== moveTarget.poolId)}
          onMove={toPoolId => handleMove(moveTarget.poolId, moveTarget.vmName, toPoolId)}
          onClose={() => setMoveTarget(null)}
        />
      )}

      {confirmState && (
        <ConfirmDialog
          title={confirmState.title}
          message={confirmState.message}
          confirmLabel={confirmState.confirmLabel}
          variant={confirmState.variant}
          onConfirm={confirmState.onConfirm}
          onCancel={cancel}
        />
      )}
    </div>
  )
}

function SharesLevelPicker({ label, value, onChange }: { label: string; value: SharesLevel; onChange: (v: SharesLevel) => void }) {
  const preset = typeof value === 'string' ? value : 'custom'
  const customValue = typeof value === 'string' ? 1000 : value.custom

  return (
    <div>
      <label className="block text-sm font-medium mb-1">{label}</label>
      <select value={preset} onChange={e => {
        const v = e.target.value
        onChange(v === 'custom' ? { custom: customValue } : (v as SharesLevel))
      }} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2">
        <option value="low">Low</option>
        <option value="normal">Normal</option>
        <option value="high">High</option>
        <option value="custom">Custom</option>
      </select>
      {preset === 'custom' && (
        <input type="number" value={customValue} onChange={e => onChange({ custom: Number(e.target.value) })} min={1}
          className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2 mt-2" placeholder="Custom share value" />
      )}
    </div>
  )
}

function CreatePoolModal({ pools, onClose, onCreated }: { pools: ResourcePool[]; onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [clusterId, setClusterId] = useState('')
  const [parentId, setParentId] = useState('')
  const [cpuShares, setCpuShares] = useState<SharesLevel>('normal')
  const [cpuReservation, setCpuReservation] = useState(0)
  const [cpuLimit, setCpuLimit] = useState('')
  const [memoryShares, setMemoryShares] = useState<SharesLevel>('normal')
  const [memoryReservation, setMemoryReservation] = useState(0)
  const [memoryLimit, setMemoryLimit] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await createPool({
        name,
        cluster_id: clusterId,
        parent_id: parentId || undefined,
        cpu_shares: cpuShares,
        cpu_reservation_mhz: cpuReservation,
        cpu_limit_mhz: cpuLimit ? Number(cpuLimit) : undefined,
        memory_shares: memoryShares,
        memory_reservation_mb: memoryReservation,
        memory_limit_mb: memoryLimit ? Number(memoryLimit) : undefined,
      })
      onCreated()
    } catch { toast.error('Failed to create pool') }
  }

  return (
    <div className="modal-backdrop">
      <div className="modal-card w-full max-w-lg">
        <h2 className="text-xl font-bold mb-4">Create Resource Pool</h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">Name</label>
            <input aria-label="Name" type="text" value={name} onChange={e => setName(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" required />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Cluster ID</label>
            <input aria-label="Cluster ID" type="text" value={clusterId} onChange={e => setClusterId(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" required />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Parent Pool (optional)</label>
            <select aria-label="Parent Pool (optional)" value={parentId} onChange={e => setParentId(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2">
              <option value="">None (root pool)</option>
              {pools.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
            </select>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <SharesLevelPicker label="CPU Shares" value={cpuShares} onChange={setCpuShares} />
            <SharesLevelPicker label="Memory Shares" value={memoryShares} onChange={setMemoryShares} />
            <div>
              <label className="block text-sm font-medium mb-1">CPU Reservation (MHz)</label>
              <input aria-label="CPU Reservation (MHz)" type="number" value={cpuReservation} onChange={e => setCpuReservation(Number(e.target.value))} min={0}
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">Memory Reservation (MB)</label>
              <input aria-label="Memory Reservation (MB)" type="number" value={memoryReservation} onChange={e => setMemoryReservation(Number(e.target.value))} min={0}
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">CPU Limit (MHz)</label>
              <input aria-label="CPU Limit (MHz)" type="number" value={cpuLimit} onChange={e => setCpuLimit(e.target.value)} min={1} placeholder="Unlimited"
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">Memory Limit (MB)</label>
              <input aria-label="Memory Limit (MB)" type="number" value={memoryLimit} onChange={e => setMemoryLimit(e.target.value)} min={1} placeholder="Unlimited"
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" />
            </div>
          </div>
          <div className="flex gap-3">
            <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
            <button type="submit" className="zf-btn zf-btn-primary flex-1">Create</button>
          </div>
        </form>
      </div>
    </div>
  )
}

function AdmissionTestModal({ poolId, result, onTest, onClose }: {
  poolId: string
  result: AdmissionControlResult | null
  onTest: (poolId: string, cpu: number, mem: number) => void
  onClose: () => void
}) {
  const [cpu, setCpu] = useState(1000)
  const [memory, setMemory] = useState(2048)

  return (
    <div className="modal-backdrop">
      <div className="modal-card w-full max-w-md">
        <h2 className="text-xl font-bold mb-4">Admission Control Test</h2>
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">Required CPU (MHz)</label>
            <input aria-label="Required CPU (MHz)" type="number" value={cpu} onChange={e => setCpu(Number(e.target.value))}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Required Memory (MB)</label>
            <input aria-label="Required Memory (MB)" type="number" value={memory} onChange={e => setMemory(Number(e.target.value))}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" />
          </div>
          <button onClick={() => onTest(poolId, cpu, memory)}
            className="zf-btn zf-btn-primary w-full">Run Test</button>
          {result && (
            <div className={`p-4 rounded border ${result.admitted ? 'border-emerald-200 bg-emerald-50' : 'border-red-200 bg-red-50'}`}>
              <div className="font-medium mb-2">{result.admitted ? 'Admitted' : 'Denied'}</div>
              {result.reason && <div className="text-sm text-[var(--zf-ink)]">{result.reason}</div>}
              <div className="text-xs text-[var(--zf-muted)] mt-2">
                Available: {result.available_cpu} MHz CPU, {result.available_memory_mb} MB Memory
              </div>
            </div>
          )}
          <button onClick={onClose} className="zf-btn zf-btn-ghost w-full">Close</button>
        </div>
      </div>
    </div>
  )
}

function AssignVmModal({ vms, onAssign, onClose }: { vms: VM[]; onAssign: (vmName: string) => void; onClose: () => void }) {
  const [vmName, setVmName] = useState(vms[0]?.name || '')

  return (
    <div className="modal-backdrop">
      <div className="modal-card w-full max-w-md">
        <h2 className="text-xl font-bold mb-4">Assign VM to Pool</h2>
        {vms.length === 0 ? (
          <>
            <p className="text-sm text-[var(--zf-muted)] mb-4">Every VM is already assigned to a resource pool.</p>
            <button onClick={onClose} className="zf-btn zf-btn-ghost w-full">Close</button>
          </>
        ) : (
          <>
            <div className="mb-4">
              <label className="block text-sm font-medium mb-1">VM</label>
              <select aria-label="VM" value={vmName} onChange={e => setVmName(e.target.value)}
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2">
                {vms.map(v => <option key={v.name} value={v.name}>{v.name}</option>)}
              </select>
            </div>
            <div className="flex gap-3">
              <button onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
              <button onClick={() => onAssign(vmName)} className="zf-btn zf-btn-primary flex-1">Assign</button>
            </div>
          </>
        )}
      </div>
    </div>
  )
}

function MoveVmModal({ vmName, pools, onMove, onClose }: { vmName: string; pools: ResourcePool[]; onMove: (toPoolId: string) => void; onClose: () => void }) {
  const [toPoolId, setToPoolId] = useState(pools[0]?.id || '')

  return (
    <div className="modal-backdrop">
      <div className="modal-card w-full max-w-md">
        <h2 className="text-xl font-bold mb-4">Move '{vmName}'</h2>
        <div className="mb-4">
          <label className="block text-sm font-medium mb-1">Target Pool</label>
          <select aria-label="Target Pool" value={toPoolId} onChange={e => setToPoolId(e.target.value)}
            className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2">
            {pools.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
          </select>
        </div>
        <div className="flex gap-3">
          <button onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
          <button onClick={() => onMove(toPoolId)} className="zf-btn zf-btn-primary flex-1">Move</button>
        </div>
      </div>
    </div>
  )
}
