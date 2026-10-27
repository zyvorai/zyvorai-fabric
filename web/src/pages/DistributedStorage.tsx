// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Plus, Trash2, ShieldCheck, CheckCircle, XCircle, ArrowRightLeft } from 'lucide-react'
import {
  listDistributedPools,
  createDistributedPool,
  deleteDistributedPool,
  listStoragePolicies,
  createStoragePolicy,
  deleteStoragePolicy,
  listStorageMigrations,
  startStorageMigration,
  listDatastoreClusters,
  createDatastoreCluster,
  checkCompliance,
  type DistributedStoragePool,
  type StoragePolicy,
  type StorageMigration,
  type DatastoreCluster,
  type ComplianceReport,
} from '../api/distributedStorage'
import { listVMs, VM } from '../api/vm'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'
import { PageHeader } from '../components/ui'

export default function DistributedStorage() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [pools, setPools] = useState<DistributedStoragePool[]>([])
  const [policies, setPolicies] = useState<StoragePolicy[]>([])
  const [migrations, setMigrations] = useState<StorageMigration[]>([])
  const [dsClusters, setDsClusters] = useState<DatastoreCluster[]>([])
  const [vms, setVMs] = useState<VM[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load distributed storage')
  const [activeTab, setActiveTab] = useState<'pools' | 'policies' | 'migrations' | 'clusters'>('pools')
  const [showCreatePool, setShowCreatePool] = useState(false)
  const [showCreatePolicy, setShowCreatePolicy] = useState(false)
  const [showCreateCluster, setShowCreateCluster] = useState(false)
  const [showStartMigration, setShowStartMigration] = useState(false)
  const [complianceModalPolicy, setComplianceModalPolicy] = useState<StoragePolicy | null>(null)

  const loadData = useCallback(() => {
    return run(async () => {
      const [p, pol, mig, cl, vm] = await Promise.all([
        listDistributedPools(),
        listStoragePolicies(),
        listStorageMigrations(),
        listDatastoreClusters(),
        listVMs(),
      ])
      setPools(p)
      setPolicies(pol)
      setMigrations(mig)
      setDsClusters(cl)
      setVMs(vm)
    })
  }, [run])

  useEffect(() => {
    void loadData()
  }, [loadData])

  const handleDeletePool = async (id: string) => {
    const ok = await confirm('Delete Pool', 'Delete this storage pool?', { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return
    try {
      await deleteDistributedPool(id)
      toast.success('Storage pool deleted')
      loadData()
    } catch { toast.error('Failed to delete storage pool') }
  }

  const handleDeletePolicy = async (id: string) => {
    const ok = await confirm('Delete Policy', 'Delete this storage policy?', { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return
    try {
      await deleteStoragePolicy(id)
      toast.success('Storage policy deleted')
      loadData()
    } catch { toast.error('Failed to delete storage policy') }
  }

  const formatGB = (gb: number) => gb >= 1024 ? `${(gb / 1024).toFixed(1)} TB` : `${gb.toFixed(1)} GB`
  const formatBytes = (bytes: number) => {
    if (bytes === 0) return '0 B'
    const k = 1024
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
    const i = Math.floor(Math.log(bytes) / Math.log(k))
    return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`
  }

  const getStatusColor = (status: string) => {
    const colors: Record<string, string> = {
      online: 'text-emerald-700 bg-emerald-50 border-emerald-200',
      healthy: 'text-emerald-700 bg-emerald-50 border-emerald-200',
      degraded: 'text-amber-800 bg-amber-50 border-amber-200',
      offline: 'text-red-700 bg-red-50 border-red-200',
      compliant: 'text-emerald-700 bg-emerald-50 border-emerald-200',
      non_compliant: 'text-red-700 bg-red-50 border-red-200',
      completed: 'text-emerald-700 bg-emerald-50 border-emerald-200',
      in_progress: 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
      pending: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
      failed: 'text-red-700 bg-red-50 border-red-200',
    }
    return colors[status] || 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
  }


  return (
    <div className="p-6">
      <PageHeader
        title="Distributed Storage"
        onRefresh={() => void loadData()}
        refreshing={loading}
      />

      <PageLoadBanner title="Could not load distributed storage" headline={loadError} onRetry={() => void loadData()} />

      {/* Summary */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-3 mb-4">
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Storage Pools</div>
          <div className="text-2xl font-bold">{pools.length}</div>
        </div>
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Total Capacity</div>
          <div className="text-2xl font-bold">{formatGB(pools.reduce((s, p) => s + p.total_capacity_gb, 0))}</div>
        </div>
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Policies</div>
          <div className="text-2xl font-bold">{policies.length}</div>
        </div>
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Active Migrations</div>
          <div className="text-2xl font-bold text-[var(--zf-link)]">
            {migrations.filter(m => m.status === 'in_progress').length}
          </div>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex gap-1 mb-4 bg-[var(--zf-canvas)] rounded-lg p-1">
        {(['pools', 'policies', 'migrations', 'clusters'] as const).map(tab => (
          <button key={tab} onClick={() => setActiveTab(tab)}
            className={`flex-1 px-4 py-2 rounded text-sm font-medium capitalize ${activeTab === tab ? 'bg-[var(--zf-link-fill)] text-white' : 'text-[var(--zf-muted)] hover:bg-black/[0.04]'}`}>
            {tab === 'clusters' ? 'Datastore Clusters' : tab}
          </button>
        ))}
      </div>

      {/* Pools Tab */}
      {activeTab === 'pools' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreatePool(true)}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Create Pool
            </button>
          </div>
          <div className="space-y-4">
            {pools.length === 0 ? (
              <div className="text-center py-12 text-[var(--zf-muted)] bg-[var(--zf-canvas)] rounded-lg">No storage pools configured.</div>
            ) : pools.map(pool => {
              const usedPct = pool.total_capacity_gb > 0 ? (pool.used_capacity_gb / pool.total_capacity_gb * 100) : 0
              return (
                <div key={pool.id} className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg p-4">
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center gap-3">
                      <span className="font-semibold text-lg">{pool.name}</span>
                      <span className={`px-2 py-1 rounded text-xs font-medium border ${getStatusColor(pool.status)}`}>
                        {pool.status}
                      </span>
                    </div>
                    <div className="flex items-center gap-2">
                      <span className="text-sm text-[var(--zf-muted)]">{pool.hosts.length} hosts | RF: {pool.replication_factor}</span>
                      <button onClick={() => handleDeletePool(pool.id)} className="text-red-600 hover:text-red-800 p-1">
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </div>
                  </div>
                  <div className="mb-2">
                    <div className="flex justify-between text-sm text-[var(--zf-muted)] mb-1">
                      <span>Used: {formatGB(pool.used_capacity_gb)}</span>
                      <span>Total: {formatGB(pool.total_capacity_gb)}</span>
                    </div>
                    <div className="w-full bg-[var(--zf-surface)] rounded-full h-3">
                      <div className={`h-3 rounded-full ${usedPct > 90 ? 'bg-[var(--zf-danger)]' : usedPct > 75 ? 'bg-[var(--zf-warning)]' : 'bg-[var(--zf-link-fill)]'}`}
                        style={{ width: `${Math.min(usedPct, 100)}%` }} />
                    </div>
                    <div className="text-right text-xs text-[var(--zf-muted)] mt-1">
                      {formatGB(pool.free_capacity_gb)} free ({(100 - usedPct).toFixed(1)}%)
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
        </div>
      )}

      {/* Policies Tab */}
      {activeTab === 'policies' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreatePolicy(true)}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Create Policy
            </button>
          </div>
          <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Name</th>
                  <th className="p-4">Tier</th>
                  <th className="p-4">RF</th>
                  <th className="p-4">Disk Type</th>
                  <th className="p-4">Encryption</th>
                  <th className="p-4">IOPS Limit</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {policies.length === 0 ? (
                  <tr><td colSpan={7} className="p-8 text-center text-[var(--zf-muted)]">No storage policies.</td></tr>
                ) : policies.map(pol => (
                  <tr key={pol.id} className="hover:bg-[var(--zf-surface)]">
                    <td className="p-4">
                      <div className="font-medium">{pol.name}</div>
                      {pol.description && <div className="text-xs text-[var(--zf-muted)]">{pol.description}</div>}
                    </td>
                    <td className="p-4">
                      <span className={`px-2 py-1 rounded text-xs font-medium border ${
                        pol.tier === 'gold' ? 'text-amber-800 bg-amber-50 border-amber-200' :
                        pol.tier === 'bronze' ? 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]' : 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
                      }`}>{pol.tier}</span>
                    </td>
                    <td className="p-4 text-sm">{pol.replication_factor}</td>
                    <td className="p-4 text-sm uppercase">{pol.disk_type_required ?? '—'}</td>
                    <td className="p-4 text-sm">{pol.encryption_required ? 'Yes' : 'No'}</td>
                    <td className="p-4 text-sm">{pol.iops_limit ?? '—'}</td>
                    <td className="p-4">
                      <div className="flex items-center gap-2">
                        <button onClick={() => setComplianceModalPolicy(pol)} className="text-[var(--zf-link)] hover:text-[var(--zf-link-hover)]" title="Check compliance">
                          <ShieldCheck className="w-4 h-4" />
                        </button>
                        <button onClick={() => handleDeletePolicy(pol.id)} className="text-red-600 hover:text-red-800">
                          <Trash2 className="w-4 h-4" />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Migrations Tab */}
      {activeTab === 'migrations' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowStartMigration(true)} disabled={pools.length < 2 || vms.length === 0}
              title={pools.length < 2 ? 'Need at least 2 storage pools' : vms.length === 0 ? 'No VMs to migrate' : undefined}
              className="zf-btn zf-btn-primary">
              <ArrowRightLeft className="w-4 h-4" /> Start Migration
            </button>
          </div>
          <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg">
          <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
            <thead>
              <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                <th className="p-4">VM</th>
                <th className="p-4">From</th>
                <th className="p-4">To</th>
                <th className="p-4">Progress</th>
                <th className="p-4">Transferred</th>
                <th className="p-4">Status</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {migrations.length === 0 ? (
                <tr><td colSpan={6} className="p-8 text-center text-[var(--zf-muted)]">No storage migrations.</td></tr>
              ) : migrations.map(mig => (
                <tr key={mig.id} className="hover:bg-[var(--zf-surface)]">
                  <td className="p-4 font-medium">{mig.vm_name}</td>
                  <td className="p-4 text-sm text-[var(--zf-muted)]">{mig.source_pool_name}</td>
                  <td className="p-4 text-sm text-[var(--zf-muted)]">{mig.target_pool_name}</td>
                  <td className="p-4">
                    <div className="flex items-center gap-2">
                      <div className="w-24 bg-[var(--zf-surface)] rounded-full h-2">
                        <div className="h-2 rounded-full bg-[var(--zf-link-fill)]" style={{ width: `${mig.progress_pct}%` }} />
                      </div>
                      <span className="text-xs text-[var(--zf-muted)]">{mig.progress_pct}%</span>
                    </div>
                  </td>
                  <td className="p-4 text-sm">{formatBytes(mig.bytes_migrated)} / {formatBytes(mig.bytes_total)}</td>
                  <td className="p-4">
                    <span className={`px-2 py-1 rounded text-xs font-medium border ${getStatusColor(mig.status)}`}>
                      {mig.status}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          </div>
        </div>
      )}

      {/* Datastore Clusters Tab */}
      {activeTab === 'clusters' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreateCluster(true)}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Create Cluster
            </button>
          </div>
          <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Name</th>
                  <th className="p-4">Datastores</th>
                  <th className="p-4">SDRS</th>
                  <th className="p-4">Space Threshold</th>
                  <th className="p-4">IO Latency Threshold</th>
                  <th className="p-4">Automation</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {dsClusters.length === 0 ? (
                  <tr><td colSpan={6} className="p-8 text-center text-[var(--zf-muted)]">No datastore clusters.</td></tr>
                ) : dsClusters.map(cl => (
                  <tr key={cl.id} className="hover:bg-[var(--zf-surface)]">
                    <td className="p-4 font-medium">{cl.name}</td>
                    <td className="p-4 text-sm">{cl.datastore_ids.length}</td>
                    <td className="p-4 text-sm">{cl.storage_drs_enabled ? 'Enabled' : 'Disabled'}</td>
                    <td className="p-4 text-sm">{cl.space_threshold_pct}%</td>
                    <td className="p-4 text-sm">{cl.io_latency_threshold_ms ? `${cl.io_latency_threshold_ms} ms` : '—'}</td>
                    <td className="p-4 text-sm capitalize">{cl.automation_level.replace('_', ' ')}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Create Pool Modal */}
      {showCreatePool && (
        <ModalForm title="Create Storage Pool" onClose={() => setShowCreatePool(false)}
          onSubmit={async (data) => {
            const hostNames = String(data.hosts || '').split(',').map((h: string) => h.trim()).filter(Boolean)
            if (hostNames.length === 0) { toast.error('At least one host is required'); return }
            if (!String(data.cluster_id || '').trim()) { toast.error('Cluster ID is required'); return }
            const hosts = hostNames.map((hostname: string) => ({ host_id: hostname, hostname, disks: [] }))
            await createDistributedPool({
              name: data.name,
              cluster_id: data.cluster_id,
              hosts,
              replication_factor: Number(data.replication_factor) || 2,
              erasure_coding: false,
              fault_domains: [],
            })
            toast.success('Pool created')
            setShowCreatePool(false)
            loadData()
          }}
          fields={[
            { name: 'name', label: 'Pool Name', type: 'text', required: true },
            { name: 'cluster_id', label: 'Cluster ID', type: 'text', required: true },
            { name: 'hosts', label: 'Hosts (comma-separated hostnames)', type: 'text', required: true },
            { name: 'replication_factor', label: 'Replication Factor', type: 'number', defaultValue: 2 },
          ]}
        />
      )}

      {/* Create Policy Modal */}
      {showCreatePolicy && (
        <ModalForm title="Create Storage Policy" onClose={() => setShowCreatePolicy(false)}
          onSubmit={async (data) => {
            await createStoragePolicy({
              name: data.name,
              description: data.description || '',
              replication_factor: Number(data.replication_factor) || 2,
              disk_type_required: data.disk_type_required || undefined,
              encryption_required: data.encryption_required === 'yes',
              iops_limit: data.iops_limit ? Number(data.iops_limit) : undefined,
              throughput_limit_mbps: data.throughput_limit_mbps ? Number(data.throughput_limit_mbps) : undefined,
              tier: data.tier,
            })
            toast.success('Policy created')
            setShowCreatePolicy(false)
            loadData()
          }}
          fields={[
            { name: 'name', label: 'Policy Name', type: 'text', required: true },
            { name: 'description', label: 'Description', type: 'text' },
            { name: 'replication_factor', label: 'Replication Factor', type: 'number', defaultValue: 2 },
            { name: 'disk_type_required', label: 'Required Disk Type', type: 'select', options: ['', 'ssd', 'hdd', 'nvme'], defaultValue: '' },
            { name: 'encryption_required', label: 'Encryption Required', type: 'select', options: ['no', 'yes'], defaultValue: 'no' },
            { name: 'iops_limit', label: 'IOPS Limit (optional)', type: 'number' },
            { name: 'tier', label: 'Tier', type: 'select', options: ['gold', 'silver', 'bronze'], defaultValue: 'silver' },
          ]}
        />
      )}

      {/* Create Cluster Modal */}
      {showCreateCluster && (
        <ModalForm title="Create Datastore Cluster" onClose={() => setShowCreateCluster(false)}
          onSubmit={async (data) => {
            if (!String(data.cluster_id || '').trim()) { toast.error('Cluster ID is required'); return }
            const automation_level = data.automation_level === 'fully_automated' ? 'fully_automated' : 'manual'
            await createDatastoreCluster({
              name: data.name,
              cluster_id: data.cluster_id,
              datastore_ids: data.datastore_ids,
              storage_drs_enabled: automation_level === 'fully_automated',
              space_threshold_pct: Number(data.space_threshold_pct) || 80,
              io_latency_threshold_ms: Number(data.io_latency_threshold_ms) || undefined,
              automation_level,
            })
            toast.success('Cluster created')
            setShowCreateCluster(false)
            loadData()
          }}
          fields={[
            { name: 'name', label: 'Cluster Name', type: 'text', required: true },
            { name: 'cluster_id', label: 'Cluster ID', type: 'text', required: true },
            {
              name: 'datastore_ids', label: 'Storage Pools (Datastores)', type: 'multiselect', required: true,
              options: pools.map(p => p.id),
              optionLabels: Object.fromEntries(pools.map(p => [p.id, p.name])),
            },
            { name: 'space_threshold_pct', label: 'Space Threshold (%)', type: 'number', defaultValue: 80 },
            { name: 'io_latency_threshold_ms', label: 'IO Latency Threshold (ms)', type: 'number', defaultValue: 15 },
            { name: 'automation_level', label: 'Automation Level', type: 'select', options: ['manual', 'fully_automated'], defaultValue: 'manual' },
          ]}
        />
      )}

      {/* Start Migration Modal */}
      {showStartMigration && (
        <ModalForm title="Start Storage Migration" onClose={() => setShowStartMigration(false)}
          onSubmit={async (data) => {
            if (!data.vm_id) { toast.error('Select a VM'); return }
            if (!data.source_pool_id || !data.target_pool_id) { toast.error('Select source and target pools'); return }
            if (data.source_pool_id === data.target_pool_id) { toast.error('Source and target pools must differ'); return }
            await startStorageMigration({
              vm_id: String(data.vm_id),
              source_pool_id: String(data.source_pool_id),
              target_pool_id: String(data.target_pool_id),
              policy_id: data.policy_id ? String(data.policy_id) : undefined,
            })
            toast.success('Migration started')
            setShowStartMigration(false)
            loadData()
          }}
          fields={[
            {
              name: 'vm_id', label: 'VM', type: 'select', required: true,
              options: vms.map(v => v.name),
              optionLabels: Object.fromEntries(vms.map(v => [v.name, v.name])),
            },
            {
              name: 'source_pool_id', label: 'Source Pool', type: 'select', required: true,
              options: pools.map(p => p.id),
              optionLabels: Object.fromEntries(pools.map(p => [p.id, p.name])),
            },
            {
              name: 'target_pool_id', label: 'Target Pool', type: 'select', required: true,
              options: pools.map(p => p.id),
              optionLabels: Object.fromEntries(pools.map(p => [p.id, p.name])),
            },
            {
              name: 'policy_id', label: 'Storage Policy (optional)', type: 'select',
              options: policies.map(p => p.id),
              optionLabels: Object.fromEntries(policies.map(p => [p.id, p.name])),
            },
          ]}
        />
      )}

      {complianceModalPolicy && (
        <ComplianceCheckModal policy={complianceModalPolicy} pools={pools} onClose={() => setComplianceModalPolicy(null)} />
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

interface FieldDef {
  name: string
  label: string
  type: 'text' | 'number' | 'select' | 'multiselect'
  required?: boolean
  defaultValue?: string | number | string[]
  options?: string[]
  optionLabels?: Record<string, string>
}

function ModalForm({ title, fields, onClose, onSubmit }: {
  title: string; fields: FieldDef[]; onClose: () => void; onSubmit: (data: Record<string, any>) => Promise<void>
}) {
  const toast = useToastContext()
  const [values, setValues] = useState<Record<string, any>>(() => {
    const init: Record<string, any> = {}
    fields.forEach(f => { init[f.name] = f.defaultValue ?? (f.type === 'multiselect' ? [] : '') })
    return init
  })

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    for (const f of fields) {
      if (f.required && f.type === 'multiselect' && (values[f.name] as string[]).length === 0) {
        toast.error(`${f.label} requires at least one selection`)
        return
      }
    }
    try {
      await onSubmit(values)
    } catch { toast.error(`Failed: ${title}`) }
  }

  return (
    <div className="modal-backdrop">
      <div className="modal-card w-full max-w-md">
        <h2 className="text-xl font-bold mb-4">{title}</h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          {fields.map(f => (
            <div key={f.name}>
              <label className="block text-sm font-medium mb-1">{f.label}</label>
              {f.type === 'select' ? (
                <select value={values[f.name]} onChange={e => setValues(v => ({ ...v, [f.name]: e.target.value }))}
                  className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2">
                  {f.options?.map(o => <option key={o} value={o}>{f.optionLabels?.[o] ?? o}</option>)}
                </select>
              ) : f.type === 'multiselect' ? (
                f.options && f.options.length > 0 ? (
                  <div className="flex flex-wrap gap-2">
                    {f.options.map(o => {
                      const selected: string[] = values[f.name]
                      const isSel = selected.includes(o)
                      return (
                        <button type="button" key={o}
                          onClick={() => setValues(v => ({
                            ...v,
                            [f.name]: isSel ? selected.filter(x => x !== o) : [...selected, o],
                          }))}
                          className={`px-2.5 py-1 rounded text-xs transition ${isSel ? 'bg-[var(--zf-link-fill)] text-white' : 'bg-[var(--zf-surface)] text-[var(--zf-muted)] border border-[var(--zf-hairline)]'}`}>
                          {f.optionLabels?.[o] ?? o}
                        </button>
                      )
                    })}
                  </div>
                ) : (
                  <p className="text-xs text-[var(--zf-muted)]">Nothing available to select yet.</p>
                )
              ) : (
                <input type={f.type} value={values[f.name]}
                  onChange={e => setValues(v => ({ ...v, [f.name]: f.type === 'number' ? Number(e.target.value) : e.target.value }))}
                  className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" required={f.required} />
              )}
            </div>
          ))}
          <div className="flex gap-3">
            <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
            <button type="submit" className="zf-btn zf-btn-primary flex-1">Create</button>
          </div>
        </form>
      </div>
    </div>
  )
}

function ComplianceCheckModal({ policy, pools, onClose }: {
  policy: StoragePolicy; pools: DistributedStoragePool[]; onClose: () => void
}) {
  const toast = useToastContext()
  const [vmName, setVmName] = useState('')
  const [poolId, setPoolId] = useState(pools[0]?.id ?? '')
  const [checking, setChecking] = useState(false)
  const [report, setReport] = useState<ComplianceReport | null>(null)

  const handleCheck = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!vmName.trim()) { toast.error('VM name is required'); return }
    if (!poolId) { toast.error('A storage pool is required'); return }
    setChecking(true)
    setReport(null)
    try {
      const result = await checkCompliance(policy.id, vmName.trim(), poolId)
      setReport(result)
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Compliance check failed')
    } finally {
      setChecking(false)
    }
  }

  return (
    <div className="modal-backdrop">
      <div className="modal-card w-full max-w-md">
        <h2 className="text-xl font-bold mb-1">Check Compliance</h2>
        <p className="text-sm text-[var(--zf-muted)] mb-4">Against policy "{policy.name}"</p>
        <form onSubmit={handleCheck} className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">VM Name</label>
            <input value={vmName} onChange={e => setVmName(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" placeholder="my-vm" />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Storage Pool</label>
            <select value={poolId} onChange={e => setPoolId(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2">
              {pools.length === 0 && <option value="">No pools available</option>}
              {pools.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
            </select>
          </div>

          {report && (
            <div className={`rounded-lg border p-3 ${report.compliant ? 'bg-emerald-50 border-emerald-200' : 'bg-red-50 border-red-200'}`}>
              <div className={`flex items-center gap-2 text-sm font-medium ${report.compliant ? 'text-emerald-700' : 'text-red-700'}`}>
                {report.compliant ? <CheckCircle className="w-4 h-4" /> : <XCircle className="w-4 h-4" />}
                {report.compliant ? 'Compliant' : `${report.violations.length} violation(s)`}
              </div>
              {!report.compliant && (
                <ul className="mt-2 space-y-1 text-xs text-[var(--zf-ink)] list-disc list-inside">
                  {report.violations.map((v, i) => (
                    <li key={i} className="text-amber-800">{v}</li>
                  ))}
                </ul>
              )}
            </div>
          )}

          <div className="flex gap-3">
            <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Close</button>
            <button type="submit" disabled={checking || pools.length === 0} className="zf-btn zf-btn-primary flex-1">
              {checking ? 'Checking…' : 'Check'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
