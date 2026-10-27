// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect } from 'react'
import { Plus, Trash2, RefreshCw, Check, Pencil } from 'lucide-react'
import * as api from '../../api/network-security'
import type {
  MonitorPolicy, CreateMonitorPolicyRequest, MonitorThreshold,
  NetworkMetrics, BandwidthAlert, AlertSeverity, MetricDirection,
} from '../../api/network-security'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from '../network/ModalShared'
import { LabelSelectorInput, LabelTags, StatusBadge } from './ModalShared'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

interface MonitorTabProps {
  policies: MonitorPolicy[]
  metrics: NetworkMetrics[]
  alerts: BandwidthAlert[]
  onDelete: (id: string) => void
  onAdopt?: (id: string) => void
  onEdit?: (id: string) => void
  onAcknowledge: (id: string) => void
  onCreate: () => void
  onSync: () => void
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`
  return `${(bytes / 1073741824).toFixed(1)} GB`
}

function MonitorTabContent({ policies, metrics, alerts, onDelete, onAdopt, onEdit, onAcknowledge, onCreate, onSync }: MonitorTabProps) {
  const readOnly = useReadOnly()
  const [view, setView] = useState<'policies' | 'metrics' | 'alerts'>('policies')
  const [status, setStatus] = useState<{ active_policies: number; monitored_vms: number } | null>(null)

  const refreshStatus = () => { api.monitorStatus().then(setStatus).catch(() => {}) }
  useEffect(() => { refreshStatus() }, [])
  const handleSyncClick = async () => { await onSync(); refreshStatus() }

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <div className="flex items-center gap-4">
          <h2 className="text-xl font-semibold">Network Monitor</h2>
          <div className="flex bg-[var(--zf-surface)] rounded-lg p-0.5">
            {(['policies', 'metrics', 'alerts'] as const).map(v => (
              <button key={v} onClick={() => setView(v)} className={`px-3 py-1 rounded text-sm transition ${view === v ? 'bg-[var(--zf-canvas-alt)] text-[var(--zf-ink)]' : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'}`}>
                {v.charAt(0).toUpperCase() + v.slice(1)}
                {v === 'alerts' && alerts.filter(a => !a.acknowledged).length > 0 && (
                  <span className="ml-1 bg-red-500 text-white text-xs rounded-full px-1.5">{alerts.filter(a => !a.acknowledged).length}</span>
                )}
              </button>
            ))}
          </div>
        </div>
        <div className="flex items-center gap-2">
          {status && (
            <span className="text-xs text-[var(--zf-muted)] bg-[var(--zf-surface)] rounded-lg px-3 py-1.5 border border-[var(--zf-hairline)]">
              {status.active_policies} active &middot; {status.monitored_vms} monitored VMs
            </span>
          )}
          {!readOnly && <button onClick={handleSyncClick} className="flex items-center gap-2 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
            <RefreshCw className="w-4 h-4" /> Sync
          </button>}
          {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] text-white py-2 px-4 rounded-lg transition text-sm">
            <Plus className="w-4 h-4" /> Add Policy
          </button>}
        </div>
      </div>

      {view === 'policies' && (
        policies.length === 0 ? (
          <div className="p-12 text-center text-[var(--zf-muted)]">No monitor policies configured. Create one to monitor VM network metrics.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-[var(--zf-surface)]">
                <tr>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Labels</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Thresholds</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Interval</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Status</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {policies.map(p => (
                  <tr key={p.id} className="hover:bg-white/[0.03] transition">
                    <td className="p-4">
                      <div className="font-medium">{p.name}{isHostManaged(p) && <HostBadge />}</div>
                      {p.description && <div className="text-xs text-[var(--zf-muted)] mt-1">{p.description}</div>}
                    </td>
                    <td className="p-4"><LabelTags labels={p.labels ?? p.selector?.match_labels} /></td>
                    <td className="p-4 font-medium text-cyan-400">{p.thresholds.length}</td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">{p.sample_interval_secs ?? p.interval_seconds ?? 10}s</td>
                    <td className="p-4">
                      <StatusBadge status={p.enabled ? 'active' : 'disabled'} color={p.enabled ? 'green' : 'gray'} />
                    </td>
                    <td className="p-4">
                      <div className="flex items-center gap-1">
                        {!readOnly && !isHostManaged(p) && onEdit && (
                          <button onClick={() => onEdit(p.id)} className="p-2 hover:bg-[var(--zf-hairline)] rounded transition" title="Edit">
                            <Pencil className="w-4 h-4" />
                          </button>
                        )}
                        <HostManagedActions readOnly={readOnly}
                          item={{ id: p.id, managed: p.managed }}
                          onDelete={() => onDelete(p.id)}
                          onAdopt={onAdopt ? () => onAdopt(p.id) : undefined}
                        />
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      )}

      {view === 'metrics' && (
        metrics.length === 0 ? (
          <div className="p-12 text-center text-[var(--zf-muted)]">No metrics available.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-[var(--zf-surface)]">
                <tr>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">VM</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Interface</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">RX</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">TX</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">RX Pkts</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">TX Pkts</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Errors</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Timestamp</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {metrics.map((m, i) => (
                  <tr key={i} className="hover:bg-white/[0.03] transition">
                    <td className="p-4 font-medium">{m.vm_name}</td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-link)]">{m.interface_name}</td>
                    <td className="p-4 font-mono text-sm text-emerald-600">{formatBytes(m.rx_bytes)}</td>
                    <td className="p-4 font-mono text-sm text-amber-600">{formatBytes(m.tx_bytes)}</td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">{m.rx_packets}</td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">{m.tx_packets}</td>
                    <td className="p-4 font-mono text-sm">
                      {m.rx_errors + m.tx_errors > 0 ? (
                        <span className="text-red-600">{m.rx_errors + m.tx_errors}</span>
                      ) : (
                        <span className="text-[var(--zf-muted)]">0</span>
                      )}
                    </td>
                    <td className="p-4 text-xs text-[var(--zf-muted)]">{new Date(m.timestamp).toLocaleString()}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      )}

      {view === 'alerts' && (
        alerts.length === 0 ? (
          <div className="p-12 text-center text-[var(--zf-muted)]">No bandwidth alerts.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-[var(--zf-surface)]">
                <tr>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">VM</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Metric</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Value</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Threshold</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Severity</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Time</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {alerts.map(a => (
                  <tr key={a.id} className={`hover:bg-white/[0.03] transition ${a.acknowledged ? 'opacity-50' : ''}`}>
                    <td className="p-4 font-medium">{a.vm_name}</td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">{a.metric}</td>
                    <td className="p-4 font-mono text-sm text-red-600">{a.value}</td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">{a.threshold}</td>
                    <td className="p-4">
                      <StatusBadge
                        status={a.severity}
                        color={a.severity === 'critical' ? 'red' : a.severity === 'warning' ? 'yellow' : 'blue'}
                      />
                    </td>
                    <td className="p-4 text-xs text-[var(--zf-muted)]">{new Date(a.created).toLocaleString()}</td>
                    <td className="p-4">
                      {!readOnly && !a.acknowledged && (
                        <button onClick={() => onAcknowledge(a.id)} className="p-2 hover:bg-green-600 rounded transition" title="Acknowledge">
                          <Check className="w-4 h-4" />
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      )}
    </div>
  )
}

export function CreateMonitorPolicyModal({ onClose, onCreated }: { onClose: () => void; onCreated: (p: MonitorPolicy) => void }) {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [labels, setLabels] = useState<Record<string, string>>({})
  const [interval, setInterval] = useState('60')
  const [thresholds, setThresholds] = useState<MonitorThreshold[]>([])
  const [thMetric, setThMetric] = useState('bandwidth')
  const [thValue, setThValue] = useState('')
  const [thUnit, setThUnit] = useState('mbps')
  const [thDirection, setThDirection] = useState<MetricDirection>('both')
  const [thSeverity, setThSeverity] = useState<AlertSeverity>('warning')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const addThreshold = () => {
    if (!thValue) return
    setThresholds(prev => [...prev, {
      metric: thMetric,
      value: parseFloat(thValue),
      unit: thUnit,
      direction: thDirection,
      severity: thSeverity,
    }])
    setThValue('')
  }

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateMonitorPolicyRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        selector: { match_labels: labels },
        thresholds,
        sample_interval_secs: parseInt(interval) || 60,
      }
      const p = await api.createMonitorPolicy(req)
      onCreated(p)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create Monitor Policy" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="high-bandwidth-alert" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="Alert on high bandwidth" />
        <LabelSelectorInput labels={labels} onChange={setLabels} />
        <InputField label="Interval (seconds)" value={interval} onChange={setInterval} placeholder="60" type="number" />
        <div className="border border-[var(--zf-hairline)] rounded-lg p-4 space-y-3">
          <div className="text-sm font-medium text-[var(--zf-ink)]">Add Threshold</div>
          <div className="grid grid-cols-2 gap-2">
            <div>
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Metric</label>
              <select aria-label="Metric" value={thMetric} onChange={e => setThMetric(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
                <option value="bandwidth">Bandwidth</option>
                <option value="packets">Packets</option>
                <option value="errors">Errors</option>
                <option value="drops">Drops</option>
              </select>
            </div>
            <InputField label="Value" value={thValue} onChange={setThValue} placeholder="100" type="number" />
          </div>
          <div className="grid grid-cols-3 gap-2">
            <div>
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Unit</label>
              <select aria-label="Unit" value={thUnit} onChange={e => setThUnit(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
                <option value="mbps">Mbps</option>
                <option value="gbps">Gbps</option>
                <option value="kpps">Kpps</option>
                <option value="count">Count</option>
              </select>
            </div>
            <div>
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Direction</label>
              <select aria-label="Direction" value={thDirection} onChange={e => setThDirection(e.target.value as MetricDirection)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
                <option value="both">Both</option>
                <option value="inbound">Inbound</option>
                <option value="outbound">Outbound</option>
              </select>
            </div>
            <div>
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Severity</label>
              <select aria-label="Severity" value={thSeverity} onChange={e => setThSeverity(e.target.value as AlertSeverity)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
                <option value="info">Info</option>
                <option value="warning">Warning</option>
                <option value="critical">Critical</option>
              </select>
            </div>
          </div>
          <button type="button" onClick={addThreshold} className="flex items-center gap-1 text-sm text-[var(--zf-link)] hover:text-blue-300 transition">
            <Plus className="w-3.5 h-3.5" /> Add Threshold
          </button>
          {thresholds.length > 0 && (
            <div className="space-y-1 mt-2">
              {thresholds.map((t, i) => (
                <div key={i} className="flex items-center gap-2 text-xs bg-[var(--zf-surface)] rounded px-2 py-1">
                  <StatusBadge status={t.severity} color={t.severity === 'critical' ? 'red' : t.severity === 'warning' ? 'yellow' : 'blue'} />
                  <span className="text-[var(--zf-ink)]">{t.metric}</span>
                  <span className="text-[var(--zf-muted)]">{t.value} {t.unit}</span>
                  <span className="text-[var(--zf-muted)]">{t.direction}</span>
                  <button onClick={() => setThresholds(prev => prev.filter((_, j) => j !== i))} className="ml-auto text-red-600 hover:text-red-300" aria-label="Delete" title="Delete">
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create Monitor Policy'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export function EditMonitorPolicyModal({ id, onClose, onUpdated }: { id: string; onClose: () => void; onUpdated: (p: MonitorPolicy) => void }) {
  const [loading, setLoading] = useState(true)
  const [loadErr, setLoadErr] = useState('')
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [labels, setLabels] = useState<Record<string, string>>({})
  const [interval, setInterval] = useState('60')
  const [thresholds, setThresholds] = useState<MonitorThreshold[]>([])
  const [thMetric, setThMetric] = useState('bandwidth')
  const [thValue, setThValue] = useState('')
  const [thUnit, setThUnit] = useState('mbps')
  const [thDirection, setThDirection] = useState<MetricDirection>('both')
  const [thSeverity, setThSeverity] = useState<AlertSeverity>('warning')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  useEffect(() => {
    let cancelled = false
    api.getMonitorPolicy(id).then(p => {
      if (cancelled) return
      setName(p.name)
      setDescription(p.description ?? '')
      setLabels(p.labels ?? p.selector?.match_labels ?? {})
      setInterval(String(p.interval_seconds ?? p.sample_interval_secs ?? 60))
      setThresholds(p.thresholds ?? [])
      setLoading(false)
    }).catch((e: unknown) => {
      if (cancelled) return
      setLoadErr(extractErrorMessage(e))
      setLoading(false)
    })
    return () => { cancelled = true }
  }, [id])

  const addThreshold = () => {
    if (!thValue) return
    setThresholds(prev => [...prev, {
      metric: thMetric,
      value: parseFloat(thValue),
      unit: thUnit,
      direction: thDirection,
      severity: thSeverity,
    }])
    setThValue('')
  }

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateMonitorPolicyRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        selector: { match_labels: labels },
        thresholds,
        sample_interval_secs: parseInt(interval) || 60,
      }
      const p = await api.updateMonitorPolicy(id, req)
      onUpdated(p)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  if (loading) {
    return (
      <ModalWrapper title="Edit Monitor Policy" onClose={onClose}>
        <div className="text-[var(--zf-muted)] text-sm">Loading...</div>
      </ModalWrapper>
    )
  }
  if (loadErr) {
    return (
      <ModalWrapper title="Edit Monitor Policy" onClose={onClose}>
        <p className="text-red-600 text-sm">{loadErr}</p>
      </ModalWrapper>
    )
  }

  return (
    <ModalWrapper title="Edit Monitor Policy" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="high-bandwidth-alert" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="Alert on high bandwidth" />
        <LabelSelectorInput labels={labels} onChange={setLabels} />
        <InputField label="Interval (seconds)" value={interval} onChange={setInterval} placeholder="60" type="number" />
        <div className="border border-[var(--zf-hairline)] rounded-lg p-4 space-y-3">
          <div className="text-sm font-medium text-[var(--zf-ink)]">Add Threshold</div>
          <div className="grid grid-cols-2 gap-2">
            <div>
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Metric</label>
              <select aria-label="Metric" value={thMetric} onChange={e => setThMetric(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
                <option value="bandwidth">Bandwidth</option>
                <option value="packets">Packets</option>
                <option value="errors">Errors</option>
                <option value="drops">Drops</option>
              </select>
            </div>
            <InputField label="Value" value={thValue} onChange={setThValue} placeholder="100" type="number" />
          </div>
          <div className="grid grid-cols-3 gap-2">
            <div>
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Unit</label>
              <select aria-label="Unit" value={thUnit} onChange={e => setThUnit(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
                <option value="mbps">Mbps</option>
                <option value="gbps">Gbps</option>
                <option value="kpps">Kpps</option>
                <option value="count">Count</option>
              </select>
            </div>
            <div>
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Direction</label>
              <select aria-label="Direction" value={thDirection} onChange={e => setThDirection(e.target.value as MetricDirection)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
                <option value="both">Both</option>
                <option value="inbound">Inbound</option>
                <option value="outbound">Outbound</option>
              </select>
            </div>
            <div>
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Severity</label>
              <select aria-label="Severity" value={thSeverity} onChange={e => setThSeverity(e.target.value as AlertSeverity)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
                <option value="info">Info</option>
                <option value="warning">Warning</option>
                <option value="critical">Critical</option>
              </select>
            </div>
          </div>
          <button type="button" onClick={addThreshold} className="flex items-center gap-1 text-sm text-[var(--zf-link)] hover:text-blue-300 transition">
            <Plus className="w-3.5 h-3.5" /> Add Threshold
          </button>
          {thresholds.length > 0 && (
            <div className="space-y-1 mt-2">
              {thresholds.map((t, i) => (
                <div key={i} className="flex items-center gap-2 text-xs bg-[var(--zf-surface)] rounded px-2 py-1">
                  <StatusBadge status={t.severity} color={t.severity === 'critical' ? 'red' : t.severity === 'warning' ? 'yellow' : 'blue'} />
                  <span className="text-[var(--zf-ink)]">{t.metric}</span>
                  <span className="text-[var(--zf-muted)]">{t.value} {t.unit}</span>
                  <span className="text-[var(--zf-muted)]">{t.direction}</span>
                  <button onClick={() => setThresholds(prev => prev.filter((_, j) => j !== i))} className="ml-auto text-red-600 hover:text-red-300" aria-label="Delete" title="Delete">
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Saving...' : 'Save Changes'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default MonitorTabContent
