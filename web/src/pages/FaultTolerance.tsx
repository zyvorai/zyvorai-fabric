// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Plus, Trash2, Play, AlertTriangle, CheckCircle } from 'lucide-react'
import {
  listFtVms,
  enableFt,
  disableFt,
  checkFtCompatibility,
  triggerFailover,
  testFailover,
  getFtMetrics,
  getFtEvents,
  type FtConfig,
  type FtCompatibility,
  type FtMetrics,
  type FailoverResult,
} from '../api/faultTolerance'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import { PageHeader } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'

export default function FaultTolerance() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [ftVMs, setFtVMs] = useState<FtConfig[]>([])
  const [events, setEvents] = useState<FailoverResult[]>([])
  const [metrics, setMetrics] = useState<Map<string, FtMetrics>>(new Map())
  const { loading, loadError, run } = usePageLoader('Failed to load fault tolerance data')
  const [activeTab, setActiveTab] = useState<'vms' | 'compatibility' | 'events' | 'metrics'>('vms')
  const [showEnableFT, setShowEnableFT] = useState(false)
  const [compatResult, setCompatResult] = useState<FtCompatibility | null>(null)
  const [compatVmId, setCompatVmId] = useState('')

  const loadData = useCallback(() => {
    return run(async () => {
      const [vms, evts] = await Promise.all([listFtVms(), getFtEvents()])
      setFtVMs(vms)
      setEvents(evts)

      const metricsMap = new Map<string, FtMetrics>()
      for (const vm of vms) {
        try {
          const m = await getFtMetrics(vm.vm_name)
          metricsMap.set(vm.vm_name, m)
        } catch { /* skip */ }
      }
      setMetrics(metricsMap)
    })
  }, [run])

  useEffect(() => {
    void loadData()
  }, [loadData])

  const handleDisableFT = async (vmId: string) => {
    const ok = await confirm('Disable Fault Tolerance', 'Disable fault tolerance for this VM?', { variant: 'danger', confirmLabel: 'Disable' })
    if (!ok) return
    try {
      await disableFt(vmId)
      toast.success('FT disabled')
      loadData()
    } catch { toast.error('Failed to disable FT') }
  }

  const handleTriggerFailover = async (vmId: string) => {
    const ok = await confirm('Trigger Failover', 'Trigger failover for this VM? This will move the VM to the secondary host.', { variant: 'danger', confirmLabel: 'Trigger Failover' })
    if (!ok) return
    try {
      await triggerFailover(vmId)
      toast.success('Failover triggered')
      loadData()
    } catch { toast.error('Failed to trigger failover') }
  }

  const handleTestFailover = async (vmId: string) => {
    try {
      await testFailover(vmId)
      toast.success('Test failover initiated')
      loadData()
    } catch { toast.error('Failed to test failover') }
  }

  const handleCheckCompatibility = async () => {
    if (!compatVmId) return
    try {
      const result = await checkFtCompatibility(compatVmId)
      setCompatResult(result)
    } catch { toast.error('Compatibility check failed') }
  }

  const getStatusColor = (status: string) => {
    const m: Record<string, string> = {
      running: 'text-emerald-700 bg-emerald-50 border-emerald-200',
      needs_secondary: 'text-amber-800 bg-amber-50 border-amber-200',
      not_started: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
      error: 'text-red-700 bg-red-50 border-red-200',
      success: 'text-emerald-700 bg-emerald-50 border-emerald-200',
      failed: 'text-red-700 bg-red-50 border-red-200',
    }
    return m[status] || 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
  }


  return (
    <div className="p-6">
      <PageHeader
        onRefresh={() => void loadData()}
        refreshing={loading}
        title="Fault Tolerance"
        actions={
          <button onClick={() => setShowEnableFT(true)}
            className="zf-btn zf-btn-primary zf-btn-sm">
            <Plus className="w-4 h-4" /> Enable FT
          </button>
        }
      />

      <PageLoadBanner title="Could not load fault tolerance data" headline={loadError} onRetry={() => void loadData()} />

      {/* Summary */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-3 mb-4">
        <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Protected VMs</div>
          <div className="text-2xl font-bold">{ftVMs.length}</div>
        </div>
        <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Running</div>
          <div className="text-2xl font-bold text-emerald-600">{ftVMs.filter(v => v.status === 'running').length}</div>
        </div>
        <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Needs Attention</div>
          <div className="text-2xl font-bold text-amber-600">{ftVMs.filter(v => v.status !== 'running').length}</div>
        </div>
        <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Failover Events</div>
          <div className="text-2xl font-bold">{events.length}</div>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex gap-1 mb-4 bg-[var(--zf-surface)] rounded-lg p-1">
        {(['vms', 'compatibility', 'events', 'metrics'] as const).map(tab => (
          <button key={tab} onClick={() => setActiveTab(tab)}
            className={`flex-1 px-4 py-2 rounded text-sm font-medium capitalize ${activeTab === tab ? 'bg-[var(--zf-link-fill)] text-white' : 'text-[var(--zf-muted)] hover:bg-black/[0.04]'}`}>
            {tab === 'vms' ? 'Protected VMs' : tab === 'compatibility' ? 'Compatibility Check' : tab}
          </button>
        ))}
      </div>

      {/* Protected VMs Tab */}
      {activeTab === 'vms' && (
        <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg">
          <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
            <thead>
              <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                <th className="p-4">VM</th>
                <th className="p-4">Secondary Host</th>
                <th className="p-4">Status</th>
                <th className="p-4">Bandwidth</th>
                <th className="p-4">Checkpoint</th>
                <th className="p-4">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {ftVMs.length === 0 ? (
                <tr><td colSpan={6} className="p-8 text-center text-[var(--zf-muted)]">No FT-protected VMs.</td></tr>
              ) : ftVMs.map(vm => (
                <tr key={vm.vm_name} className="hover:bg-[var(--zf-surface)]">
                  <td className="p-4 font-medium">{vm.vm_name}</td>
                  <td className="p-4 text-sm text-[var(--zf-muted)]">{vm.secondary_host_id || 'Not assigned'}</td>
                  <td className="p-4">
                    <span className={`px-2 py-1 rounded text-xs font-medium border ${getStatusColor(vm.status)}`}>{vm.status}</span>
                  </td>
                  <td className="p-4 text-sm">{vm.bandwidth_limit_mbps ?? '—'} Mbps</td>
                  <td className="p-4 text-sm">—</td>
                  <td className="p-4">
                    <div className="flex items-center gap-2">
                      <button onClick={() => handleTestFailover(vm.vm_name)} className="text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] text-sm px-2 py-1" title="Test failover">
                        Test
                      </button>
                      <button onClick={() => handleTriggerFailover(vm.vm_name)} className="text-amber-600 hover:text-amber-700 text-sm px-2 py-1" title="Trigger failover">
                        <Play className="w-4 h-4" />
                      </button>
                      <button onClick={() => handleDisableFT(vm.vm_name)} className="text-red-600 hover:text-red-800 p-1" title="Disable FT">
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

      {/* Compatibility Check Tab */}
      {activeTab === 'compatibility' && (
        <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg p-6">
          <h2 className="text-lg font-semibold mb-4">FT Compatibility Checker</h2>
          <div className="flex gap-4 mb-4">
            <input type="text" value={compatVmId} onChange={e => setCompatVmId(e.target.value)}
              placeholder="Enter VM ID" className="flex-1 input-field" />
            <button onClick={handleCheckCompatibility} className="zf-btn zf-btn-primary">
              Check
            </button>
          </div>
          {compatResult && (
            <div className={`p-4 rounded border ${compatResult.compatible ? 'border-emerald-200 bg-emerald-50' : 'border-red-200 bg-red-50'}`}>
              <div className="flex items-center gap-2 mb-3">
                {compatResult.compatible ? <CheckCircle className="w-5 h-5 text-emerald-600" /> : <AlertTriangle className="w-5 h-5 text-red-600" />}
                <span className="font-semibold">{compatResult.vm_name}: {compatResult.compatible ? 'Compatible' : 'Not Compatible'}</span>
              </div>
              {compatResult.issues.length > 0 && (
                <div className="space-y-2 mb-3">
                  <div className="text-sm font-medium">Issues:</div>
                  {compatResult.issues.map((issue, i) => (
                    <div key={i} className={`text-sm p-2 rounded ${issue.severity === 'error' ? 'bg-red-50 text-red-700' : 'bg-amber-50 text-amber-800'}`}>
                      {issue.message}
                      {issue.resolution && <div className="text-xs text-[var(--zf-muted)] mt-1">Fix: {issue.resolution}</div>}
                    </div>
                  ))}
                </div>
              )}
              {compatResult.recommended_secondary_hosts.length > 0 && (
                <div>
                  <div className="text-sm font-medium mb-2">Recommended Secondary Hosts:</div>
                  {compatResult.recommended_secondary_hosts.map((h, i) => (
                    <div key={i} className="text-sm text-[var(--zf-ink)]">{h.hostname} (score: {h.score.toFixed(1)})</div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {/* Events Tab */}
      {activeTab === 'events' && (
        <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg">
          <div className="p-4 border-b border-[var(--zf-hairline)]">
            <h2 className="text-lg font-semibold">FT Events Timeline</h2>
          </div>
          {events.length === 0 ? (
            <div className="p-8 text-center text-[var(--zf-muted)]">No FT events recorded.</div>
          ) : (
            <div className="divide-y divide-[var(--zf-hairline)]">
              {events.map(evt => (
                <div key={`${evt.vm_name}-${evt.started_at}`} className="p-4 flex items-start gap-4">
                  <div className={`w-3.5 h-3.5 rounded-full mt-1 ${evt.status === 'success' ? 'bg-emerald-500' : 'bg-red-500'}`} />
                  <div className="flex-1">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="font-medium">{evt.vm_name}</span>
                      <span className={`px-2 py-0.5 rounded text-xs font-medium border ${getStatusColor(evt.status)}`}>{evt.status}</span>
                      <span className="text-xs px-2 py-0.5 bg-[var(--zf-canvas)] rounded">{evt.failover_type}</span>
                    </div>
                    <div className="text-sm text-[var(--zf-muted)]">
                      {evt.source_host_id} &rarr; {evt.target_host_id}
                      {evt.downtime_ms > 0 && <span className="ml-2">| Downtime: {evt.downtime_ms}ms</span>}
                    </div>
                    <div className="text-xs text-[var(--zf-muted)] mt-1">{new Date(evt.started_at).toLocaleString()}</div>
                    {evt.error && <div className="text-sm text-red-600 mt-1">{evt.error}</div>}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Metrics Tab */}
      {activeTab === 'metrics' && (
        <div className="space-y-4">
          {ftVMs.length === 0 ? (
            <div className="text-center py-12 text-[var(--zf-muted)] bg-[var(--zf-surface)] rounded-lg">No FT-protected VMs to show metrics for.</div>
          ) : ftVMs.map(vm => {
            const m = metrics.get(vm.vm_name)
            if (!m) return null
            return (
              <div key={vm.vm_name} className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg p-4">
                <h3 className="font-semibold mb-3">{vm.vm_name}</h3>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                  <div>
                    <div className="text-xs text-[var(--zf-muted)]">Log Bandwidth</div>
                    <div className="text-lg font-bold">{m.log_bandwidth_usage_mbps.toFixed(1)} Mbps</div>
                  </div>
                  <div>
                    <div className="text-xs text-[var(--zf-muted)]">Checkpoint Latency</div>
                    <div className="text-lg font-bold">{m.checkpoint_latency_ms.toFixed(0)} ms</div>
                  </div>
                  <div>
                    <div className="text-xs text-[var(--zf-muted)]">Secondary CPU</div>
                    <div className="text-lg font-bold">{m.secondary_cpu_usage_pct.toFixed(1)}%</div>
                  </div>
                  <div>
                    <div className="text-xs text-[var(--zf-muted)]">Secondary Memory</div>
                    <div className="text-lg font-bold">{m.secondary_memory_usage_pct.toFixed(1)}%</div>
                  </div>
                  <div>
                    <div className="text-xs text-[var(--zf-muted)]">Failover Count</div>
                    <div className="text-lg font-bold">{m.failover_count}</div>
                  </div>
                  <div>
                    <div className="text-xs text-[var(--zf-muted)]">Uptime</div>
                    <div className="text-lg font-bold">{m.uptime_pct.toFixed(2)}%</div>
                  </div>
                </div>
              </div>
            )
          })}
        </div>
      )}

      {/* Enable FT Modal */}
      {showEnableFT && (
        <EnableFTModal onClose={() => setShowEnableFT(false)} onCreated={() => { setShowEnableFT(false); loadData() }} />
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

function EnableFTModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [vmName, setVmName] = useState('')
  const [primaryHostId, setPrimaryHostId] = useState('local')
  const [secondaryHostId, setSecondaryHostId] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await enableFt({
        vm_name: vmName,
        primary_host_id: primaryHostId,
        secondary_host_id: secondaryHostId || 'auto',
      })
      onCreated()
    } catch { toast.error('Failed to enable FT') }
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-[var(--zf-surface)] rounded-lg p-6 w-full max-w-md">
        <h2 className="text-xl font-bold mb-4">Enable Fault Tolerance</h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">VM name</label>
            <input aria-label="VM name" type="text" value={vmName} onChange={e => setVmName(e.target.value)}
              className="input-field" required />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Primary host ID</label>
            <input aria-label="Primary host ID" type="text" value={primaryHostId} onChange={e => setPrimaryHostId(e.target.value)}
              className="input-field" required />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Secondary host ID</label>
            <input aria-label="Secondary host ID" type="text" value={secondaryHostId} onChange={e => setSecondaryHostId(e.target.value)}
              className="input-field"
              placeholder="Auto-select if empty" />
          </div>
          <div className="flex gap-3">
            <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
            <button type="submit" className="flex-1 zf-btn zf-btn-primary">Enable</button>
          </div>
        </form>
      </div>
    </div>
  )
}
