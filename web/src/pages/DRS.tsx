// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Plus, Check, X, Trash2 } from 'lucide-react'
import {
  getDrsConfig,
  configureDrs,
  analyzeBalance,
  listRecommendations,
  approveRecommendation,
  rejectRecommendation,
  listAffinityRules,
  createAffinityRule,
  deleteAffinityRule,
  computePlacement,
  type DrsConfig,
  type ClusterBalance,
  type MigrationRecommendation,
  type AffinityRule,
  type PlacementResult,
} from '../api/drs'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'
import { PageHeader } from '../components/ui'

export default function DRS() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [clusterId] = useState('default')
  const [config, setConfig] = useState<DrsConfig | null>(null)
  const [balance, setBalance] = useState<ClusterBalance | null>(null)
  const [recommendations, setRecommendations] = useState<MigrationRecommendation[]>([])
  const [rules, setRules] = useState<AffinityRule[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load DRS data')
  const [showCreateRule, setShowCreateRule] = useState(false)
  const [placementResult, setPlacementResult] = useState<PlacementResult | null>(null)
  const [activeTab, setActiveTab] = useState<'balance' | 'recommendations' | 'rules' | 'placement'>('balance')

  const loadData = useCallback(() => {
    return run(async () => {
      const [cfg, bal, recs, rls] = await Promise.all([
        getDrsConfig(clusterId).catch(() => null),
        analyzeBalance(clusterId).catch(() => null),
        listRecommendations(clusterId).catch(() => []),
        listAffinityRules(clusterId).catch(() => []),
      ])
      setConfig(cfg)
      setBalance(bal)
      setRecommendations(recs)
      setRules(rls)
    })
  }, [run, clusterId])

  useEffect(() => {
    void loadData()
  }, [loadData])

  const handleConfigUpdate = async (updates: Partial<DrsConfig>) => {
    try {
      const updated = await configureDrs({ cluster_id: clusterId, ...updates })
      setConfig(updated)
      toast.success('DRS configuration updated')
    } catch { toast.error('Failed to update DRS config') }
  }

  const handleApprove = async (id: string) => {
    try {
      await approveRecommendation(id)
      toast.success('Migration approved')
      loadData()
    } catch { toast.error('Failed to approve migration') }
  }

  const handleReject = async (id: string) => {
    const ok = await confirm('Reject Recommendation', 'Reject this DRS migration recommendation?', { variant: 'danger', confirmLabel: 'Reject' })
    if (!ok) return
    try {
      await rejectRecommendation(id)
      toast.success('Migration rejected')
      loadData()
    } catch { toast.error('Failed to reject migration') }
  }

  const handleDeleteRule = async (id: string) => {
    const ok = await confirm('Delete Affinity Rule', 'Delete this affinity rule?', { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return
    try {
      await deleteAffinityRule(id)
      toast.success('Affinity rule deleted')
      loadData()
    } catch { toast.error('Failed to delete rule') }
  }

  const handlePlacementTest = async (vmCpus: number, vmMemoryMb: number) => {
    try {
      const result = await computePlacement({ cluster_id: clusterId, vm_cpus: vmCpus, vm_memory_mb: vmMemoryMb })
      setPlacementResult(result)
    } catch { toast.error('Placement test failed') }
  }

  const getPriorityColor = (p: string) => {
    const colors: Record<string, string> = {
      critical: 'text-red-700 bg-red-50 border border-red-200',
      high: 'text-amber-800 bg-amber-50 border border-amber-200',
      medium: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border border-[var(--zf-hairline)]',
      low: 'text-emerald-700 bg-emerald-50 border border-emerald-200',
    }
    return colors[p] || 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border border-[var(--zf-hairline)]'
  }


  return (
    <div className="p-6">
      <PageHeader
        title="DRS"
        description="Distributed Resource Scheduler — cluster balance, migration recommendations, and affinity rules"
        onRefresh={() => void loadData()}
        refreshing={loading}
      />

      <PageLoadBanner title="Could not load DRS data" headline={loadError} onRetry={() => void loadData()} />

      {/* DRS Configuration */}
      {config && (
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg p-4 mb-6">
          <h2 className="text-lg font-semibold mb-4">DRS Configuration</h2>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <div>
              <label className="block text-sm text-[var(--zf-muted)] mb-1">Enabled</label>
              <button
                onClick={() => handleConfigUpdate({ enabled: !config.enabled })}
                className={`px-3 py-1 rounded text-sm border ${config.enabled ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'}`}
              >{config.enabled ? 'On' : 'Off'}</button>
            </div>
            <div>
              <label className="block text-sm text-[var(--zf-muted)] mb-1">Automation Level</label>
              <select aria-label="Automation Level" value={config.automation_level}
                onChange={e => handleConfigUpdate({ automation_level: e.target.value as DrsConfig['automation_level'] })}
                className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-1 text-sm">
                <option value="manual">Manual</option>
                <option value="partially_automated">Semi-Auto</option>
                <option value="fully_automated">Fully Auto</option>
              </select>
            </div>
            <div>
              <label className="block text-sm text-[var(--zf-muted)] mb-1">Migration Threshold</label>
              <input type="range" min={1} max={5} value={config.migration_threshold}
                onChange={e => handleConfigUpdate({ migration_threshold: Number(e.target.value) })}
                className="w-full" />
              <span className="text-xs text-[var(--zf-muted)]">{config.migration_threshold}/5</span>
            </div>
            <div>
              <label className="block text-sm text-[var(--zf-muted)] mb-1">Check Interval</label>
              <span className="text-sm">{config.check_interval_seconds}s</span>
            </div>
          </div>
        </div>
      )}

      {/* Tabs */}
      <div className="flex gap-1 mb-4 bg-[var(--zf-canvas)] rounded-lg p-1">
        {(['balance', 'recommendations', 'rules', 'placement'] as const).map(tab => (
          <button key={tab} onClick={() => setActiveTab(tab)}
            className={`flex-1 px-4 py-2 rounded text-sm font-medium capitalize ${activeTab === tab ? 'bg-[var(--zf-link-fill)] text-white' : 'text-[var(--zf-muted)] hover:bg-black/[0.04]'}`}>
            {tab}
          </button>
        ))}
      </div>

      {/* Balance Tab */}
      {activeTab === 'balance' && !balance && !loading && (
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg p-8 text-center text-[var(--zf-muted)]">
          No cluster balance data available. Balance analysis needs at least one cluster with hosts registered.
        </div>
      )}

      {activeTab === 'balance' && balance && (
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg p-4">
          <div className="flex justify-between items-center mb-4">
            <h2 className="text-lg font-semibold">Cluster Balance</h2>
            <div className="text-sm text-[var(--zf-muted)]">
              Score: <span className="font-bold text-[var(--zf-ink)]">{balance.overall_score.toFixed(1)}</span> |
              CPU Imbalance: {balance.cpu_imbalance_pct.toFixed(1)}% |
              Memory Imbalance: {balance.memory_imbalance_pct.toFixed(1)}%
            </div>
          </div>
          <div className="space-y-3">
            {balance.hosts.map(host => (
              <div key={host.host_id} className="p-3 bg-[var(--zf-canvas)] rounded">
                <div className="flex items-center justify-between mb-2">
                  <span className="font-medium">{host.hostname}</span>
                  <span className="text-sm text-[var(--zf-muted)]">{host.vm_count} VMs</span>
                </div>
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <div className="flex justify-between text-xs text-[var(--zf-muted)] mb-1">
                      <span>CPU</span>
                      <span>{host.cpu_usage_pct.toFixed(1)}%</span>
                    </div>
                    <div className="w-full bg-[var(--zf-surface)] rounded-full h-4">
                      <div className={`h-4 rounded-full text-xs text-center leading-4 text-white ${host.cpu_usage_pct > 80 ? 'bg-[var(--zf-danger)]' : host.cpu_usage_pct > 60 ? 'bg-[var(--zf-warning)]' : 'bg-[var(--zf-link-fill)]'}`}
                        style={{ width: `${Math.min(host.cpu_usage_pct, 100)}%` }}>
                        {host.cpu_usage_pct > 20 ? `${host.cpu_usage_pct.toFixed(0)}%` : ''}
                      </div>
                    </div>
                  </div>
                  <div>
                    <div className="flex justify-between text-xs text-[var(--zf-muted)] mb-1">
                      <span>Memory</span>
                      <span>{host.memory_usage_pct.toFixed(1)}%</span>
                    </div>
                    <div className="w-full bg-[var(--zf-surface)] rounded-full h-4">
                      <div className={`h-4 rounded-full text-xs text-center leading-4 text-white ${host.memory_usage_pct > 80 ? 'bg-[var(--zf-danger)]' : host.memory_usage_pct > 60 ? 'bg-[var(--zf-warning)]' : 'bg-[var(--zf-ink)]'}`}
                        style={{ width: `${Math.min(host.memory_usage_pct, 100)}%` }}>
                        {host.memory_usage_pct > 20 ? `${host.memory_usage_pct.toFixed(0)}%` : ''}
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Recommendations Tab */}
      {activeTab === 'recommendations' && (
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg">
          <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
            <thead>
              <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                <th className="p-4">VM</th>
                <th className="p-4">From</th>
                <th className="p-4">To</th>
                <th className="p-4">Reason</th>
                <th className="p-4">Priority</th>
                <th className="p-4">Benefit</th>
                <th className="p-4">Status</th>
                <th className="p-4">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {recommendations.length === 0 ? (
                <tr><td colSpan={8} className="p-8 text-center text-[var(--zf-muted)]">No migration recommendations.</td></tr>
              ) : recommendations.map(rec => (
                <tr key={rec.id} className="hover:bg-[var(--zf-surface)]">
                  <td className="p-4 font-medium">{rec.vm_name}</td>
                  <td className="p-4 text-sm text-[var(--zf-muted)]">{rec.source_hostname}</td>
                  <td className="p-4 text-sm text-[var(--zf-muted)]">{rec.target_hostname}</td>
                  <td className="p-4 text-sm">{rec.reason}</td>
                  <td className="p-4">
                    <span className={`px-2 py-1 rounded text-xs font-medium ${getPriorityColor(rec.priority)}`}>
                      {rec.priority}
                    </span>
                  </td>
                  <td className="p-4 text-sm">{rec.estimated_benefit_pct.toFixed(1)}%</td>
                  <td className="p-4 text-sm">{rec.status}</td>
                  <td className="p-4">
                    {rec.status === 'pending' && (
                      <div className="flex gap-2">
                        <button onClick={() => handleApprove(rec.id)} className="p-1 text-emerald-600 hover:text-emerald-700" aria-label="Confirm" title="Confirm">
                          <Check className="w-4 h-4" />
                        </button>
                        <button onClick={() => handleReject(rec.id)} className="p-1 text-red-600 hover:text-red-700" aria-label="Reject recommendation" title="Reject recommendation">
                          <X className="w-4 h-4" />
                        </button>
                      </div>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Rules Tab */}
      {activeTab === 'rules' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreateRule(true)} className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Create Rule
            </button>
          </div>
          <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Name</th>
                  <th className="p-4">Type</th>
                  <th className="p-4">VMs</th>
                  <th className="p-4">Mandatory</th>
                  <th className="p-4">Enabled</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {rules.length === 0 ? (
                  <tr><td colSpan={6} className="p-8 text-center text-[var(--zf-muted)]">No affinity rules configured.</td></tr>
                ) : rules.map(rule => (
                  <tr key={rule.id} className="hover:bg-[var(--zf-surface)]">
                    <td className="p-4 font-medium">{rule.name}</td>
                    <td className="p-4">
                      <span className={`px-2 py-1 rounded text-xs font-medium border ${rule.rule_type === 'affinity' ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-red-700 bg-red-50 border-red-200'}`}>
                        {rule.rule_type}
                      </span>
                    </td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">{rule.vm_ids.length} VMs</td>
                    <td className="p-4 text-sm">{rule.mandatory ? 'Yes' : 'No'}</td>
                    <td className="p-4 text-sm">{rule.enabled ? 'Yes' : 'No'}</td>
                    <td className="p-4">
                      <button onClick={() => handleDeleteRule(rule.id)} className="text-red-600 hover:text-red-800" aria-label={`Delete ${rule.name}`} title={`Delete ${rule.name}`}>
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

      {/* Placement Tab */}
      {activeTab === 'placement' && (
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg p-4">
          <h2 className="text-lg font-semibold mb-4">Placement Calculator</h2>
          <PlacementForm clusterId={clusterId} result={placementResult} onTest={handlePlacementTest} />
        </div>
      )}

      {/* Create Rule Modal */}
      {showCreateRule && (
        <CreateRuleModal
          clusterId={clusterId}
          onClose={() => setShowCreateRule(false)}
          onCreated={() => { setShowCreateRule(false); loadData() }}
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

function PlacementForm({ result, onTest }: { clusterId: string; result: PlacementResult | null; onTest: (cpu: number, mem: number) => void }) {
  const [cpus, setCpus] = useState(2)
  const [memory, setMemory] = useState(4096)

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-4">
        <div>
          <label className="block text-sm font-medium mb-1">VM CPUs (MHz)</label>
          <input aria-label="VM CPUs (MHz)" type="number" value={cpus} onChange={e => setCpus(Number(e.target.value))}
            className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1">VM Memory (MB)</label>
          <input aria-label="VM Memory (MB)" type="number" value={memory} onChange={e => setMemory(Number(e.target.value))}
            className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" />
        </div>
      </div>
      <button onClick={() => onTest(cpus, memory)} className="zf-btn zf-btn-primary">Test Placement</button>
      {result && (
        <div className="p-4 bg-[var(--zf-canvas)] rounded border border-[var(--zf-hairline)]">
          <div className="font-medium mb-2">Recommended: {result.hostname}</div>
          <div className="text-sm text-[var(--zf-muted)]">Score: {result.score.toFixed(2)} | Reason: {result.reason}</div>
          <div className="text-sm text-[var(--zf-muted)]">After placement: CPU {result.cpu_after_pct.toFixed(1)}% | Memory {result.memory_after_pct.toFixed(1)}%</div>
          {result.alternatives.length > 0 && (
            <div className="mt-2">
              <div className="text-xs text-[var(--zf-muted)]">Alternatives:</div>
              {result.alternatives.map((alt, i) => (
                <div key={i} className="text-xs text-[var(--zf-muted)]">{alt.hostname} (score: {alt.score.toFixed(2)})</div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  )
}

function CreateRuleModal({ clusterId, onClose, onCreated }: { clusterId: string; onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [ruleType, setRuleType] = useState<'affinity' | 'anti_affinity'>('affinity')
  const [vmIds, setVmIds] = useState('')
  const [mandatory, setMandatory] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await createAffinityRule({
        cluster_id: clusterId,
        name,
        rule_type: ruleType,
        mandatory,
        vm_ids: vmIds.split(',').map(s => s.trim()).filter(Boolean),
      })
      onCreated()
    } catch { toast.error('Failed to create rule') }
  }

  return (
    <div className="modal-backdrop">
      <div className="modal-card w-full max-w-md">
        <h2 className="text-xl font-bold mb-4">Create Affinity Rule</h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">Name</label>
            <input aria-label="Name" type="text" value={name} onChange={e => setName(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" required />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Type</label>
            <select aria-label="Type" value={ruleType} onChange={e => setRuleType(e.target.value as 'affinity' | 'anti_affinity')}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2">
              <option value="affinity">Affinity (keep together)</option>
              <option value="anti_affinity">Anti-Affinity (keep apart)</option>
            </select>
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">VM IDs (comma-separated)</label>
            <input aria-label="VM IDs (comma-separated)" type="text" value={vmIds} onChange={e => setVmIds(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" required />
          </div>
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={mandatory} onChange={e => setMandatory(e.target.checked)} />
            <span className="text-sm">Mandatory</span>
          </label>
          <div className="flex gap-3">
            <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
            <button type="submit" className="zf-btn zf-btn-primary flex-1">Create</button>
          </div>
        </form>
      </div>
    </div>
  )
}
