// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PanelLoading } from '../../components/Skeleton'
import { useState, useEffect } from 'react'
import { Plus, Trash2, RefreshCw, Pencil } from 'lucide-react'
import * as api from '../../api/network-security'
import type { DnsZone, CreateDnsZoneRequest, DnsPolicy, CreateDnsPolicyRequest, DnsRecordType } from '../../api/network-security'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from '../network/ModalShared'
import { LabelSelectorInput, LabelTags, StatusBadge } from './ModalShared'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

interface DnsTabProps {
  zones: DnsZone[]
  policies: DnsPolicy[]
  onDeleteZone: (id: string) => void
  onDeletePolicy: (id: string) => void
  onAdoptZone?: (id: string) => void
  onAdoptPolicy?: (id: string) => void
  onEditZone?: (id: string) => void
  onEditPolicy?: (id: string) => void
  onCreate: () => void
  onSync: () => void
}

function DnsTabContent({ zones, policies, onDeleteZone, onDeletePolicy, onAdoptZone, onAdoptPolicy, onEditZone, onEditPolicy, onCreate, onSync }: DnsTabProps) {
  const readOnly = useReadOnly()
  const [view, setView] = useState<'zones' | 'policies'>('zones')
  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <div className="flex items-center gap-4">
          <h2 className="text-xl font-semibold">DNS Policy</h2>
          <div className="flex bg-[var(--zf-surface)] rounded-lg p-0.5">
            {(['zones', 'policies'] as const).map(v => (
              <button key={v} onClick={() => setView(v)} className={`px-3 py-1 rounded text-sm transition ${view === v ? 'bg-[var(--zf-canvas-alt)] text-[var(--zf-ink)]' : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'}`}>
                {v.charAt(0).toUpperCase() + v.slice(1)}
              </button>
            ))}
          </div>
        </div>
        <div className="flex gap-2">
          {!readOnly && <button onClick={onSync} className="flex items-center gap-2 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
            <RefreshCw className="w-4 h-4" /> Sync
          </button>}
          {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] text-white py-2 px-4 rounded-lg transition text-sm">
            <Plus className="w-4 h-4" /> Add {view === 'zones' ? 'Zone' : 'Policy'}
          </button>}
        </div>
      </div>

      {view === 'zones' && (
        zones.length === 0 ? (
          <div className="p-12 text-center text-[var(--zf-muted)]">No DNS zones configured.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-[var(--zf-surface)]">
                <tr>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Domain</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Records</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Status</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {zones.map(z => (
                  <tr key={z.id} className="hover:bg-white/[0.03] transition">
                    <td className="p-4 font-medium">
                      {z.name}
                      {isHostManaged(z) && <HostBadge />}
                      {z.description && <div className="text-xs text-[var(--zf-muted)] font-normal mt-0.5">{z.description}</div>}
                    </td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-link)]">{z.domain ?? z.name}</td>
                    <td className="p-4 font-medium text-cyan-400">{z.records?.length ?? '—'}</td>
                    <td className="p-4">
                      <StatusBadge status={z.enabled === false ? 'disabled' : 'active'} color={z.enabled === false ? 'gray' : 'green'} />
                    </td>
                    <td className="p-4">
                      <div className="flex items-center gap-1">
                        {!readOnly && !isHostManaged(z) && onEditZone && (
                          <button onClick={() => onEditZone(z.id)} className="p-2 hover:bg-[var(--zf-hairline)] rounded transition" title="Edit">
                            <Pencil className="w-4 h-4" />
                          </button>
                        )}
                        <HostManagedActions readOnly={readOnly}
                          item={{ id: z.id, managed: z.managed }}
                          onDelete={() => onDeleteZone(z.id)}
                          onAdopt={onAdoptZone ? () => onAdoptZone(z.id) : undefined}
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

      {view === 'policies' && (
        policies.length === 0 ? (
          <div className="p-12 text-center text-[var(--zf-muted)]">No DNS policies configured.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-[var(--zf-surface)]">
                <tr>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Zone / Template</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Labels</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Type</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Status</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {policies.map(p => (
                  <tr key={p.id} className="hover:bg-white/[0.03] transition">
                    <td className="p-4 font-medium">
                      {p.name}
                      {isHostManaged(p) && <HostBadge />}
                      {p.description && <div className="text-xs text-[var(--zf-muted)] font-normal mt-0.5">{p.description}</div>}
                    </td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">
                      {p.record_template ?? p.upstream_servers?.join(', ') ?? '—'}
                    </td>
                    <td className="p-4"><LabelTags labels={p.selector?.match_labels ?? p.labels} /></td>
                    <td className="p-4 text-[var(--zf-muted)]">{p.record_type ?? '—'}</td>
                    <td className="p-4">
                      <StatusBadge status={p.enabled ? 'active' : 'disabled'} color={p.enabled ? 'green' : 'gray'} />
                    </td>
                    <td className="p-4">
                      <div className="flex items-center gap-1">
                        {!readOnly && !isHostManaged(p) && onEditPolicy && (
                          <button onClick={() => onEditPolicy(p.id)} className="p-2 hover:bg-[var(--zf-hairline)] rounded transition" title="Edit">
                            <Pencil className="w-4 h-4" />
                          </button>
                        )}
                        <HostManagedActions readOnly={readOnly}
                          item={{ id: p.id, managed: p.managed }}
                          onDelete={() => onDeletePolicy(p.id)}
                          onAdopt={onAdoptPolicy ? () => onAdoptPolicy(p.id) : undefined}
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
    </div>
  )
}

export function CreateDnsZoneModal({ onClose, onCreated }: { onClose: () => void; onCreated: (z: DnsZone) => void }) {
  const [name, setName] = useState('')
  const [domain, setDomain] = useState('')
  const [description, setDescription] = useState('')
  const [records, setRecords] = useState<{ name: string; record_type: DnsRecordType; value: string; ttl: number }[]>([])
  const [recName, setRecName] = useState('')
  const [recType, setRecType] = useState<DnsRecordType>('A')
  const [recValue, setRecValue] = useState('')
  const [recTtl, setRecTtl] = useState('300')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const addRecord = () => {
    if (!recName.trim() || !recValue.trim()) return
    setRecords(prev => [...prev, { name: recName.trim(), record_type: recType, value: recValue.trim(), ttl: parseInt(recTtl) || 300 }])
    setRecName('')
    setRecValue('')
  }

  const handleSubmit = async () => {
    if (!name.trim() || !domain.trim()) { setErr('Name and domain are required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateDnsZoneRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        domain: domain.trim(),
        records: records.length > 0 ? records : undefined,
      }
      const z = await api.createDnsZone(req)
      onCreated(z)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create DNS Zone" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="internal-zone" />
        <InputField label="Domain" value={domain} onChange={setDomain} placeholder="internal.local" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="Internal DNS zone" />
        <div className="border border-[var(--zf-hairline)] rounded-lg p-4 space-y-3">
          <div className="text-sm font-medium text-[var(--zf-ink)]">Add Record</div>
          <div className="grid grid-cols-2 gap-2">
            <InputField label="Record Name" value={recName} onChange={setRecName} placeholder="web" />
            <div>
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Type</label>
              <select aria-label="Type" value={recType} onChange={e => setRecType(e.target.value as DnsRecordType)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
                {(['A', 'AAAA', 'CNAME', 'MX', 'TXT', 'SRV', 'PTR'] as DnsRecordType[]).map(t => (
                  <option key={t} value={t}>{t}</option>
                ))}
              </select>
            </div>
          </div>
          <div className="grid grid-cols-2 gap-2">
            <InputField label="Value" value={recValue} onChange={setRecValue} placeholder="10.0.0.1" />
            <InputField label="TTL" value={recTtl} onChange={setRecTtl} placeholder="300" type="number" />
          </div>
          <button type="button" onClick={addRecord} className="flex items-center gap-1 text-sm text-[var(--zf-link)] hover:text-blue-300 transition">
            <Plus className="w-3.5 h-3.5" /> Add Record
          </button>
          {records.length > 0 && (
            <div className="space-y-1 mt-2">
              {records.map((r, i) => (
                <div key={i} className="flex items-center gap-2 text-xs bg-[var(--zf-surface)] rounded px-2 py-1">
                  <StatusBadge status={r.record_type} color="blue" />
                  <span className="text-[var(--zf-ink)]">{r.name}</span>
                  <span className="text-[var(--zf-muted)]">{r.value}</span>
                  <span className="text-[var(--zf-muted)]">TTL:{r.ttl}</span>
                  <button onClick={() => setRecords(prev => prev.filter((_, j) => j !== i))} className="ml-auto text-red-600 hover:text-red-300" aria-label="Delete" title="Delete">
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create Zone'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export function CreateDnsPolicyModal({ zones, onClose, onCreated }: { zones: DnsZone[]; onClose: () => void; onCreated: (p: DnsPolicy) => void }) {
  const [name, setName] = useState('')
  const [zoneId, setZoneId] = useState(zones[0]?.id ?? '')
  const [labels, setLabels] = useState<Record<string, string>>({})
  const [recordTemplate, setRecordTemplate] = useState('{name}.zyvor-fabricd.local')
  const [recordType, setRecordType] = useState<DnsRecordType>('A')
  const [enabled, setEnabled] = useState(true)
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    if (!zoneId) { setErr('Select a DNS zone first'); return }
    if (!recordTemplate.trim()) { setErr('Record template is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateDnsPolicyRequest = {
        name: name.trim(),
        zone_id: zoneId,
        selector: Object.keys(labels).length > 0 ? { match_labels: labels } : undefined,
        record_template: recordTemplate.trim(),
        record_type: recordType,
        enabled,
      }
      const p = await api.createDnsPolicy(req)
      onCreated(p)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create DNS Policy" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="vm-a-records" />
        <div>
          <label className="block text-xs text-[var(--zf-muted)] mb-1">Zone</label>
          <select aria-label="Zone" value={zoneId} onChange={e => setZoneId(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
            {zones.length === 0 ? <option value="">No zones — create a zone first</option> : zones.map(z => (
              <option key={z.id} value={z.id}>{z.name}</option>
            ))}
          </select>
        </div>
        <LabelSelectorInput labels={labels} onChange={setLabels} />
        <InputField label="Record Template" value={recordTemplate} onChange={setRecordTemplate} placeholder="{name}.zyvor-fabricd.local" />
        <div>
          <label className="block text-xs text-[var(--zf-muted)] mb-1">Record Type</label>
          <select aria-label="Record Type" value={recordType} onChange={e => setRecordType(e.target.value as DnsRecordType)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
            {(['A', 'CNAME', 'SRV'] as DnsRecordType[]).map(t => (
              <option key={t} value={t}>{t}</option>
            ))}
          </select>
        </div>
        <label className="flex items-center gap-2 text-sm text-[var(--zf-ink)]">
          <input type="checkbox" checked={enabled} onChange={e => setEnabled(e.target.checked)} className="rounded border-[var(--zf-hairline)]" />
          Enabled
        </label>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting || !zoneId} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create DNS Policy'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export function EditDnsZoneModal({ id, onClose, onUpdated }: { id: string; onClose: () => void; onUpdated: (z: DnsZone) => void }) {
  const [loading, setLoading] = useState(true)
  const [loadErr, setLoadErr] = useState('')
  const [name, setName] = useState('')
  const [domain, setDomain] = useState('')
  const [description, setDescription] = useState('')
  const [records, setRecords] = useState<{ name: string; record_type: DnsRecordType; value: string; ttl: number }[]>([])
  const [recName, setRecName] = useState('')
  const [recType, setRecType] = useState<DnsRecordType>('A')
  const [recValue, setRecValue] = useState('')
  const [recTtl, setRecTtl] = useState('300')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  useEffect(() => {
    let cancelled = false
    api.getDnsZone(id).then(z => {
      if (cancelled) return
      setName(z.name)
      setDomain(z.domain ?? z.name)
      setDescription(z.description ?? '')
      setRecords(z.records ?? [])
      setLoading(false)
    }).catch((e: unknown) => {
      if (cancelled) return
      setLoadErr(extractErrorMessage(e))
      setLoading(false)
    })
    return () => { cancelled = true }
  }, [id])

  const addRecord = () => {
    if (!recName.trim() || !recValue.trim()) return
    setRecords(prev => [...prev, { name: recName.trim(), record_type: recType, value: recValue.trim(), ttl: parseInt(recTtl) || 300 }])
    setRecName('')
    setRecValue('')
  }

  const handleSubmit = async () => {
    if (!name.trim() || !domain.trim()) { setErr('Name and domain are required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateDnsZoneRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        domain: domain.trim(),
        records: records.length > 0 ? records : undefined,
      }
      const z = await api.updateDnsZone(id, req)
      onUpdated(z)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  if (loading) {
    return (
      <ModalWrapper title="Edit DNS Zone" onClose={onClose}>
        <PanelLoading label="Loading..." />
      </ModalWrapper>
    )
  }
  if (loadErr) {
    return (
      <ModalWrapper title="Edit DNS Zone" onClose={onClose}>
        <p className="text-red-600 text-sm">{loadErr}</p>
      </ModalWrapper>
    )
  }

  return (
    <ModalWrapper title="Edit DNS Zone" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="internal-zone" />
        <InputField label="Domain" value={domain} onChange={setDomain} placeholder="internal.local" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="Internal DNS zone" />
        <div className="border border-[var(--zf-hairline)] rounded-lg p-4 space-y-3">
          <div className="text-sm font-medium text-[var(--zf-ink)]">Add Record</div>
          <div className="grid grid-cols-2 gap-2">
            <InputField label="Record Name" value={recName} onChange={setRecName} placeholder="web" />
            <div>
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Type</label>
              <select aria-label="Type" value={recType} onChange={e => setRecType(e.target.value as DnsRecordType)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
                {(['A', 'AAAA', 'CNAME', 'MX', 'TXT', 'SRV', 'PTR'] as DnsRecordType[]).map(t => (
                  <option key={t} value={t}>{t}</option>
                ))}
              </select>
            </div>
          </div>
          <div className="grid grid-cols-2 gap-2">
            <InputField label="Value" value={recValue} onChange={setRecValue} placeholder="10.0.0.1" />
            <InputField label="TTL" value={recTtl} onChange={setRecTtl} placeholder="300" type="number" />
          </div>
          <button type="button" onClick={addRecord} className="flex items-center gap-1 text-sm text-[var(--zf-link)] hover:text-blue-300 transition">
            <Plus className="w-3.5 h-3.5" /> Add Record
          </button>
          {records.length > 0 && (
            <div className="space-y-1 mt-2">
              {records.map((r, i) => (
                <div key={i} className="flex items-center gap-2 text-xs bg-[var(--zf-surface)] rounded px-2 py-1">
                  <StatusBadge status={r.record_type} color="blue" />
                  <span className="text-[var(--zf-ink)]">{r.name}</span>
                  <span className="text-[var(--zf-muted)]">{r.value}</span>
                  <span className="text-[var(--zf-muted)]">TTL:{r.ttl}</span>
                  <button onClick={() => setRecords(prev => prev.filter((_, j) => j !== i))} className="ml-auto text-red-600 hover:text-red-300" aria-label="Delete" title="Delete">
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

export function EditDnsPolicyModal({ id, zones, onClose, onUpdated }: { id: string; zones: DnsZone[]; onClose: () => void; onUpdated: (p: DnsPolicy) => void }) {
  const [loading, setLoading] = useState(true)
  const [loadErr, setLoadErr] = useState('')
  const [name, setName] = useState('')
  const [zoneId, setZoneId] = useState('')
  const [labels, setLabels] = useState<Record<string, string>>({})
  const [recordTemplate, setRecordTemplate] = useState('')
  const [recordType, setRecordType] = useState<DnsRecordType>('A')
  const [enabled, setEnabled] = useState(true)
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  useEffect(() => {
    let cancelled = false
    api.getDnsPolicy(id).then(p => {
      if (cancelled) return
      setName(p.name)
      setZoneId(p.zone_id ?? zones[0]?.id ?? '')
      setLabels(p.selector?.match_labels ?? p.labels ?? {})
      setRecordTemplate(p.record_template ?? '')
      setRecordType((p.record_type as DnsRecordType) ?? 'A')
      setEnabled(p.enabled)
      setLoading(false)
    }).catch((e: unknown) => {
      if (cancelled) return
      setLoadErr(extractErrorMessage(e))
      setLoading(false)
    })
    return () => { cancelled = true }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id])

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    if (!zoneId) { setErr('Select a DNS zone first'); return }
    if (!recordTemplate.trim()) { setErr('Record template is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateDnsPolicyRequest = {
        name: name.trim(),
        zone_id: zoneId,
        selector: Object.keys(labels).length > 0 ? { match_labels: labels } : undefined,
        record_template: recordTemplate.trim(),
        record_type: recordType,
        enabled,
      }
      const p = await api.updateDnsPolicy(id, req)
      onUpdated(p)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  if (loading) {
    return (
      <ModalWrapper title="Edit DNS Policy" onClose={onClose}>
        <PanelLoading label="Loading..." />
      </ModalWrapper>
    )
  }
  if (loadErr) {
    return (
      <ModalWrapper title="Edit DNS Policy" onClose={onClose}>
        <p className="text-red-600 text-sm">{loadErr}</p>
      </ModalWrapper>
    )
  }

  return (
    <ModalWrapper title="Edit DNS Policy" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="vm-a-records" />
        <div>
          <label className="block text-xs text-[var(--zf-muted)] mb-1">Zone</label>
          <select aria-label="Zone" value={zoneId} onChange={e => setZoneId(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
            {zones.length === 0 ? <option value="">No zones — create a zone first</option> : zones.map(z => (
              <option key={z.id} value={z.id}>{z.name}</option>
            ))}
          </select>
        </div>
        <LabelSelectorInput labels={labels} onChange={setLabels} />
        <InputField label="Record Template" value={recordTemplate} onChange={setRecordTemplate} placeholder="{name}.zyvor-fabricd.local" />
        <div>
          <label className="block text-xs text-[var(--zf-muted)] mb-1">Record Type</label>
          <select aria-label="Record Type" value={recordType} onChange={e => setRecordType(e.target.value as DnsRecordType)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
            {(['A', 'CNAME', 'SRV'] as DnsRecordType[]).map(t => (
              <option key={t} value={t}>{t}</option>
            ))}
          </select>
        </div>
        <label className="flex items-center gap-2 text-sm text-[var(--zf-ink)]">
          <input type="checkbox" checked={enabled} onChange={e => setEnabled(e.target.checked)} className="rounded border-[var(--zf-hairline)]" />
          Enabled
        </label>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting || !zoneId} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Saving...' : 'Save Changes'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default DnsTabContent
