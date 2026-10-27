// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Plus, ChevronRight, ChevronDown, Trash2, Server, Wrench, Pencil } from 'lucide-react'
import {
  listDatacenters,
  createDatacenter,
  updateDatacenter,
  deleteDatacenter,
  getDatacenterSummary,
  listClusters,
  createCluster,
  updateCluster,
  deleteCluster,
  listHosts,
  registerHost,
  removeHost,
  hostEnterMaintenance,
  hostExitMaintenance,
  type Datacenter,
  type Cluster,
  type HostInfo,
  type DatacenterSummary,
} from '../api/datacenter'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import { PageHeader, Modal } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'
import { toastFailure } from '../utils/toastError'

export default function Datacenters() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [datacenters, setDatacenters] = useState<Datacenter[]>([])
  const [clusters, setClusters] = useState<Cluster[]>([])
  const [hosts, setHosts] = useState<HostInfo[]>([])
  const [summaries, setSummaries] = useState<Map<string, DatacenterSummary>>(new Map())
  const { loading, loadError, run } = usePageLoader('Failed to load datacenters')
  const [expandedDCs, setExpandedDCs] = useState<Set<string>>(new Set())
  const [expandedClusters, setExpandedClusters] = useState<Set<string>>(new Set())
  const [showCreateDC, setShowCreateDC] = useState(false)
  const [showCreateCluster, setShowCreateCluster] = useState<string | null>(null)
  const [showRegisterHost, setShowRegisterHost] = useState<string | null>(null)
  const [editDC, setEditDC] = useState<Datacenter | null>(null)
  const [editCluster, setEditCluster] = useState<Cluster | null>(null)

  const loadData = useCallback(() => {
    return run(async () => {
      const [dcs, cls, hs] = await Promise.all([
        listDatacenters(),
        listClusters(),
        listHosts(),
      ])
      setDatacenters(dcs)
      setClusters(cls)
      setHosts(hs)

      const sums = new Map<string, DatacenterSummary>()
      for (const dc of dcs) {
        try {
          const s = await getDatacenterSummary(dc.id)
          sums.set(dc.id, s)
        } catch { /* skip */ }
      }
      setSummaries(sums)
    })
  }, [run])

  useEffect(() => {
    void loadData()
  }, [loadData])

  const toggleDC = (id: string) => {
    setExpandedDCs(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const toggleCluster = (id: string) => {
    setExpandedClusters(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const handleDeleteDC = async (id: string) => {
    if (!await confirm('Delete Datacenter', 'Delete this datacenter?')) return
    try {
      await deleteDatacenter(id)
      toast.success('Datacenter deleted')
      loadData()
    } catch { toast.error('Failed to delete datacenter') }
  }

  const handleDeleteCluster = async (id: string) => {
    if (!await confirm('Delete Cluster', 'Delete this cluster?')) return
    try {
      await deleteCluster(id)
      toast.success('Cluster deleted')
      loadData()
    } catch { toast.error('Failed to delete cluster') }
  }

  const handleSaveDC = async (id: string, name: string, description: string) => {
    try {
      await updateDatacenter(id, { name, description: description || undefined })
      toast.success('Datacenter updated')
      setEditDC(null)
      loadData()
    } catch (err) { toastFailure(toast, 'Failed to update datacenter', err) }
  }

  const handleSaveCluster = async (id: string, name: string, description: string) => {
    try {
      await updateCluster(id, { name, description: description || undefined })
      toast.success('Cluster updated')
      setEditCluster(null)
      loadData()
    } catch (err) { toastFailure(toast, 'Failed to update cluster', err) }
  }

  const handleRemoveHost = async (id: string) => {
    if (!await confirm('Remove Host', 'Remove this host?')) return
    try {
      await removeHost(id)
      toast.success('Host removed')
      loadData()
    } catch { toast.error('Failed to remove host') }
  }

  const handleToggleMaintenance = async (host: HostInfo) => {
    try {
      if (host.status === 'Maintenance') {
        await hostExitMaintenance(host.id)
        toast.success('Host exited maintenance mode')
      } else {
        await hostEnterMaintenance(host.id)
        toast.success('Host entering maintenance mode')
      }
      loadData()
    } catch { toast.error('Failed to toggle maintenance mode') }
  }

  const getStatusBadge = (status: string) => {
    const colors: Record<string, string> = {
      Connected: 'text-emerald-700 bg-emerald-50 border-emerald-200',
      Disconnected: 'text-red-700 bg-red-50 border-red-200',
      Maintenance: 'text-amber-800 bg-amber-50 border-amber-200',
      Active: 'text-emerald-700 bg-emerald-50 border-emerald-200',
    }
    return colors[status] || 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
  }

  return (
    <div className="p-6">
      <PageHeader
        title="Datacenters"
        onRefresh={() => void loadData()}
        refreshing={loading}
        actions={
          <button
            onClick={() => setShowCreateDC(true)}
            className="zf-btn zf-btn-primary"
          >
            <Plus className="w-4 h-4" />
            Create Datacenter
          </button>
        }
      />
      <PageLoadBanner
        title="Could not load datacenters"
        headline={loadError}
        onRetry={() => void loadData()}
      />

      {loading && !loadError ? (
        <div className="text-center py-8 text-[var(--zf-muted)]">Loading…</div>
      ) : !loadError ? (
      <>
      {/* Summary Cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-3 mb-4">
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Datacenters</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{datacenters.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Clusters</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{clusters.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Hosts</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{hosts.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Total VMs</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">
            {hosts.reduce((s, h) => s + h.vm_count, 0)}
          </div>
        </div>
      </div>

      {/* Tree View */}
      <div className="zf-panel">
        {datacenters.length === 0 ? (
          <div className="text-center py-12 text-[var(--zf-muted)]">
            No datacenters configured. Create one to get started.
          </div>
        ) : (
          datacenters.map(dc => {
            const dcClusters = clusters.filter(c => c.datacenter_id === dc.id)
            const summary = summaries.get(dc.id)
            const isExpanded = expandedDCs.has(dc.id)

            return (
              <div key={dc.id} className="border-b border-[var(--zf-hairline)] last:border-b-0">
                <div
                  className="flex items-center justify-between p-4 hover:bg-black/[0.03] cursor-pointer"
                  onClick={() => toggleDC(dc.id)}
                >
                  <div className="flex items-center gap-3 min-w-0 flex-1">
                    {isExpanded ? <ChevronDown className="w-5 h-5 shrink-0 text-[var(--zf-muted)]" /> : <ChevronRight className="w-5 h-5 shrink-0 text-[var(--zf-muted)]" />}
                    <Server className="w-5 h-5 text-[var(--zf-link)] shrink-0" />
                    <span className="font-semibold text-lg truncate text-[var(--zf-ink)]">{dc.name}</span>
                    {summary && (
                      <span className="text-sm text-[var(--zf-muted)] ml-2 truncate">
                        {summary.cluster_count} clusters, {summary.host_count} hosts, {summary.vm_count} VMs
                        {summary.total_cpus > 0 && ` | ${summary.total_cpus} CPUs`}
                        {summary.total_memory_mb > 0 && ` | ${(summary.total_memory_mb / 1024).toFixed(1)} GB RAM`}
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-2 shrink-0" onClick={e => e.stopPropagation()}>
                    <button
                      onClick={() => setShowCreateCluster(dc.id)}
                      className="text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] text-sm px-2 py-1"
                    >
                      + Cluster
                    </button>
                    <button
                      onClick={() => setEditDC(dc)}
                      className="p-1 text-[var(--zf-muted)] hover:text-[var(--zf-ink)]"
                      title="Edit datacenter"
                    >
                      <Pencil className="w-4 h-4" />
                    </button>
                    <button
                      onClick={() => handleDeleteDC(dc.id)}
                      className="text-[var(--zf-danger)] hover:opacity-70 p-1" aria-label="Delete" title="Delete">
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </div>

                {isExpanded && dcClusters.map(cl => {
                  const clHosts = hosts.filter(h => h.cluster_id === cl.id)
                  const clExpanded = expandedClusters.has(cl.id)

                  return (
                    <div key={cl.id} className="ml-8">
                      <div
                        className="flex items-center justify-between p-3 hover:bg-black/[0.03] cursor-pointer border-t border-[var(--zf-hairline)]"
                        onClick={() => toggleCluster(cl.id)}
                      >
                        <div className="flex items-center gap-3 min-w-0 flex-1">
                          {clExpanded ? <ChevronDown className="w-4 h-4 shrink-0 text-[var(--zf-muted)]" /> : <ChevronRight className="w-4 h-4 shrink-0 text-[var(--zf-muted)]" />}
                          <span className="font-medium truncate text-[var(--zf-ink)]">{cl.name}</span>
                          <span className="text-xs text-[var(--zf-muted)] truncate">
                            {clHosts.length} hosts |
                            HA: {cl.ha_enabled ? 'On' : 'Off'} |
                            DRS: {cl.drs_enabled ? cl.drs_mode : 'Off'}
                          </span>
                        </div>
                        <div className="flex items-center gap-2 shrink-0" onClick={e => e.stopPropagation()}>
                          <button
                            onClick={() => setShowRegisterHost(cl.id)}
                            className="text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] text-sm px-2 py-1"
                          >
                            + Host
                          </button>
                          <button
                            onClick={() => setEditCluster(cl)}
                            className="p-1 text-[var(--zf-muted)] hover:text-[var(--zf-ink)]"
                            title="Edit cluster"
                          >
                            <Pencil className="w-4 h-4" />
                          </button>
                          <button
                            onClick={() => handleDeleteCluster(cl.id)}
                            className="text-[var(--zf-danger)] hover:opacity-70 p-1" aria-label="Delete" title="Delete">
                            <Trash2 className="w-4 h-4" />
                          </button>
                        </div>
                      </div>

                      {clExpanded && clHosts.length > 0 && (
                        <div className="ml-8 border-t border-[var(--zf-hairline)]">
                          <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
                            <thead>
                              <tr className="text-left text-xs text-[var(--zf-muted)]">
                                <th className="p-2">Hostname</th>
                                <th className="p-2">Address</th>
                                <th className="p-2">CPUs</th>
                                <th className="p-2">Memory</th>
                                <th className="p-2">CPU Usage</th>
                                <th className="p-2">Mem Usage</th>
                                <th className="p-2">VMs</th>
                                <th className="p-2">Secure Containers</th>
                                <th className="p-2">Status</th>
                                <th className="p-2">Actions</th>
                              </tr>
                            </thead>
                            <tbody className="divide-y divide-[var(--zf-hairline)]">
                              {clHosts.map(host => (
                                <tr key={host.id} className="hover:bg-black/[0.03]">
                                  <td className="p-2 font-medium text-[var(--zf-ink)]">{host.hostname}</td>
                                  <td className="p-2 font-mono text-sm text-[var(--zf-muted)]">{host.address}</td>
                                  <td className="p-2 text-sm text-[var(--zf-ink)]">{host.cpus}</td>
                                  <td className="p-2 text-sm text-[var(--zf-ink)]">{(host.memory_mb / 1024).toFixed(1)} GB</td>
                                  <td className="p-2">
                                    <div className="flex items-center gap-2">
                                      <div className="w-16 bg-[var(--zf-canvas)] rounded-full h-2">
                                        <div
                                          className={`h-2 rounded-full ${host.cpu_usage_pct > 80 ? 'bg-[var(--zf-danger)]' : host.cpu_usage_pct > 60 ? 'bg-[var(--zf-warning)]' : 'bg-[var(--zf-success)]'}`}
                                          style={{ width: `${host.cpu_usage_pct}%` }}
                                        />
                                      </div>
                                      <span className="text-xs text-[var(--zf-muted)]">{host.cpu_usage_pct}%</span>
                                    </div>
                                  </td>
                                  <td className="p-2">
                                    <div className="flex items-center gap-2">
                                      <div className="w-16 bg-[var(--zf-canvas)] rounded-full h-2">
                                        <div
                                          className={`h-2 rounded-full ${host.memory_usage_pct > 80 ? 'bg-[var(--zf-danger)]' : host.memory_usage_pct > 60 ? 'bg-[var(--zf-warning)]' : 'bg-[var(--zf-success)]'}`}
                                          style={{ width: `${host.memory_usage_pct}%` }}
                                        />
                                      </div>
                                      <span className="text-xs text-[var(--zf-muted)]">{host.memory_usage_pct}%</span>
                                    </div>
                                  </td>
                                  <td className="p-2 text-sm text-[var(--zf-ink)]">{host.vm_count}</td>
                                  <td className="p-2">
                                    <span
                                      className={`px-2 py-1 rounded-full text-xs font-medium border ${
                                        host.secure_containers_ready || host.secure_containers?.available
                                          ? 'text-emerald-700 bg-emerald-50 border-emerald-200'
                                          : 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
                                      }`}
                                      title={
                                        host.secure_containers
                                          ? `available=${host.secure_containers.available} shim=${host.secure_containers.shim_installed} guest_image=${host.secure_containers.guest_image_present}`
                                          : host.secure_containers_ready
                                            ? 'ready'
                                            : 'not ready'
                                      }
                                    >
                                      {host.secure_containers_ready || host.secure_containers?.available
                                        ? 'ready'
                                        : 'no'}
                                      {host.secure_containers && (
                                        <span className="ml-1 opacity-70">
                                          (shim {host.secure_containers.shim_installed ? '✓' : '✗'} · img{' '}
                                          {host.secure_containers.guest_image_present ? '✓' : '✗'})
                                        </span>
                                      )}
                                    </span>
                                  </td>
                                  <td className="p-2">
                                    <span className={`px-2 py-1 rounded-full text-xs font-medium border ${getStatusBadge(host.status)}`}>
                                      {host.status}
                                    </span>
                                  </td>
                                  <td className="p-2">
                                    <div className="flex items-center gap-1">
                                      <button
                                        onClick={() => handleToggleMaintenance(host)}
                                        className="p-1 hover:bg-black/[0.04] rounded"
                                        title={host.status === 'Maintenance' ? 'Exit maintenance' : 'Enter maintenance'}
                                      >
                                        <Wrench className={`w-4 h-4 ${host.status === 'Maintenance' ? 'text-[var(--zf-warning)]' : 'text-[var(--zf-muted)]'}`} />
                                      </button>
                                      <button
                                        onClick={() => handleRemoveHost(host.id)}
                                        className="text-[var(--zf-danger)] hover:opacity-70 p-1" aria-label="Delete" title="Delete">
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

                      {clExpanded && clHosts.length === 0 && (
                        <div className="ml-8 p-4 text-[var(--zf-muted)] text-sm">No hosts registered in this cluster.</div>
                      )}
                    </div>
                  )
                })}

                {isExpanded && dcClusters.length === 0 && (
                  <div className="ml-8 p-4 text-[var(--zf-muted)] text-sm">No clusters in this datacenter.</div>
                )}
              </div>
            )
          })
        )}
      </div>
      </>
      ) : null}

      {/* Create Datacenter Modal */}
      {showCreateDC && (
        <CreateDCModal
          onClose={() => setShowCreateDC(false)}
          onCreated={() => { setShowCreateDC(false); loadData() }}
        />
      )}

      {/* Create Cluster Modal */}
      {showCreateCluster && (
        <CreateClusterModal
          datacenterId={showCreateCluster}
          onClose={() => setShowCreateCluster(null)}
          onCreated={() => { setShowCreateCluster(null); loadData() }}
        />
      )}

      {/* Register Host Modal */}
      {showRegisterHost && (
        <RegisterHostModal
          clusterId={showRegisterHost}
          onClose={() => setShowRegisterHost(null)}
          onCreated={() => { setShowRegisterHost(null); loadData() }}
        />
      )}

      {/* Edit Datacenter Modal */}
      {editDC && (
        <EditNameModal
          title="Edit Datacenter"
          name={editDC.name}
          description={editDC.description || ''}
          onSave={(name, description) => handleSaveDC(editDC.id, name, description)}
          onClose={() => setEditDC(null)}
        />
      )}

      {/* Edit Cluster Modal */}
      {editCluster && (
        <EditNameModal
          title="Edit Cluster"
          name={editCluster.name}
          description={editCluster.description || ''}
          onSave={(name, description) => handleSaveCluster(editCluster.id, name, description)}
          onClose={() => setEditCluster(null)}
        />
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

function CreateDCModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await createDatacenter({ name, description: description || undefined })
      onCreated()
    } catch { toast.error('Failed to create datacenter') }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Create Datacenter</h2>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Name</label>
          <input aria-label="Name" type="text" value={name} onChange={e => setName(e.target.value)}
            className="input-field" required />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Description</label>
          <input aria-label="Description" type="text" value={description} onChange={e => setDescription(e.target.value)}
            className="input-field" />
        </div>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
          <button type="submit" className="flex-1 zf-btn zf-btn-primary">Create</button>
        </div>
      </form>
    </Modal>
  )
}

function CreateClusterModal({ datacenterId, onClose, onCreated }: { datacenterId: string; onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await createCluster({ name, datacenter_id: datacenterId, description: description || undefined })
      onCreated()
    } catch { toast.error('Failed to create cluster') }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Create Cluster</h2>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Cluster Name</label>
          <input aria-label="Cluster Name" type="text" value={name} onChange={e => setName(e.target.value)}
            className="input-field" required />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Description</label>
          <input aria-label="Description" type="text" value={description} onChange={e => setDescription(e.target.value)}
            className="input-field" />
        </div>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
          <button type="submit" className="flex-1 zf-btn zf-btn-primary">Create</button>
        </div>
      </form>
    </Modal>
  )
}

function RegisterHostModal({ clusterId, onClose, onCreated }: { clusterId: string; onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [hostname, setHostname] = useState('')
  const [address, setAddress] = useState('')
  const [cpus, setCpus] = useState(4)
  const [memoryMb, setMemoryMb] = useState(8192)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await registerHost({ hostname, address, cluster_id: clusterId, cpus, memory_mb: memoryMb })
      onCreated()
    } catch { toast.error('Failed to register host') }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Register Host</h2>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Hostname</label>
          <input aria-label="Hostname" type="text" value={hostname} onChange={e => setHostname(e.target.value)}
            className="input-field" required />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">IP Address</label>
          <input aria-label="IP Address" type="text" value={address} onChange={e => setAddress(e.target.value)}
            className="input-field" required />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">CPUs</label>
            <input aria-label="CPUs" type="number" value={cpus} onChange={e => setCpus(Number(e.target.value))}
              className="input-field" min={1} required />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Memory (MB)</label>
            <input aria-label="Memory (MB)" type="number" value={memoryMb} onChange={e => setMemoryMb(Number(e.target.value))}
              className="input-field" min={512} required />
          </div>
        </div>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
          <button type="submit" className="flex-1 zf-btn zf-btn-primary">Register</button>
        </div>
      </form>
    </Modal>
  )
}

function EditNameModal({ title, name: initialName, description: initialDescription, onSave, onClose }: {
  title: string; name: string; description: string; onSave: (name: string, description: string) => void; onClose: () => void
}) {
  const [name, setName] = useState(initialName)
  const [description, setDescription] = useState(initialDescription)

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    onSave(name, description)
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">{title}</h2>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Name</label>
          <input aria-label="Name" type="text" value={name} onChange={e => setName(e.target.value)}
            className="input-field" required />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Description</label>
          <input aria-label="Description" type="text" value={description} onChange={e => setDescription(e.target.value)}
            className="input-field" />
        </div>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
          <button type="submit" className="flex-1 zf-btn zf-btn-primary">Save</button>
        </div>
      </form>
    </Modal>
  )
}
