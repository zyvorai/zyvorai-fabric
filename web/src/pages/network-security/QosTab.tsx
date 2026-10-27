// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect } from 'react'
import { Plus, RefreshCw, Pencil } from 'lucide-react'
import * as api from '../../api/network-security'
import type { QoSPolicy, CreateQoSPolicyRequest } from '../../api/network-security'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from '../network/ModalShared'
import { LabelSelectorInput, StatusBadge } from './ModalShared'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

function formatRate(r: { value: number; unit: string }): string {
  return `${r.value}${r.unit}`
}

interface QosTabProps {
  policies: QoSPolicy[]
  onDelete: (id: string) => void
  onAdopt?: (id: string) => void
  onEdit?: (id: string) => void
  onCreate: () => void
  onSync: () => void
}

function QosTabContent({ policies, onDelete, onAdopt, onEdit, onCreate, onSync }: QosTabProps) {
  const readOnly = useReadOnly()
  const [status, setStatus] = useState<{ active_policies: number; shaped_vms: number } | null>(null)

  const refreshStatus = () => { api.qosStatus().then(setStatus).catch(() => {}) }
  useEffect(() => { refreshStatus() }, [])
  const handleSyncClick = async () => { await onSync(); refreshStatus() }

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <h2 className="text-xl font-semibold">QoS / Traffic Shaping</h2>
        <div className="flex items-center gap-2">
          {status && (
            <span className="text-xs text-[var(--zf-muted)] bg-[var(--zf-surface)] rounded-lg px-3 py-1.5 border border-[var(--zf-hairline)]">
              {status.active_policies} active &middot; {status.shaped_vms} shaped VMs
            </span>
          )}
          {!readOnly && <button onClick={handleSyncClick} className="flex items-center gap-2 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
            <RefreshCw className="w-4 h-4" /> Sync
          </button>}
          {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-[var(--zf-link)] hover:bg-[var(--zf-link-hover)] text-white py-2 px-4 rounded-lg transition text-sm">
            <Plus className="w-4 h-4" /> Add QoS Policy
          </button>}
        </div>
      </div>
      {policies.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No QoS policies configured. Create one to shape VM network traffic.</div>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full">
            <thead className="bg-[var(--zf-surface)]">
              <tr>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Interface</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Class</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Guaranteed</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Max Rate</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Priority</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Status</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {policies.map(p => (
                <tr key={p.id} className="hover:bg-white/[0.03] transition">
                  <td className="p-4">
                    <div className="font-medium">{p.name}{isHostManaged(p) && <HostBadge />}</div>
                    {p.description && <div className="text-xs text-[var(--zf-muted)] mt-1 max-w-xs truncate">{p.description}</div>}
                  </td>
                  <td className="p-4 font-mono text-sm text-cyan-400">{p.interface}</td>
                  <td className="p-4 font-mono text-sm">{p.traffic_class?.name ?? '-'}</td>
                  <td className="p-4 font-mono text-sm text-emerald-600">
                    {p.traffic_class ? formatRate(p.traffic_class.guaranteed_rate) : '-'}
                  </td>
                  <td className="p-4 font-mono text-sm text-amber-600">
                    {p.traffic_class ? formatRate(p.traffic_class.max_rate) : '-'}
                  </td>
                  <td className="p-4 font-mono text-sm">{p.traffic_class?.priority ?? '-'}</td>
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
      )}
    </div>
  )
}

export function CreateQosModal({ onClose, onCreated }: { onClose: () => void; onCreated: (p: QoSPolicy) => void }) {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [iface, setIface] = useState('eth0')
  const [labels, setLabels] = useState<Record<string, string>>({})
  const [guaranteedRate, setGuaranteedRate] = useState('100')
  const [maxRate, setMaxRate] = useState('100')
  const [burst, setBurst] = useState('')
  const [priority, setPriority] = useState('4')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim() || !iface.trim()) { setErr('Name and interface are required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateQoSPolicyRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        interface: iface.trim(),
        selector: { match_labels: labels },
        traffic_class: {
          name: 'default',
          guaranteed_rate: { value: parseInt(guaranteedRate) || 100, unit: 'mbit' },
          max_rate: { value: parseInt(maxRate) || 100, unit: 'mbit' },
          burst: burst.trim() || undefined,
          priority: parseInt(priority) || 4,
        },
      }
      const p = await api.createQosPolicy(req)
      onCreated(p)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create QoS Policy" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="high-priority" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="High priority traffic shaping" />
        <InputField label="Interface" value={iface} onChange={setIface} placeholder="eth0" />
        <LabelSelectorInput labels={labels} onChange={setLabels} />
        <div className="grid grid-cols-2 gap-2">
          <InputField label="Guaranteed (mbit)" value={guaranteedRate} onChange={setGuaranteedRate} placeholder="100" type="number" />
          <InputField label="Max (mbit)" value={maxRate} onChange={setMaxRate} placeholder="1000" type="number" />
        </div>
        <div className="grid grid-cols-2 gap-2">
          <InputField label="Burst" value={burst} onChange={setBurst} placeholder="32kbit" />
          <InputField label="Priority" value={priority} onChange={setPriority} placeholder="100" type="number" />
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link)] hover:bg-[var(--zf-link-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create QoS Policy'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export function EditQosModal({ id, onClose, onUpdated }: { id: string; onClose: () => void; onUpdated: (p: QoSPolicy) => void }) {
  const [loading, setLoading] = useState(true)
  const [loadErr, setLoadErr] = useState('')
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [iface, setIface] = useState('eth0')
  const [labels, setLabels] = useState<Record<string, string>>({})
  const [guaranteedRate, setGuaranteedRate] = useState('100')
  const [maxRate, setMaxRate] = useState('100')
  const [burst, setBurst] = useState('')
  const [priority, setPriority] = useState('4')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  useEffect(() => {
    let cancelled = false
    api.getQosPolicy(id).then(p => {
      if (cancelled) return
      setName(p.name)
      setDescription(p.description ?? '')
      setIface(p.interface)
      setLabels(p.selector?.match_labels ?? {})
      setGuaranteedRate(String(p.traffic_class?.guaranteed_rate?.value ?? 100))
      setMaxRate(String(p.traffic_class?.max_rate?.value ?? 100))
      setBurst(p.traffic_class?.burst ?? '')
      setPriority(String(p.traffic_class?.priority ?? 4))
      setLoading(false)
    }).catch((e: unknown) => {
      if (cancelled) return
      setLoadErr(extractErrorMessage(e))
      setLoading(false)
    })
    return () => { cancelled = true }
  }, [id])

  const handleSubmit = async () => {
    if (!name.trim() || !iface.trim()) { setErr('Name and interface are required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateQoSPolicyRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        interface: iface.trim(),
        selector: { match_labels: labels },
        traffic_class: {
          name: 'default',
          guaranteed_rate: { value: parseInt(guaranteedRate) || 100, unit: 'mbit' },
          max_rate: { value: parseInt(maxRate) || 100, unit: 'mbit' },
          burst: burst.trim() || undefined,
          priority: parseInt(priority) || 4,
        },
      }
      const p = await api.updateQosPolicy(id, req)
      onUpdated(p)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  if (loading) {
    return (
      <ModalWrapper title="Edit QoS Policy" onClose={onClose}>
        <div className="text-[var(--zf-muted)] text-sm">Loading...</div>
      </ModalWrapper>
    )
  }
  if (loadErr) {
    return (
      <ModalWrapper title="Edit QoS Policy" onClose={onClose}>
        <p className="text-red-600 text-sm">{loadErr}</p>
      </ModalWrapper>
    )
  }

  return (
    <ModalWrapper title="Edit QoS Policy" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="high-priority" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="High priority traffic shaping" />
        <InputField label="Interface" value={iface} onChange={setIface} placeholder="eth0" />
        <LabelSelectorInput labels={labels} onChange={setLabels} />
        <div className="grid grid-cols-2 gap-2">
          <InputField label="Guaranteed (mbit)" value={guaranteedRate} onChange={setGuaranteedRate} placeholder="100" type="number" />
          <InputField label="Max (mbit)" value={maxRate} onChange={setMaxRate} placeholder="1000" type="number" />
        </div>
        <div className="grid grid-cols-2 gap-2">
          <InputField label="Burst" value={burst} onChange={setBurst} placeholder="32kbit" />
          <InputField label="Priority" value={priority} onChange={setPriority} placeholder="100" type="number" />
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link)] hover:bg-[var(--zf-link-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Saving...' : 'Save Changes'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default QosTabContent
