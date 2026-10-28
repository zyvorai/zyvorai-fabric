// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Plus, Trash2, Play, Pause, SkipForward } from 'lucide-react'
import {
  listBaselines,
  createBaseline,
  deleteBaseline,
  getComplianceStatus,
  scanHostCompliance,
  listRemediations,
  listRollingUpdates,
  createRollingUpdate,
  startRollingUpdate,
  pauseRollingUpdate,
  advanceRollingUpdate,
  type Baseline,
  type HostComplianceStatus,
  type RemediationTask,
  type RollingUpdatePlan,
} from '../api/lifecycle'
import { listHosts, HostInfo } from '../api/datacenter'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import PageLoadBanner from '../components/PageLoadBanner'
import { PageHeader, Modal } from '../components/ui'
import { usePageLoader } from '../hooks/usePageLoader'
import { toastFailure } from '../utils/toastError'

export default function LifecycleManager() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [baselines, setBaselines] = useState<Baseline[]>([])
  const [scans, setScans] = useState<HostComplianceStatus[]>([])
  const [tasks, setTasks] = useState<RemediationTask[]>([])
  const [updates, setUpdates] = useState<RollingUpdatePlan[]>([])
  const [hosts, setHosts] = useState<HostInfo[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load lifecycle data')
  const [activeTab, setActiveTab] = useState<'baselines' | 'compliance' | 'remediation' | 'updates'>('baselines')
  const [showCreateBaseline, setShowCreateBaseline] = useState(false)
  const [showCreateUpdate, setShowCreateUpdate] = useState(false)

  const loadData = useCallback(() => {
    return run(async () => {
      const [b, s, t, u, h] = await Promise.all([
        listBaselines(),
        getComplianceStatus(),
        listRemediations(),
        listRollingUpdates(),
        listHosts(),
      ])
      setBaselines(b)
      setScans(s)
      setTasks(t)
      setUpdates(u)
      setHosts(h)
    })
  }, [run])

  const handleStartUpdate = async (id: string) => {
    try { await startRollingUpdate(id); toast.success('Rolling update started'); loadData() }
    catch (err) { toastFailure(toast, 'Failed to start rolling update', err) }
  }

  const handlePauseUpdate = async (id: string) => {
    try { await pauseRollingUpdate(id); toast.success('Rolling update paused'); loadData() }
    catch (err) { toastFailure(toast, 'Failed to pause rolling update', err) }
  }

  const handleAdvanceUpdate = async (id: string) => {
    try { await advanceRollingUpdate(id); toast.success('Advanced to next host'); loadData() }
    catch (err) { toastFailure(toast, 'Failed to advance rolling update', err) }
  }

  useEffect(() => {
    void loadData()
  }, [loadData])

  const handleDeleteBaseline = async (id: string) => {
    const ok = await confirm('Delete Baseline', 'Delete this baseline?', { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return
    try { await deleteBaseline(id); toast.success('Baseline deleted'); loadData() }
    catch { toast.error('Failed to delete baseline') }
  }

  const handleRunScan = async (baselineId: string) => {
    try { await scanHostCompliance(baselineId); toast.success('Compliance scan initiated'); loadData() }
    catch { toast.error('Failed to run scan') }
  }

  const getSeverityColor = (severity: string) => {
    const m: Record<string, string> = {
      critical: 'text-red-700 bg-red-50 border-red-200', important: 'text-amber-800 bg-amber-50 border-amber-200',
      moderate: 'text-amber-800 bg-amber-50 border-amber-200', low: 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
    }
    return m[severity] || 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
  }

  const getStatusColor = (status: string) => {
    const m: Record<string, string> = {
      compliant: 'text-emerald-700 bg-emerald-50 border-emerald-200', non_compliant: 'text-red-700 bg-red-50 border-red-200',
      incompatible: 'text-amber-800 bg-amber-50 border-amber-200', unknown: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
      pending: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]', pre_check: 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
      maintenance_mode: 'text-amber-800 bg-amber-50 border-amber-200', remediating: 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
      rebooting: 'text-amber-800 bg-amber-50 border-amber-200', completed: 'text-emerald-700 bg-emerald-50 border-emerald-200',
      failed: 'text-red-700 bg-red-50 border-red-200', running: 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
      paused: 'text-amber-800 bg-amber-50 border-amber-200',
    }
    return m[status] || 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
  }


  return (
    <div className="space-y-6">
      <PageHeader
        title="Lifecycle"
        description="Baselines, compliance scans, remediation, and rolling updates"
        onRefresh={() => void loadData()}
        refreshing={loading}
      />

      <PageLoadBanner title="Could not load lifecycle data" headline={loadError} onRetry={() => void loadData()} />

      {/* Summary */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-3 mb-4">
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Baselines</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{baselines.length}</div>
        </div>
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Non-Compliant Hosts</div>
          <div className="text-2xl font-bold text-red-700">
            {scans.filter(s => s.status === 'non_compliant').length}
          </div>
        </div>
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Active Tasks</div>
          <div className="text-2xl font-bold text-[var(--zf-link)]">
            {tasks.filter(t => t.status !== 'completed' && t.status !== 'failed').length}
          </div>
        </div>
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Rolling Updates</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{updates.filter(u => u.status === 'running').length} active</div>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex gap-1 mb-4 bg-[var(--zf-canvas)] rounded-lg p-1">
        {(['baselines', 'compliance', 'remediation', 'updates'] as const).map(tab => (
          <button key={tab} onClick={() => setActiveTab(tab)}
            className={`flex-1 px-4 py-2 rounded text-sm font-medium capitalize ${activeTab === tab ? 'bg-[var(--zf-link-fill)] text-white' : 'text-[var(--zf-ink)] hover:bg-black/[0.04]'}`}>
            {tab === 'updates' ? 'Rolling Updates' : tab === 'compliance' ? 'Compliance Scans' : tab}
          </button>
        ))}
      </div>

      {/* Baselines Tab */}
      {activeTab === 'baselines' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreateBaseline(true)}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Create Baseline
            </button>
          </div>
          <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Name</th>
                  <th className="p-4">Type</th>
                  <th className="p-4">Severity</th>
                  <th className="p-4">Release Date</th>
                  <th className="p-4">Hosts</th>
                  <th className="p-4">Compliant</th>
                  <th className="p-4">Compliance</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {baselines.length === 0 ? (
                  <tr><td colSpan={8} className="p-8 text-center text-[var(--zf-muted)]">No baselines configured.</td></tr>
                ) : baselines.map(bl => {
                  const compPct = bl.host_count > 0 ? (bl.compliant_count / bl.host_count * 100) : 0
                  return (
                    <tr key={bl.id} className="hover:bg-black/[0.02]">
                      <td className="p-4">
                        <div className="font-medium">{bl.name}</div>
                        {bl.description && <div className="text-xs text-[var(--zf-muted)]">{bl.description}</div>}
                      </td>
                      <td className="p-4 text-sm">{bl.baseline_type}</td>
                      <td className="p-4">
                        <span className={`px-2 py-1 rounded-full text-xs font-medium border ${getSeverityColor(bl.severity)}`}>{bl.severity}</span>
                      </td>
                      <td className="p-4 text-sm text-[var(--zf-muted)]">{new Date(bl.release_date).toLocaleDateString()}</td>
                      <td className="p-4 text-sm">{bl.host_count}</td>
                      <td className="p-4 text-sm text-emerald-600">{bl.compliant_count}</td>
                      <td className="p-4">
                        <div className="flex items-center gap-2">
                          <div className="w-20 bg-[var(--zf-hairline)] rounded-full h-2">
                            <div className={`h-2 rounded-full ${compPct === 100 ? 'bg-[var(--zf-success)]' : compPct > 50 ? 'bg-[var(--zf-warning)]' : 'bg-[var(--zf-danger)]'}`}
                              style={{ width: `${compPct}%` }} />
                          </div>
                          <span className="text-xs text-[var(--zf-muted)]">{compPct.toFixed(0)}%</span>
                        </div>
                      </td>
                      <td className="p-4">
                        <div className="flex items-center gap-2">
                          <button onClick={() => handleRunScan(bl.id)} className="text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] p-1" title="Run scan">
                            <Play className="w-4 h-4" />
                          </button>
                          <button onClick={() => handleDeleteBaseline(bl.id)} className="text-[var(--zf-danger)] hover:opacity-70 p-1" aria-label={`Delete ${bl.name}`} title={`Delete ${bl.name}`}>
                            <Trash2 className="w-4 h-4" />
                          </button>
                        </div>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Compliance Scans Tab */}
      {activeTab === 'compliance' && (
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg">
          <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
            <thead>
              <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                <th className="p-4">Host</th>
                <th className="p-4">Baseline</th>
                <th className="p-4">Status</th>
                <th className="p-4">Missing Patches</th>
                <th className="p-4">Scanned</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {scans.length === 0 ? (
                <tr><td colSpan={5} className="p-8 text-center text-[var(--zf-muted)]">No compliance scan results.</td></tr>
              ) : scans.map(scan => (
                <tr key={scan.id} className="hover:bg-black/[0.02]">
                  <td className="p-4 font-medium">{scan.hostname}</td>
                  <td className="p-4 text-sm text-[var(--zf-muted)]">{scan.baseline_name}</td>
                  <td className="p-4">
                    <span className={`px-2 py-1 rounded-full text-xs font-medium border ${getStatusColor(scan.status)}`}>{scan.status.replace(/_/g, ' ')}</span>
                  </td>
                  <td className="p-4 text-sm">
                    {scan.missing_patches.length > 0 ? (
                      <span className="text-red-700">{scan.missing_patches.length} patches</span>
                    ) : <span className="text-emerald-700">None</span>}
                  </td>
                  <td className="p-4 text-sm text-[var(--zf-muted)]">{new Date(scan.last_scanned).toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Remediation Tab */}
      {activeTab === 'remediation' && (
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg">
          <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
            <thead>
              <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                <th className="p-4">Host</th>
                <th className="p-4">Baseline</th>
                <th className="p-4">Status</th>
                <th className="p-4">Progress</th>
                <th className="p-4">Patches</th>
                <th className="p-4">Error</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {tasks.length === 0 ? (
                <tr><td colSpan={6} className="p-8 text-center text-[var(--zf-muted)]">No remediation tasks.</td></tr>
              ) : tasks.map(task => (
                <tr key={task.id} className="hover:bg-black/[0.02]">
                  <td className="p-4 font-medium">{task.hostname}</td>
                  <td className="p-4 text-sm text-[var(--zf-muted)]">{task.baseline_name}</td>
                  <td className="p-4">
                    <span className={`px-2 py-1 rounded-full text-xs font-medium border ${getStatusColor(task.status)}`}>{task.status.replace(/_/g, ' ')}</span>
                  </td>
                  <td className="p-4">
                    <div className="flex items-center gap-2">
                      <div className="w-24 bg-[var(--zf-hairline)] rounded-full h-2">
                        <div className="h-2 rounded-full bg-[var(--zf-link-fill)]" style={{ width: `${task.progress}%` }} />
                      </div>
                      <span className="text-xs text-[var(--zf-muted)]">{task.progress}%</span>
                    </div>
                  </td>
                  <td className="p-4 text-sm">{task.patches_applied}/{task.patches_total}</td>
                  <td className="p-4 text-sm text-red-700">{task.error || '-'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Rolling Updates Tab */}
      {activeTab === 'updates' && (
        <div className="space-y-4">
          <div className="flex justify-end">
            <button onClick={() => setShowCreateUpdate(true)} disabled={baselines.length === 0 || hosts.length === 0}
              title={baselines.length === 0 ? 'Create a baseline first' : hosts.length === 0 ? 'No hosts registered' : undefined}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Create Rolling Update
            </button>
          </div>
          {updates.length === 0 ? (
            <div className="text-center py-12 text-[var(--zf-muted)] bg-[var(--zf-canvas)] rounded-lg">No rolling updates.</div>
          ) : updates.map(update => {
            const updatePct = update.total_hosts > 0 ? (update.completed_hosts / update.total_hosts) * 100 : 0
            return (
            <div key={update.id} className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg p-4">
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-3">
                  <span className="font-semibold">{update.name}</span>
                  <span className={`px-2 py-1 rounded-full text-xs font-medium border ${getStatusColor(update.status)}`}>{update.status}</span>
                </div>
                <div className="flex items-center gap-3">
                  <span className="text-sm text-[var(--zf-muted)]">
                    {update.completed_hosts}/{update.total_hosts} hosts | Parallel: {update.parallel_count}
                  </span>
                  {update.status === 'pending' && (
                    <button onClick={() => handleStartUpdate(update.id)} className="flex items-center gap-1 text-emerald-700 hover:opacity-70 text-sm">
                      <Play className="w-3.5 h-3.5" /> Start
                    </button>
                  )}
                  {update.status === 'running' && (
                    <>
                      <button onClick={() => handleAdvanceUpdate(update.id)} className="flex items-center gap-1 text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] text-sm" title="Advance to next host">
                        <SkipForward className="w-3.5 h-3.5" /> Advance
                      </button>
                      <button onClick={() => handlePauseUpdate(update.id)} className="flex items-center gap-1 text-amber-700 hover:opacity-70 text-sm">
                        <Pause className="w-3.5 h-3.5" /> Pause
                      </button>
                    </>
                  )}
                  {update.status === 'paused' && (
                    <button onClick={() => handleStartUpdate(update.id)} className="flex items-center gap-1 text-emerald-700 hover:opacity-70 text-sm">
                      <Play className="w-3.5 h-3.5" /> Resume
                    </button>
                  )}
                </div>
              </div>
              <div className="mb-2">
                <div className="flex justify-between text-xs text-[var(--zf-muted)] mb-1">
                  <span>{update.current_host ? `Current: ${update.current_host}` : 'Waiting...'}</span>
                  <span>{updatePct.toFixed(0)}%</span>
                </div>
                <div className="w-full bg-[var(--zf-hairline)] rounded-full h-3">
                  <div className={`h-3 rounded-full ${update.status === 'completed' ? 'bg-[var(--zf-success)]' : 'bg-[var(--zf-link-fill)]'}`}
                    style={{ width: `${updatePct}%` }} />
                </div>
              </div>
              <div className="text-xs text-[var(--zf-muted)]">
                {update.started_at && `Started: ${new Date(update.started_at).toLocaleString()}`}
                {update.completed_at && ` | Completed: ${new Date(update.completed_at).toLocaleString()}`}
                {update.failed_hosts > 0 && <span className="text-red-700"> | {update.failed_hosts} failed</span>}
              </div>
            </div>
          )})}
        </div>
      )}

      {/* Create Baseline Modal */}
      {showCreateBaseline && (
        <CreateBaselineModal onClose={() => setShowCreateBaseline(false)} onCreated={() => { setShowCreateBaseline(false); loadData() }} />
      )}
      {/* Create Rolling Update Modal */}
      {showCreateUpdate && (
        <CreateRollingUpdateModal baselines={baselines} hosts={hosts} onClose={() => setShowCreateUpdate(false)} onCreated={() => { setShowCreateUpdate(false); loadData() }} />
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

function CreateBaselineModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [baselineType, setBaselineType] = useState<'patch' | 'upgrade' | 'extension'>('patch')
  const [severity, setSeverity] = useState<'critical' | 'important' | 'moderate' | 'low'>('moderate')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try { await createBaseline({ name, description: description || undefined, baseline_type: baselineType, severity }); onCreated() }
    catch { toast.error('Failed to create baseline') }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Create Baseline</h2>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Name</label>
          <input aria-label="Name" type="text" value={name} onChange={e => setName(e.target.value)} className="input-field" required /></div>
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Description</label>
          <input aria-label="Description" type="text" value={description} onChange={e => setDescription(e.target.value)} className="input-field" /></div>
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Type</label>
          <select aria-label="Type" value={baselineType} onChange={e => setBaselineType(e.target.value as 'patch' | 'upgrade' | 'extension')} className="input-field">
            <option value="patch">Patch</option><option value="upgrade">Upgrade</option><option value="extension">Extension</option>
          </select></div>
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Severity</label>
          <select aria-label="Severity" value={severity} onChange={e => setSeverity(e.target.value as 'critical' | 'important' | 'moderate' | 'low')} className="input-field">
            <option value="critical">Critical</option><option value="important">Important</option><option value="moderate">Moderate</option><option value="low">Low</option>
          </select></div>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
          <button type="submit" className="zf-btn zf-btn-primary flex-1">Create</button>
        </div>
      </form>
    </Modal>
  )
}

function CreateRollingUpdateModal({ baselines, hosts, onClose, onCreated }: {
  baselines: Baseline[]; hosts: HostInfo[]; onClose: () => void; onCreated: () => void
}) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [baselineId, setBaselineId] = useState(baselines[0]?.id || '')
  const [selectedHosts, setSelectedHosts] = useState<Set<string>>(new Set())
  const [parallelCount, setParallelCount] = useState(1)
  const [failureThreshold, setFailureThreshold] = useState(1)
  const [preCheckEnabled, setPreCheckEnabled] = useState(true)
  const [autoRemediate, setAutoRemediate] = useState(false)
  const [submitting, setSubmitting] = useState(false)

  const toggleHost = (id: string) => setSelectedHosts(prev => {
    const next = new Set(prev)
    if (next.has(id)) next.delete(id); else next.add(id)
    return next
  })

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (selectedHosts.size === 0) { toast.error('Select at least one host'); return }
    setSubmitting(true)
    try {
      await createRollingUpdate({
        name,
        baseline_id: baselineId,
        host_ids: Array.from(selectedHosts),
        parallel_count: parallelCount,
        failure_threshold: failureThreshold,
        pre_check_enabled: preCheckEnabled,
        auto_remediate: autoRemediate,
      })
      toast.success('Rolling update plan created')
      onCreated()
    } catch (err) { toastFailure(toast, 'Failed to create rolling update', err) } finally { setSubmitting(false) }
  }

  return (
    <Modal open onClose={onClose} className="max-w-lg">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Create Rolling Update</h2>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Name</label>
          <input aria-label="Name" type="text" value={name} onChange={e => setName(e.target.value)} placeholder="e.g. patch-cluster-2026-08" className="input-field" required /></div>
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Baseline</label>
          <select aria-label="Baseline" value={baselineId} onChange={e => setBaselineId(e.target.value)} className="input-field">
            {baselines.map(b => <option key={b.id} value={b.id}>{b.name}</option>)}
          </select></div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Hosts</label>
          <div className="max-h-40 overflow-y-auto space-y-1 zf-panel-muted p-2">
            {hosts.map(h => (
              <label key={h.id} className="flex items-center gap-2 px-2 py-1 rounded hover:bg-black/[0.04] cursor-pointer text-[var(--zf-ink)]">
                <input type="checkbox" checked={selectedHosts.has(h.id)} onChange={() => toggleHost(h.id)} />
                <span className="text-sm">{h.hostname}</span>
              </label>
            ))}
          </div>
        </div>
        <div className="grid grid-cols-2 gap-4">
          <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Parallel Hosts</label>
            <input aria-label="Parallel Hosts" type="number" value={parallelCount} onChange={e => setParallelCount(Math.max(1, Number(e.target.value)))} min={1} className="input-field" /></div>
          <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Failure Threshold</label>
            <input aria-label="Failure Threshold" type="number" value={failureThreshold} onChange={e => setFailureThreshold(Math.max(0, Number(e.target.value)))} min={0} className="input-field" /></div>
        </div>
        <label className="flex items-center gap-2 text-[var(--zf-ink)]"><input type="checkbox" checked={preCheckEnabled} onChange={e => setPreCheckEnabled(e.target.checked)} /><span className="text-sm">Run pre-checks before each host</span></label>
        <label className="flex items-center gap-2 text-[var(--zf-ink)]"><input type="checkbox" checked={autoRemediate} onChange={e => setAutoRemediate(e.target.checked)} /><span className="text-sm">Auto-remediate on failure</span></label>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
          <button type="submit" disabled={submitting} className="zf-btn zf-btn-primary flex-1">{submitting ? 'Creating...' : 'Create'}</button>
        </div>
      </form>
    </Modal>
  )
}
