// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PageLoading } from '../components/Skeleton'
import { useState, useEffect, useCallback } from 'react'
import { HardDrive, Trash2, Database, Plus, Link2, Link2Off, Maximize2 } from 'lucide-react'
import { apiGet } from '../api/client'
import { listVolumes, createVolume, deleteVolume, resizeVolume, attachVolume, detachVolume, type Volume } from '../api/volumes'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, Card, Modal } from '../components/ui'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import SubsystemBanner from '../components/SubsystemBanner'

interface StoragePool {
  id: string
  name: string
  pool_type: unknown
  path: string
  capacity: number
  available: number
  state: string
  auto_start: boolean
  created: string
  updated: string
}

interface VolumeRow extends Volume {
  pool: string
}

export default function Storage() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [pools, setPools] = useState<StoragePool[]>([])
  const [volumes, setVolumes] = useState<VolumeRow[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [showCreateVolume, setShowCreateVolume] = useState<string | null>(null)
  const [attachTarget, setAttachTarget] = useState<VolumeRow | null>(null)
  const [resizeTarget, setResizeTarget] = useState<VolumeRow | null>(null)
  const [busyVolume, setBusyVolume] = useState<string | null>(null)

  const loadData = useCallback(async () => {
    setLoadError(null)
    try {
      const poolData = await apiGet<StoragePool[]>('/api/storage/pools').catch(() => [])
      setPools(poolData)

      // Load volumes for each pool
      const allVolumes: VolumeRow[] = []
      for (const pool of poolData) {
        try {
          const vols = await listVolumes(pool.name)
          allVolumes.push(...vols.map(v => ({ ...v, pool: pool.name })))
        } catch {
          // Pool may not support volume listing
        }
      }
      setVolumes(allVolumes)
    } catch (error) {
      const msg = formatUserError(error)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load storage', error)
    } finally {
      setLoading(false)
    }
  }, [toast])

  useEffect(() => {
    loadData()
  }, [loadData])

  const handleDeleteVolume = async (pool: string, volumeId: string) => {
    if (!await confirm('Delete Volume Record', 'Delete this volume record? This only removes the tracking entry, not any real disk image.', { variant: 'danger', confirmLabel: 'Delete' })) return
    try {
      await deleteVolume(pool, volumeId)
      toast.success('Volume record deleted')
      await loadData()
    } catch (error) {
      toastFailure(toast, 'Failed to delete volume', error)
    }
  }

  const handleDetach = async (v: VolumeRow) => {
    setBusyVolume(v.id)
    try {
      await detachVolume(v.pool, v.id)
      toast.success(`Detached '${v.name}'`)
      await loadData()
    } catch (error) {
      toastFailure(toast, 'Failed to detach volume', error)
    } finally {
      setBusyVolume(null)
    }
  }

  const formatBytes = (bytes: number) => {
    const gb = bytes / (1024 * 1024 * 1024)
    if (gb >= 1024) return `${(gb / 1024).toFixed(1)} TB`
    if (gb >= 1) return `${gb.toFixed(1)} GB`
    const mb = bytes / (1024 * 1024)
    return `${mb.toFixed(0)} MB`
  }

  const getPoolTypeString = (pt: unknown): string => {
    if (typeof pt === 'string') return pt
    if (typeof pt === 'object' && pt !== null) {
      const obj = pt as Record<string, unknown>
      if ('Ceph' in obj) return 'Ceph'
      if ('NFS' in obj) return 'NFS'
      if ('ZFS' in obj) return 'ZFS'
      if ('LVM' in obj) return 'LVM'
      if ('LVMThin' in obj) return 'LVM-thin'
    }
    return 'Unknown'
  }

  const getUsageColor = (percentage: number) => {
    if (percentage < 50) return 'bg-[var(--zf-success)]'
    if (percentage < 80) return 'bg-[var(--zf-warning)]'
    return 'bg-[var(--zf-danger)]'
  }

  const getTypeColor = (type: string) => {
    switch (type.toLowerCase()) {
      case 'local': case 'directory': return 'text-[var(--zf-link)] bg-[var(--zf-link)]/10 border-[var(--zf-link)]/20'
      case 'lvm': case 'lvm-thin': return 'text-[var(--zf-ink)] bg-black/[0.04] border-[var(--zf-hairline)]'
      case 'zfs': return 'text-emerald-700 bg-emerald-50 border-emerald-200'
      case 'nfs': return 'text-amber-800 bg-amber-50 border-amber-200'
      case 'ceph': return 'text-red-700 bg-red-50 border-red-200'
      default: return 'text-[var(--zf-muted)] bg-black/[0.04] border-[var(--zf-hairline)]'
    }
  }


  if (loading) {
    return <PageLoading />
  }

  const totalCapacity = pools.reduce((sum, p) => sum + p.capacity, 0)
  const totalAvailable = pools.reduce((sum, p) => sum + p.available, 0)
  const totalUsed = totalCapacity - totalAvailable

  return (
    <div className="space-y-6">
      <SubsystemBanner subsystem="storage" title="Storage subsystem" />
      {loadError && (
        <ErrorBanner
          title="Could not load storage"
          headline={loadError}
          hints={hintsForError(loadError, 'storage')}
          onRetry={loadData}
        />
      )}
      <PageHeader
        title="Storage Management"
        onRefresh={loadData}
        refreshing={loading}
      />

      {/* Storage Stats */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <Card><div className="p-6">
          <div className="text-[var(--zf-muted)] text-sm mb-2">Total Capacity</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{formatBytes(totalCapacity)}</div>
        </div></Card>
        <Card><div className="p-6">
          <div className="text-[var(--zf-muted)] text-sm mb-2">Used</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{formatBytes(totalUsed)}</div>
        </div></Card>
        <Card><div className="p-6">
          <div className="text-[var(--zf-muted)] text-sm mb-2">Volumes</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{volumes.length}</div>
        </div></Card>
        <Card><div className="p-6">
          <div className="text-[var(--zf-muted)] text-sm mb-2">Pools</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{pools.length}</div>
        </div></Card>
      </div>

      {/* Storage Pools */}
      <Card>
        <div className="p-6 border-b border-[var(--zf-hairline)]">
          <h2 className="text-xl font-semibold text-[var(--zf-ink)]">Storage Pools</h2>
        </div>
        {pools.length === 0 ? (
          <div className="p-12 text-center text-[var(--zf-muted)]">No storage pools configured. Create one from the Storage Pools page.</div>
        ) : (
          <div className="p-6 space-y-4">
            {pools.map((pool) => {
              const used = pool.capacity - pool.available
              const percentage = pool.capacity > 0 ? Math.round((used / pool.capacity) * 100) : 0
              const typeStr = getPoolTypeString(pool.pool_type)
              return (
                <div key={pool.id} className="bg-[var(--zf-canvas)] rounded-lg p-4">
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center gap-3 min-w-0 flex-1">
                      <Database className="w-5 h-5 text-[var(--zf-muted)] shrink-0" />
                      <div className="min-w-0">
                        <div className="font-medium text-lg truncate text-[var(--zf-ink)]">{pool.name}</div>
                        <div className="text-sm text-[var(--zf-muted)] font-mono truncate">{pool.path}</div>
                      </div>
                    </div>
                    <div className="flex items-center gap-4 shrink-0">
                      <span className={`px-3 py-1 rounded-full text-xs font-medium border ${getTypeColor(typeStr)}`}>
                        {typeStr.toUpperCase()}
                      </span>
                      <span className={`text-sm font-medium ${pool.state === 'Active' ? 'text-emerald-700' : 'text-[var(--zf-muted)]'}`}>
                        {pool.state}
                      </span>
                      <button onClick={() => setShowCreateVolume(pool.name)} className="zf-btn zf-btn-ghost zf-btn-sm">
                        <Plus className="w-3.5 h-3.5" /> Add Volume Record
                      </button>
                    </div>
                  </div>
                  <div className="mb-2">
                    <div className="flex items-center justify-between text-sm mb-1">
                      <span className="text-[var(--zf-muted)]">{formatBytes(used)} / {formatBytes(pool.capacity)}</span>
                      <span className={`font-bold ${percentage > 80 ? 'text-red-700' : percentage > 50 ? 'text-amber-800' : 'text-emerald-700'}`}>
                        {percentage}%
                      </span>
                    </div>
                    <div className="w-full bg-[var(--zf-hairline)] rounded-full h-2">
                      <div
                        className={`h-2 rounded-full transition-all ${getUsageColor(percentage)}`}
                        style={{ width: `${percentage}%` }}
                      ></div>
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
        )}
      </Card>

      {/* Volumes */}
      <Card>
        <div className="p-6 border-b border-[var(--zf-hairline)]">
          <h2 className="text-xl font-semibold text-[var(--zf-ink)]">Volumes</h2>
          <p className="text-xs text-[var(--zf-muted)] mt-1">
            A manual tracking ledger — records here don't create or resize real disk images. Use them to track volumes you've provisioned elsewhere.
          </p>
        </div>
        {volumes.length === 0 ? (
          <div className="p-12 text-center text-[var(--zf-muted)]">No volume records.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-[var(--zf-surface)]">
                <tr>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Pool</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Size</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Attached To</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {volumes.map((volume) => (
                  <tr key={volume.id} className="hover:bg-black/[0.03] transition">
                    <td className="p-4">
                      <div className="flex items-center gap-2">
                        <HardDrive className="w-4 h-4 text-[var(--zf-muted)]" />
                        <span className="font-mono text-sm text-[var(--zf-ink)]">{volume.name}</span>
                      </div>
                    </td>
                    <td className="p-4 text-[var(--zf-muted)]">{volume.pool}</td>
                    <td className="p-4"><span className="font-mono text-sm text-[var(--zf-ink)]">{volume.size}</span></td>
                    <td className="p-4">
                      {volume.vm_attached ? (
                        <span className="text-[var(--zf-link)]">{volume.vm_attached}</span>
                      ) : (
                        <span className="text-[var(--zf-muted)] italic">Not attached</span>
                      )}
                    </td>
                    <td className="p-4">
                      <div className="flex items-center gap-2">
                        <button onClick={() => setResizeTarget(volume)} className="p-2 text-[var(--zf-muted)] hover:text-[var(--zf-ink)] hover:bg-black/[0.04] rounded transition" title="Resize record">
                          <Maximize2 className="w-4 h-4" />
                        </button>
                        {volume.vm_attached ? (
                          <button onClick={() => void handleDetach(volume)} disabled={busyVolume === volume.id} className="p-2 text-[var(--zf-muted)] hover:text-[var(--zf-ink)] hover:bg-black/[0.04] rounded transition disabled:opacity-50" title="Detach">
                            <Link2Off className="w-4 h-4" />
                          </button>
                        ) : (
                          <button onClick={() => setAttachTarget(volume)} className="p-2 text-[var(--zf-muted)] hover:text-[var(--zf-ink)] hover:bg-black/[0.04] rounded transition" title="Attach to VM">
                            <Link2 className="w-4 h-4" />
                          </button>
                        )}
                        <button onClick={() => handleDeleteVolume(volume.pool, volume.id)} className="p-2 text-[var(--zf-muted)] hover:text-white hover:bg-[var(--zf-danger)] rounded transition" title="Delete">
                          <Trash2 className="w-4 h-4" />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>

      {showCreateVolume && (
        <CreateVolumeModal pool={showCreateVolume} onClose={() => setShowCreateVolume(null)} onCreated={() => { setShowCreateVolume(null); void loadData() }} />
      )}
      {attachTarget && (
        <AttachVolumeModal volume={attachTarget} onClose={() => setAttachTarget(null)} onAttached={() => { setAttachTarget(null); void loadData() }} />
      )}
      {resizeTarget && (
        <ResizeVolumeModal volume={resizeTarget} onClose={() => setResizeTarget(null)} onResized={() => { setResizeTarget(null); void loadData() }} />
      )}

      {confirmState && (
        <ConfirmDialog
          title={confirmState.title}
          message={confirmState.message}
          confirmLabel={confirmState.confirmLabel ?? 'Delete'}
          variant={confirmState.variant ?? 'danger'}
          onConfirm={confirmState.onConfirm}
          onCancel={cancel}
        />
      )}
    </div>
  )
}

function CreateVolumeModal({ pool, onClose, onCreated }: { pool: string; onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [size, setSize] = useState('')
  const [creating, setCreating] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) { toast.error('Name is required'); return }
    if (!size.trim()) { toast.error('Size is required'); return }
    setCreating(true)
    try {
      await createVolume(pool, { name: name.trim(), size: size.trim() })
      toast.success(`Volume record '${name}' created`)
      onCreated()
    } catch (err) {
      toastFailure(toast, 'Failed to create volume record', err)
    } finally {
      setCreating(false)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-1 text-[var(--zf-ink)]">Add Volume Record</h2>
      <p className="text-sm text-[var(--zf-muted)] mb-4">Pool: {pool}</p>
      <p className="text-xs text-amber-800 bg-amber-50 border border-amber-200 rounded-lg px-3 py-2 mb-4">
        This creates a tracking record only — it does not provision a real disk image.
      </p>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Name</label>
          <input aria-label="Name" value={name} onChange={e => setName(e.target.value)} className="input-field" placeholder="my-volume" />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Size</label>
          <input aria-label="Size" value={size} onChange={e => setSize(e.target.value)} className="input-field" placeholder="20GB" />
        </div>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
          <button type="submit" disabled={creating} className="zf-btn zf-btn-primary flex-1">{creating ? 'Creating…' : 'Create'}</button>
        </div>
      </form>
    </Modal>
  )
}

function AttachVolumeModal({ volume, onClose, onAttached }: { volume: VolumeRow; onClose: () => void; onAttached: () => void }) {
  const toast = useToastContext()
  const [vmName, setVmName] = useState('')
  const [attaching, setAttaching] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!vmName.trim()) { toast.error('VM name is required'); return }
    setAttaching(true)
    try {
      await attachVolume(volume.pool, volume.id, { vm_name: vmName.trim() })
      toast.success(`Marked '${volume.name}' attached to '${vmName.trim()}'`)
      onAttached()
    } catch (err) {
      toastFailure(toast, 'Failed to attach volume', err)
    } finally {
      setAttaching(false)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-1 text-[var(--zf-ink)]">Attach Volume</h2>
      <p className="text-sm text-[var(--zf-muted)] mb-4">{volume.name} ({volume.pool})</p>
      <p className="text-xs text-amber-800 bg-amber-50 border border-amber-200 rounded-lg px-3 py-2 mb-4">
        This only updates the tracking record — it does not attach a real disk to the VM's configuration.
      </p>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">VM Name</label>
          <input aria-label="VM Name" value={vmName} onChange={e => setVmName(e.target.value)} className="input-field" placeholder="my-vm" />
        </div>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
          <button type="submit" disabled={attaching} className="zf-btn zf-btn-primary flex-1">{attaching ? 'Attaching…' : 'Attach'}</button>
        </div>
      </form>
    </Modal>
  )
}

function ResizeVolumeModal({ volume, onClose, onResized }: { volume: VolumeRow; onClose: () => void; onResized: () => void }) {
  const toast = useToastContext()
  const [size, setSize] = useState(volume.size)
  const [resizing, setResizing] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!size.trim()) { toast.error('Size is required'); return }
    setResizing(true)
    try {
      await resizeVolume(volume.pool, volume.id, { size: size.trim() })
      toast.success(`Volume record '${volume.name}' resized`)
      onResized()
    } catch (err) {
      toastFailure(toast, 'Failed to resize volume record', err)
    } finally {
      setResizing(false)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-1 text-[var(--zf-ink)]">Resize Volume Record</h2>
      <p className="text-sm text-[var(--zf-muted)] mb-4">{volume.name} ({volume.pool})</p>
      <p className="text-xs text-amber-800 bg-amber-50 border border-amber-200 rounded-lg px-3 py-2 mb-4">
        This only updates the tracking record — it does not resize a real disk image.
      </p>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">New Size</label>
          <input aria-label="New Size" value={size} onChange={e => setSize(e.target.value)} className="input-field" placeholder="40GB" />
        </div>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
          <button type="submit" disabled={resizing} className="zf-btn zf-btn-primary flex-1">{resizing ? 'Saving…' : 'Save'}</button>
        </div>
      </form>
    </Modal>
  )
}
