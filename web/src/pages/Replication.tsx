// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Plus, Trash2, Pause, Play } from 'lucide-react'
import {
  listSites,
  registerSite,
  removeSite,
  listReplications,
  configureReplication,
  pauseReplication,
  resumeReplication,
  checkRpoViolations,
  getReplicationHealth,
  type ReplicationSite,
  type ReplicationConfig,
  type ReplicationMetrics,
  type ReplicationHealthSummary,
} from '../api/replication'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import { PageHeader } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'

export default function Replication() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [sites, setSites] = useState<ReplicationSite[]>([])
  const [replications, setReplications] = useState<ReplicationConfig[]>([])
  const [violations, setViolations] = useState<ReplicationMetrics[]>([])
  const [health, setHealth] = useState<ReplicationHealthSummary | null>(null)
  const { loading, loadError, run } = usePageLoader('Failed to load replication data')
  const [activeTab, setActiveTab] = useState<'dashboard' | 'sites' | 'configs' | 'violations'>('dashboard')
  const [showCreateSite, setShowCreateSite] = useState(false)
  const [showCreateConfig, setShowCreateConfig] = useState(false)

  const loadData = useCallback(() => {
    return run(async () => {
      const [s, r, v, h] = await Promise.all([
        listSites(),
        listReplications(),
        checkRpoViolations().catch(() => []),
        getReplicationHealth().catch(() => null),
      ])
      setSites(s)
      setReplications(r)
      setViolations(v)
      setHealth(h)
    })
  }, [run])

  useEffect(() => {
    void loadData()
  }, [loadData])

  const handleRemoveSite = async (id: string) => {
    const ok = await confirm('Remove Site', 'Remove this replication site?', { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return
    try { await removeSite(id); toast.success('Site removed'); loadData() }
    catch { toast.error('Failed to remove site') }
  }

  const handlePause = async (id: string) => {
    try { await pauseReplication(id); toast.success('Replication paused'); loadData() }
    catch { toast.error('Failed to pause replication') }
  }

  const handleResume = async (id: string) => {
    try { await resumeReplication(id); toast.success('Replication resumed'); loadData() }
    catch { toast.error('Failed to resume replication') }
  }

  const getStatusColor = (status: string) => {
    const m: Record<string, string> = {
      connected: 'text-emerald-700 bg-emerald-50 border border-emerald-200',
      disconnected: 'text-red-700 bg-red-50 border border-red-200',
      syncing: 'text-[var(--zf-link)] bg-blue-50 border border-blue-100',
      active: 'text-emerald-700 bg-emerald-50 border border-emerald-200',
      paused: 'text-amber-800 bg-amber-50 border border-amber-200',
      error: 'text-red-700 bg-red-50 border border-red-200',
      initial_sync: 'text-[var(--zf-link)] bg-blue-50 border border-blue-100',
    }
    return m[status] || 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border border-[var(--zf-hairline)]'
  }


  return (
    <div className="p-6">
      <PageHeader
        onRefresh={() => void loadData()}
        refreshing={loading}
        title="Replication"
      />


      <PageLoadBanner title="Could not load replication data" headline={loadError} onRetry={() => void loadData()} />
      {/* Tabs */}
      <div className="flex gap-1 mb-4 bg-[var(--zf-canvas)] rounded-lg p-1">
        {(['dashboard', 'sites', 'configs', 'violations'] as const).map(tab => (
          <button key={tab} onClick={() => setActiveTab(tab)}
            className={`flex-1 px-4 py-2 rounded text-sm font-medium capitalize ${activeTab === tab ? 'bg-[var(--zf-link-fill)] text-white' : 'hover:bg-white/[0.03]'}`}>
            {tab === 'configs' ? 'Replications' : tab === 'violations' ? 'RPO Violations' : tab}
          </button>
        ))}
      </div>

      {/* Dashboard Tab */}
      {activeTab === 'dashboard' && !health && !loading && (
        <div className="zf-panel-muted p-8 text-center text-[var(--zf-muted)]">
          No replication data available.
        </div>
      )}

      {activeTab === 'dashboard' && health && (
        <div>
          <div className="grid grid-cols-1 md:grid-cols-4 gap-3 mb-4">
            <div className="zf-panel-muted px-4 py-3">
              <div className="text-[var(--zf-muted)] text-sm mb-1">Active Replications</div>
              <div className="text-2xl font-bold text-emerald-600">{health.active}</div>
            </div>
            <div className="zf-panel-muted px-4 py-3">
              <div className="text-[var(--zf-muted)] text-sm mb-1">RPO Violations</div>
              <div className="text-2xl font-bold text-red-600">{health.rpo_violations}</div>
            </div>
            <div className="zf-panel-muted px-4 py-3">
              <div className="text-[var(--zf-muted)] text-sm mb-1">Avg RPO</div>
              <div className="text-2xl font-bold">{health.avg_rpo_minutes.toFixed(0)} min</div>
            </div>
            <div className="zf-panel-muted px-4 py-3">
              <div className="text-[var(--zf-muted)] text-sm mb-1">Paused / Error</div>
              <div className="text-2xl font-bold text-amber-600">{health.paused} / {health.error}</div>
            </div>
          </div>
          <div className="zf-panel-muted p-4">
            <h2 className="text-lg font-semibold mb-3">Site Health</h2>
            <div className="space-y-3">
              {health.sites.map(site => (
                <div key={site.site_id} className="flex items-center justify-between p-3 bg-[var(--zf-canvas)] rounded">
                  <div className="flex items-center gap-3">
                    <div className={`w-3.5 h-3.5 rounded-full ${site.health === 'healthy' ? 'bg-emerald-500' : site.health === 'degraded' ? 'bg-amber-500' : 'bg-red-500'}`} />
                    <span className="font-medium">{site.site_name}</span>
                  </div>
                  <span className="text-sm text-[var(--zf-muted)]">{site.replication_count} replications</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Sites Tab */}
      {activeTab === 'sites' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreateSite(true)}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Add Site
            </button>
          </div>
          <div className="zf-panel-muted">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Name</th>
                  <th className="p-4">Type</th>
                  <th className="p-4">Endpoint</th>
                  <th className="p-4">Status</th>
                  <th className="p-4">Replications</th>
                  <th className="p-4">Last Sync</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {sites.length === 0 ? (
                  <tr><td colSpan={7} className="p-8 text-center text-[var(--zf-muted)]">No replication sites.</td></tr>
                ) : sites.map(site => (
                  <tr key={site.id} className="hover:bg-[var(--zf-surface)]">
                    <td className="p-4 font-medium">{site.name}</td>
                    <td className="p-4 text-sm">{site.site_type}</td>
                    <td className="p-4 text-sm font-mono text-[var(--zf-muted)]">{site.endpoint}</td>
                    <td className="p-4">
                      <span className={`px-2 py-1 rounded text-xs font-medium ${getStatusColor(site.status)}`}>{site.status}</span>
                    </td>
                    <td className="p-4 text-sm">{site.replication_count}</td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">{site.last_sync ? new Date(site.last_sync).toLocaleString() : '-'}</td>
                    <td className="p-4">
                      <button onClick={() => handleRemoveSite(site.id)} className="text-red-600 hover:text-red-800">
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Replications Tab */}
      {activeTab === 'configs' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreateConfig(true)}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Configure Replication
            </button>
          </div>
          <div className="zf-panel-muted">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">VM</th>
                  <th className="p-4">RPO</th>
                  <th className="p-4">Status</th>
                  <th className="p-4">Progress</th>
                  <th className="p-4">Last Sync</th>
                  <th className="p-4">Next Sync</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {replications.length === 0 ? (
                  <tr><td colSpan={7} className="p-8 text-center text-[var(--zf-muted)]">No replication configurations.</td></tr>
                ) : replications.map(rep => (
                  <tr key={rep.id} className="hover:bg-[var(--zf-surface)]">
                    <td className="p-4 font-medium">{rep.vm_name}</td>
                    <td className="p-4 text-sm">{rep.rpo_minutes} min</td>
                    <td className="p-4">
                      <span className={`px-2 py-1 rounded text-xs font-medium ${getStatusColor(rep.status)}`}>{rep.status}</span>
                    </td>
                    <td className="p-4">
                      {rep.sync_progress_pct !== undefined && rep.sync_progress_pct !== null ? (
                        <div className="flex items-center gap-2">
                          <div className="w-20 bg-[var(--zf-surface)] rounded-full h-2">
                            <div className="h-2 rounded-full bg-[var(--zf-link-fill)]" style={{ width: `${rep.sync_progress_pct}%` }} />
                          </div>
                          <span className="text-xs text-[var(--zf-muted)]">{rep.sync_progress_pct}%</span>
                        </div>
                      ) : '-'}
                    </td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">{rep.last_sync ? new Date(rep.last_sync).toLocaleString() : '-'}</td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">{rep.next_sync ? new Date(rep.next_sync).toLocaleString() : '-'}</td>
                    <td className="p-4">
                      <div className="flex items-center gap-2">
                        {rep.status === 'active' ? (
                          <button onClick={() => handlePause(rep.id)} className="text-amber-600 hover:text-amber-800 p-1" title="Pause">
                            <Pause className="w-4 h-4" />
                          </button>
                        ) : rep.status === 'paused' ? (
                          <button onClick={() => handleResume(rep.id)} className="text-emerald-600 hover:text-emerald-800 p-1" title="Resume">
                            <Play className="w-4 h-4" />
                          </button>
                        ) : null}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* RPO Violations Tab */}
      {activeTab === 'violations' && (
        <div className="zf-panel-muted">
          <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
            <thead>
              <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                <th className="p-4">VM</th>
                <th className="p-4">Target RPO</th>
                <th className="p-4">Current RPO</th>
                <th className="p-4">Compliant</th>
                <th className="p-4">Bandwidth</th>
                <th className="p-4">Sync Count</th>
                <th className="p-4">Failures</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {violations.length === 0 ? (
                <tr><td colSpan={7} className="p-8 text-center text-[var(--zf-muted)]">No RPO violations. All replications are compliant.</td></tr>
              ) : violations.map(v => (
                <tr key={v.replication_id} className="hover:bg-[var(--zf-surface)]">
                  <td className="p-4 font-medium">{v.vm_name}</td>
                  <td className="p-4 text-sm">{v.rpo_target_minutes} min</td>
                  <td className="p-4 text-sm text-red-600 font-bold">{v.current_rpo_minutes.toFixed(0)} min</td>
                  <td className="p-4">
                    <span className={`px-2 py-1 rounded text-xs font-medium border ${v.rpo_compliant ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-red-700 bg-red-50 border-red-200'}`}>
                      {v.rpo_compliant ? 'Yes' : 'No'}
                    </span>
                  </td>
                  <td className="p-4 text-sm">{v.bandwidth_usage_mbps.toFixed(1)} Mbps</td>
                  <td className="p-4 text-sm">{v.sync_count}</td>
                  <td className="p-4 text-sm text-red-600">{v.failure_count}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Modals */}
      {showCreateSite && (
        <CreateSiteModal onClose={() => setShowCreateSite(false)} onCreated={() => { setShowCreateSite(false); loadData() }} />
      )}
      {showCreateConfig && (
        <CreateConfigModal sites={sites} onClose={() => setShowCreateConfig(false)} onCreated={() => { setShowCreateConfig(false); loadData() }} />
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

function CreateSiteModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [endpoint, setEndpoint] = useState('')
  const [siteType, setSiteType] = useState<'primary' | 'secondary' | 'bidirectional'>('secondary')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try { await registerSite({ name, endpoint, site_type: siteType }); onCreated() }
    catch { toast.error('Failed to register site') }
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-[var(--zf-canvas)] rounded-lg p-6 w-full max-w-md">
        <h2 className="text-xl font-bold mb-4">Add Replication Site</h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">Site Name</label>
            <input type="text" value={name} onChange={e => setName(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" required />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Endpoint</label>
            <input type="text" value={endpoint} onChange={e => setEndpoint(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" placeholder="https://site.example.com" required />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Type</label>
            <select value={siteType} onChange={e => setSiteType(e.target.value as 'primary' | 'secondary' | 'bidirectional')}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2">
              <option value="primary">Primary</option>
              <option value="secondary">Recovery (secondary)</option>
              <option value="bidirectional">Bidirectional</option>
            </select>
          </div>
          <div className="flex gap-3">
            <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
            <button type="submit" className="flex-1 zf-btn zf-btn-primary">Add Site</button>
          </div>
        </form>
      </div>
    </div>
  )
}

function CreateConfigModal({ sites, onClose, onCreated }: { sites: ReplicationSite[]; onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [vmId, setVmId] = useState('')
  const [sourceSite, setSourceSite] = useState(sites[0]?.id || '')
  const [targetSite, setTargetSite] = useState(sites[1]?.id || sites[0]?.id || '')
  const [rpo, setRpo] = useState(15)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await configureReplication({ vm_id: vmId, source_site_id: sourceSite, target_site_id: targetSite, rpo_minutes: rpo })
      onCreated()
    } catch { toast.error('Failed to configure replication') }
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-[var(--zf-canvas)] rounded-lg p-6 w-full max-w-md">
        <h2 className="text-xl font-bold mb-4">Configure Replication</h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">VM ID</label>
            <input type="text" value={vmId} onChange={e => setVmId(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" required />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Source Site</label>
            <select value={sourceSite} onChange={e => setSourceSite(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2">
              {sites.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Target Site</label>
            <select value={targetSite} onChange={e => setTargetSite(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2">
              {sites.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">RPO (minutes)</label>
            <input type="number" value={rpo} onChange={e => setRpo(Number(e.target.value))}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" min={1} required />
          </div>
          <div className="flex gap-3">
            <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
            <button type="submit" className="flex-1 zf-btn zf-btn-primary">Configure</button>
          </div>
        </form>
      </div>
    </div>
  )
}
