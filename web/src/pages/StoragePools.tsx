// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PageLoading } from '../components/Skeleton'
import { useState, useEffect } from 'react'
import { HardDrive, Plus, Play, Square, Trash2, RefreshCw, Server, Folder, AlertCircle, CheckCircle, Database, X } from 'lucide-react'
import {
  listStoragePools,
  createNfsPool,
  createLocalPool,
  createCephPool,
  deleteStoragePool,
  startStoragePool,
  stopStoragePool,
  refreshPoolStats,
  getNfsHealth,
  getCephHealth,
  listRbdImages,
  createRbdImage,
  deleteRbdImage,
  type StoragePool,
  type NfsHealth,
} from '../api/storage'
import { useConfirm } from '../hooks/useConfirm'
import { useToastContext } from '../contexts/ToastContext'
import ConfirmDialog from '../components/ConfirmDialog'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, Card, Modal } from '../components/ui'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import CopyButton from '../components/CopyButton'
import TableSearch from '../components/TableSearch'
import { useTableFilter } from '../hooks/useTableFilter'

export default function StoragePools() {
  const { confirmState, confirm, cancel } = useConfirm()
  const toast = useToastContext()
  const [pools, setPools] = useState<StoragePool[]>([])
  const [loading, setLoading] = useState(true)
  const [showCreateDialog, setShowCreateDialog] = useState(false)
  const [nfsHealth, setNfsHealth] = useState<Map<string, NfsHealth>>(new Map())
  const [cephHealth, setCephHealth] = useState<Map<string, { status: string; detail: string }>>(new Map())
  const [loadError, setLoadError] = useState<string | null>(null)
  const [rbdPool, setRbdPool] = useState<StoragePool | null>(null)
  const { query: poolQuery, setQuery: setPoolQuery, filtered: filteredPools } = useTableFilter(pools, (p) => [p.name, p.path, p.state])

  useEffect(() => {
    loadPools()
  }, [])

  const loadPools = async () => {
    setLoadError(null)
    try {
      const data = await listStoragePools()
      setPools(data)

      // Load health for NFS and Ceph pools
      for (const pool of data) {
        if (typeof pool.pool_type === 'object' && 'NFS' in pool.pool_type) {
          try {
            const health = await getNfsHealth(pool.name)
            setNfsHealth((prev) => new Map(prev).set(pool.name, health))
          } catch {
            // Health is optional; skip toast per pool to avoid noise
          }
        }
        if (typeof pool.pool_type === 'object' && 'Ceph' in pool.pool_type) {
          try {
            const health = await getCephHealth(pool.name)
            setCephHealth((prev) => new Map(prev).set(pool.name, health))
          } catch {
            // Health is optional; skip toast per pool to avoid noise
          }
        }
      }
    } catch (error) {
      const msg = formatUserError(error)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load storage pools', error)
    } finally {
      setLoading(false)
    }
  }

  const handleStart = async (name: string) => {
    try {
      await startStoragePool(name)
      await loadPools()
    } catch (error) {
      toastFailure(toast, 'Failed to start pool', error)
    }
  }

  const handleStop = async (name: string) => {
    try {
      await stopStoragePool(name)
      await loadPools()
    } catch (error) {
      toastFailure(toast, 'Failed to stop pool', error)
    }
  }

  const handleDelete = async (name: string) => {
    const ok = await confirm('Delete Storage Pool', `Are you sure you want to delete storage pool "${name}"?`, { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return

    try {
      await deleteStoragePool(name)
      await loadPools()
    } catch (error) {
      toastFailure(toast, 'Failed to delete pool', error)
    }
  }

  const handleRefresh = async (name: string) => {
    try {
      await refreshPoolStats(name)
      await loadPools()
    } catch (error) {
      toastFailure(toast, 'Failed to refresh pool stats', error)
    }
  }

  const formatBytes = (bytes: number) => {
    const gb = bytes / (1024 * 1024 * 1024)
    if (gb >= 1024) {
      return `${(gb / 1024).toFixed(2)} TB`
    }
    return `${gb.toFixed(2)} GB`
  }

  const getPoolTypeDisplay = (pool: StoragePool) => {
    if (pool.pool_type === 'Local') return 'Local'
    if (pool.pool_type === 'Directory') return 'Directory'
    if (typeof pool.pool_type === 'object') {
      if ('NFS' in pool.pool_type) {
        const nfs = pool.pool_type.NFS
        return `NFS: ${nfs.server}:${nfs.export_path}`
      }
      if ('LVM' in pool.pool_type) {
        return `LVM: ${pool.pool_type.LVM.volume_group}`
      }
      if ('LVMThin' in pool.pool_type) {
        return `LVM-thin: ${pool.pool_type.LVMThin.volume_group}/${pool.pool_type.LVMThin.thin_pool}`
      }
      if ('ZFS' in pool.pool_type) {
        const zfs = pool.pool_type.ZFS
        return `ZFS: ${zfs.zpool}${zfs.dataset ? '/' + zfs.dataset : ''}`
      }
      if ('Ceph' in pool.pool_type) {
        const ceph = pool.pool_type.Ceph
        return `Ceph: ${ceph.pool_name} (${ceph.monitors.length} mons)`
      }
    }
    return 'Unknown'
  }

  const getStateColor = (state: string) => {
    switch (state) {
      case 'Active':
        return 'text-emerald-700'
      case 'Inactive':
        return 'text-[var(--zf-muted)]'
      case 'Starting':
      case 'Stopping':
        return 'text-amber-800'
      case 'Degraded':
        return 'text-[var(--zf-warning)]'
      case 'Failed':
        return 'text-[var(--zf-danger)]'
      default:
        return 'text-[var(--zf-muted)]'
    }
  }

  const getHealthIcon = (health?: NfsHealth) => {
    if (!health) return null

    if (health.status === 'Healthy') {
      return <CheckCircle className="w-4 h-4 text-emerald-600" />
    }
    return <AlertCircle className="w-4 h-4 text-[var(--zf-danger)]" />
  }

  if (loading) {
    return <PageLoading />
  }

  return (
    <div className="p-8">
      {loadError && (
        <ErrorBanner
          title="Could not load storage pools"
          headline={loadError}
          hints={hintsForError(loadError, 'storage')}
          onRetry={loadPools}
        />
      )}
      <PageHeader
        title="Storage Pools"
        description="Manage local and network storage pools"
        primaryAction={
          <button
            onClick={() => setShowCreateDialog(true)}
            className="zf-btn zf-btn-primary"
          >
            <Plus className="w-4 h-4" />
            Create Pool
          </button>
        }
      />

      {/* Statistics */}
      <div className="grid grid-cols-4 gap-3 mb-6">
        <Card><div className="px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Total Pools</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{pools.length}</div>
        </div></Card>
        <Card><div className="px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Active Pools</div>
          <div className="text-2xl font-bold text-emerald-700">
            {pools.filter((p) => p.state === 'Active').length}
          </div>
        </div></Card>
        <Card><div className="px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Total Capacity</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">
            {formatBytes(pools.reduce((sum, p) => sum + p.capacity, 0))}
          </div>
        </div></Card>
        <Card><div className="px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Available</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">
            {formatBytes(pools.reduce((sum, p) => sum + p.available, 0))}
          </div>
        </div></Card>
      </div>

      {/* Pools List */}
      <Card className="overflow-hidden">
        {pools.length > 0 && (
          <div className="p-4 border-b border-[var(--zf-hairline)]">
            <TableSearch
              value={poolQuery}
              onChange={setPoolQuery}
              placeholder="Search by name, path, state..."
              resultCount={filteredPools.length}
              totalCount={pools.length}
              className="max-w-sm"
            />
          </div>
        )}
        <table className="w-full">
          <thead className="bg-[var(--zf-surface)]">
            <tr className="text-left text-[var(--zf-muted)] text-sm">
              <th className="p-4">Name</th>
              <th className="p-4">Type</th>
              <th className="p-4">Path</th>
              <th className="p-4">Capacity</th>
              <th className="p-4">Available</th>
              <th className="p-4">Usage</th>
              <th className="p-4">State</th>
              <th className="p-4">Health</th>
              <th className="p-4">Actions</th>
            </tr>
          </thead>
          <tbody>
            {pools.length === 0 ? (
              <tr>
                <td colSpan={9} className="p-8 text-center text-[var(--zf-muted)]">
                  No storage pools configured. Create one to get started.
                </td>
              </tr>
            ) : filteredPools.length === 0 ? (
              <tr>
                <td colSpan={9} className="p-8 text-center text-[var(--zf-muted)]">
                  No storage pools match "{poolQuery}"
                </td>
              </tr>
            ) : (
              filteredPools.map((pool) => {
                const usagePercent =
                  pool.capacity > 0 ? ((pool.capacity - pool.available) / pool.capacity) * 100 : 0

                return (
                  <tr key={pool.id} className="border-t border-[var(--zf-hairline)] hover:bg-black/[0.03]">
                    <td className="p-4">
                      <div className="flex items-center gap-2">
                        {typeof pool.pool_type === 'object' && 'NFS' in pool.pool_type ? (
                          <Server className="w-4 h-4 text-[var(--zf-link)]" />
                        ) : (
                          <Folder className="w-4 h-4 text-[var(--zf-muted)]" />
                        )}
                        <span className="font-medium text-[var(--zf-ink)]">{pool.name}</span>
                      </div>
                    </td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">{getPoolTypeDisplay(pool)}</td>
                    <td className="p-4 text-sm text-[var(--zf-muted)] font-mono">
                      <div className="flex items-center gap-1.5">
                        <span className="truncate max-w-[16rem]">{pool.path}</span>
                        <CopyButton text={pool.path} iconOnly successMessage="Path copied" />
                      </div>
                    </td>
                    <td className="p-4 text-sm text-[var(--zf-ink)]">{formatBytes(pool.capacity)}</td>
                    <td className="p-4 text-sm text-[var(--zf-ink)]">{formatBytes(pool.available)}</td>
                    <td className="p-4">
                      <div className="flex items-center gap-2">
                        <div className="flex-1 bg-[var(--zf-canvas)] rounded-full h-2 overflow-hidden">
                          <div
                            className={`h-full ${
                              usagePercent > 90
                                ? 'bg-[var(--zf-danger)]'
                                : usagePercent > 75
                                ? 'bg-[var(--zf-warning)]'
                                : 'bg-[var(--zf-success)]'
                            }`}
                            style={{ width: `${Math.min(usagePercent, 100)}%` }}
                          />
                        </div>
                        <span className="text-sm text-[var(--zf-muted)] w-12 text-right">
                          {usagePercent.toFixed(0)}%
                        </span>
                      </div>
                    </td>
                    <td className="p-4">
                      <span className={`text-sm font-medium ${getStateColor(pool.state)}`}>
                        {pool.state}
                      </span>
                    </td>
                    <td className="p-4">
                      {getHealthIcon(nfsHealth.get(pool.name))}
                      {cephHealth.has(pool.name) && (
                        cephHealth.get(pool.name)?.status === 'Ok'
                          ? <CheckCircle className="w-4 h-4 text-emerald-600" />
                          : cephHealth.get(pool.name)?.status === 'Warn'
                          ? <AlertCircle className="w-4 h-4 text-amber-600" />
                          : <AlertCircle className="w-4 h-4 text-[var(--zf-danger)]" />
                      )}
                    </td>
                    <td className="p-4">
                      <div className="flex items-center gap-2">
                        {pool.state === 'Inactive' ? (
                          <button
                            onClick={() => handleStart(pool.name)}
                            className="p-2 hover:bg-black/[0.04] rounded transition"
                            title="Start pool"
                          >
                            <Play className="w-4 h-4 text-emerald-600" />
                          </button>
                        ) : pool.state === 'Active' ? (
                          <button
                            onClick={() => handleStop(pool.name)}
                            className="p-2 hover:bg-black/[0.04] rounded transition"
                            title="Stop pool"
                          >
                            <Square className="w-4 h-4 text-amber-600" />
                          </button>
                        ) : null}
                        {typeof pool.pool_type === 'object' && 'Ceph' in pool.pool_type && (
                          <button
                            onClick={() => setRbdPool(pool)}
                            className="p-2 hover:bg-black/[0.04] rounded transition"
                            title="Manage RBD images"
                          >
                            <Database className="w-4 h-4 text-[var(--zf-muted)]" />
                          </button>
                        )}
                        <button
                          onClick={() => handleRefresh(pool.name)}
                          className="p-2 hover:bg-black/[0.04] rounded transition"
                          title="Refresh stats"
                        >
                          <RefreshCw className="w-4 h-4 text-[var(--zf-link)]" />
                        </button>
                        <button
                          onClick={() => handleDelete(pool.name)}
                          className="p-2 hover:bg-black/[0.04] rounded transition"
                          title="Delete pool"
                          disabled={pool.state === 'Active'}
                        >
                          <Trash2
                            className={`w-4 h-4 ${
                              pool.state === 'Active' ? 'text-[var(--zf-muted)]' : 'text-[var(--zf-danger)]'
                            }`}
                          />
                        </button>
                      </div>
                    </td>
                  </tr>
                )
              })
            )}
          </tbody>
        </table>
      </Card>

      {/* Create Pool Dialog */}
      {showCreateDialog && (
        <CreatePoolDialog onClose={() => setShowCreateDialog(false)} onCreated={loadPools} />
      )}

      {rbdPool && (
        <RbdImagesModal pool={rbdPool} onClose={() => setRbdPool(null)} />
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

interface CreatePoolDialogProps {
  onClose: () => void
  onCreated: () => void
}

function CreatePoolDialog({ onClose, onCreated }: CreatePoolDialogProps) {
  const toast = useToastContext()
  const [poolType, setPoolType] = useState<'local' | 'nfs' | 'lvm' | 'lvm-thin' | 'zfs' | 'ceph'>('local')
  const [name, setName] = useState('')
  const [path, setPath] = useState('')
  const [autoStart, setAutoStart] = useState(true)

  // NFS specific
  const [nfsServer, setNfsServer] = useState('')
  const [nfsExportPath, setNfsExportPath] = useState('')
  const [nfsMountPath, setNfsMountPath] = useState('')
  const [nfsVersion, setNfsVersion] = useState<'V4' | 'V3' | 'V4_1' | 'V4_2'>('V4')
  const [mountOptions, setMountOptions] = useState('rw,hard,intr')

  // LVM specific
  const [volumeGroup, setVolumeGroup] = useState('')
  const [thinPool, setThinPool] = useState('')

  // ZFS specific
  const [zpool, setZpool] = useState('')
  const [dataset, setDataset] = useState('')

  // Ceph specific
  const [cephMonitors, setCephMonitors] = useState('')
  const [cephPoolName, setCephPoolName] = useState('')
  const [cephUser, setCephUser] = useState('')
  const [cephKeyring, setCephKeyring] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    const { createLvmPool, createLvmThinPool, createZfsPool } = await import('../api/storage')

    try {
      if (poolType === 'local') {
        await createLocalPool({ name, path, auto_start: autoStart })
      } else if (poolType === 'nfs') {
        await createNfsPool({
          name,
          config: {
            server: nfsServer,
            export_path: nfsExportPath,
            mount_path: nfsMountPath,
            mount_options: mountOptions.split(',').map((o) => o.trim()),
            auto_start: autoStart,
            nfs_version: nfsVersion,
          },
        })
      } else if (poolType === 'lvm') {
        await createLvmPool({ name, volume_group: volumeGroup, auto_start: autoStart })
      } else if (poolType === 'lvm-thin') {
        await createLvmThinPool({ name, volume_group: volumeGroup, thin_pool: thinPool, auto_start: autoStart })
      } else if (poolType === 'zfs') {
        await createZfsPool({ name, zpool, dataset: dataset || undefined, auto_start: autoStart })
      } else if (poolType === 'ceph') {
        await createCephPool({
          name,
          monitors: cephMonitors.split(',').map(m => m.trim()).filter(Boolean),
          pool_name: cephPoolName,
          user: cephUser || undefined,
          keyring: cephKeyring || undefined,
          auto_start: autoStart,
        })
      }

      onCreated()
      onClose()
    } catch (error) {
      toastFailure(toast, 'Failed to create pool', error)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-2xl">
      <h2 className="text-2xl font-bold mb-6 text-[var(--zf-ink)]">Create Storage Pool</h2>

      <form onSubmit={handleSubmit}>
        {/* Pool Type */}
        <div className="mb-6">
          <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Pool Type</label>
          <div className="grid grid-cols-3 gap-3">
            {([
              { key: 'local' as const, label: 'Local', desc: 'Local filesystem', Icon: HardDrive },
              { key: 'nfs' as const, label: 'NFS', desc: 'Network file system', Icon: Server },
              { key: 'lvm' as const, label: 'LVM', desc: 'LVM volume group', Icon: HardDrive },
              { key: 'lvm-thin' as const, label: 'LVM-thin', desc: 'Thin provisioned LVM', Icon: HardDrive },
              { key: 'zfs' as const, label: 'ZFS', desc: 'ZFS pool/dataset', Icon: HardDrive },
              { key: 'ceph' as const, label: 'Ceph', desc: 'Ceph RBD pool', Icon: Server },
            ]).map(({ key, label, desc, Icon }) => (
              <button
                key={key}
                type="button"
                onClick={() => setPoolType(key)}
                className={`p-3 rounded border-2 transition text-center ${
                  poolType === key
                    ? 'border-[var(--zf-ink)] bg-black/[0.04]'
                    : 'border-[var(--zf-hairline)] hover:border-[var(--zf-muted)]'
                }`}
              >
                <Icon className="w-5 h-5 mx-auto mb-1 text-[var(--zf-ink)]" />
                <div className="font-medium text-sm text-[var(--zf-ink)]">{label}</div>
                <div className="text-xs text-[var(--zf-muted)]">{desc}</div>
              </button>
            ))}
          </div>
        </div>

        {/* Common Fields */}
        <div className="mb-4">
          <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Pool Name</label>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="input-field"
            placeholder="storage-pool-1"
            required
          />
        </div>

        {poolType === 'local' ? (
          <div className="mb-4">
            <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Local Path</label>
            <input
              type="text"
              value={path}
              onChange={(e) => setPath(e.target.value)}
              className="input-field font-mono"
              placeholder="/var/lib/zyvor-fabricd/storage"
              required
            />
          </div>
        ) : poolType === 'lvm' || poolType === 'lvm-thin' ? (
          <>
            <div className="mb-4">
              <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Volume Group</label>
              <input
                type="text"
                value={volumeGroup}
                onChange={(e) => setVolumeGroup(e.target.value)}
                className="input-field font-mono"
                placeholder="vg0"
                required
              />
            </div>
            {poolType === 'lvm-thin' && (
              <div className="mb-4">
                <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Thin Pool</label>
                <input
                  type="text"
                  value={thinPool}
                  onChange={(e) => setThinPool(e.target.value)}
                  className="input-field font-mono"
                  placeholder="thinpool0"
                  required
                />
              </div>
            )}
          </>
        ) : poolType === 'zfs' ? (
          <>
            <div className="mb-4">
              <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">ZFS Pool</label>
              <input
                type="text"
                value={zpool}
                onChange={(e) => setZpool(e.target.value)}
                className="input-field font-mono"
                placeholder="tank"
                required
              />
            </div>
            <div className="mb-4">
              <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Dataset (optional)</label>
              <input
                type="text"
                value={dataset}
                onChange={(e) => setDataset(e.target.value)}
                className="input-field font-mono"
                placeholder="vms"
              />
            </div>
          </>
        ) : poolType === 'ceph' ? (
          <>
            <div className="mb-4">
              <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Monitor Addresses</label>
              <input
                type="text"
                value={cephMonitors}
                onChange={(e) => setCephMonitors(e.target.value)}
                className="input-field font-mono"
                placeholder="10.0.0.1, 10.0.0.2, 10.0.0.3"
                required
              />
              <div className="text-xs text-[var(--zf-muted)] mt-1">Comma-separated list of Ceph monitor addresses</div>
            </div>
            <div className="mb-4">
              <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Ceph Pool Name</label>
              <input
                type="text"
                value={cephPoolName}
                onChange={(e) => setCephPoolName(e.target.value)}
                className="input-field font-mono"
                placeholder="rbd"
                required
              />
            </div>
            <div className="mb-4">
              <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">User (optional)</label>
              <input
                type="text"
                value={cephUser}
                onChange={(e) => setCephUser(e.target.value)}
                className="input-field font-mono"
                placeholder="admin"
              />
            </div>
            <div className="mb-4">
              <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Keyring Path (optional)</label>
              <input
                type="text"
                value={cephKeyring}
                onChange={(e) => setCephKeyring(e.target.value)}
                className="input-field font-mono"
                placeholder="/etc/ceph/ceph.client.admin.keyring"
              />
            </div>
          </>
        ) : (
          <>
            <div className="mb-4">
              <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">NFS Server</label>
              <input
                type="text"
                value={nfsServer}
                onChange={(e) => setNfsServer(e.target.value)}
                className="input-field"
                placeholder="192.168.1.100"
                required
              />
            </div>
            <div className="mb-4">
              <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Export Path</label>
              <input
                type="text"
                value={nfsExportPath}
                onChange={(e) => setNfsExportPath(e.target.value)}
                className="input-field font-mono"
                placeholder="/export/vm-storage"
                required
              />
            </div>
            <div className="mb-4">
              <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Mount Path</label>
              <input
                type="text"
                value={nfsMountPath}
                onChange={(e) => setNfsMountPath(e.target.value)}
                className="input-field font-mono"
                placeholder="/mnt/nfs-pool"
                required
              />
            </div>
            <div className="mb-4">
              <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">NFS Version</label>
              <select
                value={nfsVersion}
                onChange={(e) => setNfsVersion(e.target.value as any)}
                className="input-field"
              >
                <option value="V3">NFSv3</option>
                <option value="V4">NFSv4</option>
                <option value="V4_1">NFSv4.1</option>
                <option value="V4_2">NFSv4.2</option>
              </select>
            </div>
            <div className="mb-4">
              <label className="block text-sm font-medium mb-2 text-[var(--zf-ink)]">Mount Options</label>
              <input
                type="text"
                value={mountOptions}
                onChange={(e) => setMountOptions(e.target.value)}
                className="input-field font-mono text-sm"
                placeholder="rw,hard,intr,rsize=8192,wsize=8192"
              />
              <div className="text-xs text-[var(--zf-muted)] mt-1">
                Comma-separated mount options
              </div>
            </div>
          </>
        )}

        <div className="mb-6">
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={autoStart}
              onChange={(e) => setAutoStart(e.target.checked)}
              className="rounded border-[var(--zf-hairline)]"
            />
            <span className="text-sm text-[var(--zf-ink)]">Auto-start pool on daemon startup</span>
          </label>
        </div>

        <div className="flex gap-4">
          <button
            type="button"
            onClick={onClose}
            className="zf-btn zf-btn-ghost flex-1"
          >
            Cancel
          </button>
          <button
            type="submit"
            className="zf-btn zf-btn-primary flex-1"
          >
            Create Pool
          </button>
        </div>
      </form>
    </Modal>
  )
}

interface RbdImagesModalProps {
  pool: StoragePool
  onClose: () => void
}

function RbdImagesModal({ pool, onClose }: RbdImagesModalProps) {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [images, setImages] = useState<string[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [newName, setNewName] = useState('')
  const [newSizeMb, setNewSizeMb] = useState(1024)
  const [creating, setCreating] = useState(false)

  const load = async () => {
    setLoadError(null)
    try {
      const data = await listRbdImages(pool.name)
      setImages(data)
    } catch (error) {
      setLoadError(formatUserError(error))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    void load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pool.name])

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!newName.trim()) { toast.error('Image name is required'); return }
    if (newSizeMb <= 0) { toast.error('Size must be greater than 0'); return }
    setCreating(true)
    try {
      await createRbdImage(pool.name, { name: newName.trim(), size_mb: newSizeMb })
      toast.success(`Image '${newName.trim()}' queued for creation`)
      setNewName('')
      setNewSizeMb(1024)
      await load()
    } catch (error) {
      toastFailure(toast, 'Failed to create RBD image', error)
    } finally {
      setCreating(false)
    }
  }

  const handleDelete = async (image: string) => {
    const ok = await confirm('Delete RBD Image', `Delete image '${image}'? This cannot be undone.`, { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return
    try {
      await deleteRbdImage(pool.name, image)
      toast.success(`Image '${image}' deleted`)
      await load()
    } catch (error) {
      toastFailure(toast, 'Failed to delete RBD image', error)
    }
  }

  return (
    <>
      <Modal open onClose={onClose} className="max-w-lg max-h-[85vh] overflow-y-auto">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="text-xl font-bold text-[var(--zf-ink)]">RBD Images</h2>
            <p className="text-sm text-[var(--zf-muted)] mt-0.5">Pool: {pool.name}</p>
          </div>
          <button onClick={onClose} className="p-2 text-[var(--zf-muted)] hover:text-[var(--zf-ink)] hover:bg-black/[0.04] rounded transition">
            <X className="w-5 h-5" />
          </button>
        </div>

        <div className="space-y-4">
          {loadError && (
            <ErrorBanner title="Could not load RBD images" headline={loadError} onRetry={load} />
          )}

          <form onSubmit={handleCreate} className="flex items-end gap-2">
            <div className="flex-1">
              <label className="block text-xs font-medium text-[var(--zf-muted)] mb-1">Image name</label>
              <input
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="my-image"
                className="input-field text-sm"
              />
            </div>
            <div className="w-28">
              <label className="block text-xs font-medium text-[var(--zf-muted)] mb-1">Size (MB)</label>
              <input
                type="number"
                min={1}
                value={newSizeMb}
                onChange={(e) => setNewSizeMb(Number(e.target.value))}
                className="input-field text-sm"
              />
            </div>
            <button
              type="submit"
              disabled={creating}
              className="zf-btn zf-btn-primary zf-btn-sm"
            >
              {creating ? 'Creating…' : 'Create'}
            </button>
          </form>

          {loading ? (
            <div className="flex items-center justify-center py-8">
              <div className="w-5 h-5 border-2 border-[var(--zf-hairline)] border-t-[var(--zf-ink)] rounded-full animate-spin" />
            </div>
          ) : images.length === 0 ? (
            <p className="text-sm text-[var(--zf-muted)] text-center py-6">No RBD images in this pool.</p>
          ) : (
            <div className="space-y-1.5">
              {images.map((image) => (
                <div key={image} className="flex items-center justify-between bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2">
                  <span className="text-sm font-mono truncate text-[var(--zf-ink)]">{image}</span>
                  <button
                    onClick={() => handleDelete(image)}
                    className="p-1.5 text-[var(--zf-muted)] hover:text-[var(--zf-danger)] hover:bg-red-50 rounded transition"
                    title="Delete image"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      </Modal>

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
    </>
  )
}
