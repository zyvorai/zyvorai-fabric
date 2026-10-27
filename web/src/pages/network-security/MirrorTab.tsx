// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect } from 'react'
import { Plus, RefreshCw, Pencil } from 'lucide-react'
import * as api from '../../api/network-security'
import type { MirrorSession, CreateMirrorSessionRequest, MirrorDirection } from '../../api/network-security'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from '../network/ModalShared'
import { StatusBadge, LabelSelectorInput, LabelTags } from './ModalShared'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

interface MirrorTabProps {
  sessions: MirrorSession[]
  onDelete: (id: string) => void
  onAdopt?: (id: string) => void
  onEdit?: (id: string) => void
  onCreate: () => void
  onSync: () => void
}

function MirrorTabContent({ sessions, onDelete, onAdopt, onEdit, onCreate, onSync }: MirrorTabProps) {
  const readOnly = useReadOnly()
  const [status, setStatus] = useState<{ active_sessions: number; mirrored_vms: number } | null>(null)

  const refreshStatus = () => { api.mirrorStatus().then(setStatus).catch(() => {}) }
  useEffect(() => { refreshStatus() }, [])
  const handleSyncClick = async () => { await onSync(); refreshStatus() }

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <h2 className="text-xl font-semibold">Packet Mirror</h2>
        <div className="flex items-center gap-2">
          {status && (
            <span className="text-xs text-[var(--zf-muted)] bg-[var(--zf-surface)] rounded-lg px-3 py-1.5 border border-[var(--zf-hairline)]">
              {status.active_sessions} active &middot; {status.mirrored_vms} mirrored VMs
            </span>
          )}
          {!readOnly && <button onClick={handleSyncClick} className="flex items-center gap-2 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
            <RefreshCw className="w-4 h-4" /> Sync
          </button>}
          {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] text-white py-2 px-4 rounded-lg transition text-sm">
            <Plus className="w-4 h-4" /> Add Session
          </button>}
        </div>
      </div>
      {sessions.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No mirror sessions configured. Create one to capture and mirror VM network traffic.</div>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full">
            <thead className="bg-[var(--zf-surface)]">
              <tr>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Source Selector</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Direction</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Collector</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Filter</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Status</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {sessions.map(s => (
                <tr key={s.id} className="hover:bg-white/[0.03] transition">
                    <td className="p-4">
                      <div className="font-medium">{s.name}{isHostManaged(s) && <HostBadge />}</div>
                      {s.description && <div className="text-xs text-[var(--zf-muted)] mt-1">{s.description}</div>}
                    </td>
                    <td className="p-4 text-sm">
                      <LabelTags labels={s.selector?.match_labels} />
                    </td>
                  <td className="p-4">
                    <StatusBadge
                      status={s.direction}
                      color={s.direction === 'both' ? 'blue' : s.direction === 'ingress' ? 'green' : 'yellow'}
                    />
                  </td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">
                      {s.collector_target || '—'}
                    </td>
                  <td className="p-4 text-sm text-[var(--zf-muted)]">
                    {s.filter?.protocol || s.filter?.dst_port || s.filter?.src_cidr || s.filter?.dst_cidr ? (
                      <span>{[s.filter.protocol, s.filter.dst_port && `:${s.filter.dst_port}`, s.filter.src_cidr, s.filter.dst_cidr].filter(Boolean).join(' ')}</span>
                    ) : (
                      <span className="text-[var(--zf-muted)]">all</span>
                    )}
                  </td>
                  <td className="p-4">
                    <StatusBadge status={s.enabled ? 'active' : 'disabled'} color={s.enabled ? 'green' : 'gray'} />
                  </td>
                    <td className="p-4">
                      <div className="flex items-center gap-1">
                        {!readOnly && !isHostManaged(s) && onEdit && (
                          <button onClick={() => onEdit(s.id)} className="p-2 hover:bg-[var(--zf-hairline)] rounded transition" title="Edit">
                            <Pencil className="w-4 h-4" />
                          </button>
                        )}
                        <HostManagedActions readOnly={readOnly}
                          item={{ id: s.id, managed: s.managed }}
                          onDelete={() => onDelete(s.id)}
                          onAdopt={onAdopt ? () => onAdopt(s.id) : undefined}
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

export function CreateMirrorModal({ onClose, onCreated }: { onClose: () => void; onCreated: (s: MirrorSession) => void }) {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [labels, setLabels] = useState<Record<string, string>>({})
  const [direction, setDirection] = useState<MirrorDirection>('both')
  const [collectorAddress, setCollectorAddress] = useState('')
  const [collectorPort, setCollectorPort] = useState('4789')
  const [filterProtocol, setFilterProtocol] = useState('')
  const [filterPort, setFilterPort] = useState('')
  const [filterCidr, setFilterCidr] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim() || Object.keys(labels).length === 0 || !collectorAddress.trim()) {
      setErr('Name, at least one source label, and collector address are required')
      return
    }
    setSubmitting(true)
    setErr('')
    try {
      const hasFilter = filterProtocol.trim() || filterPort.trim() || filterCidr.trim()
      const req: CreateMirrorSessionRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        selector: { match_labels: labels },
        direction,
        collector_target: `${collectorAddress.trim()}:${parseInt(collectorPort) || 4789}`,
        filter: hasFilter ? {
          protocol: filterProtocol.trim() || undefined,
          dst_port: filterPort ? parseInt(filterPort) : undefined,
          dst_cidr: filterCidr.trim() || undefined,
        } : undefined,
      }
      const s = await api.createMirrorSession(req)
      onCreated(s)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create Mirror Session" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="debug-capture" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="Debug traffic capture" />
        <LabelSelectorInput labels={labels} onChange={setLabels} />
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">Direction</label>
          <select value={direction} onChange={e => setDirection(e.target.value as MirrorDirection)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500">
            <option value="both">Both</option>
            <option value="ingress">Ingress</option>
            <option value="egress">Egress</option>
          </select>
        </div>
        <div className="grid grid-cols-2 gap-2">
          <InputField label="Collector Address" value={collectorAddress} onChange={setCollectorAddress} placeholder="10.0.0.50" />
          <InputField label="Collector Port" value={collectorPort} onChange={setCollectorPort} placeholder="4789" type="number" />
        </div>
        <div className="border border-[var(--zf-hairline)] rounded-lg p-4 space-y-3">
          <div className="text-sm font-medium text-[var(--zf-ink)]">Filters (optional)</div>
          <div className="grid grid-cols-3 gap-2">
            <InputField label="Protocol" value={filterProtocol} onChange={setFilterProtocol} placeholder="tcp" />
            <InputField label="Port" value={filterPort} onChange={setFilterPort} placeholder="80" type="number" />
            <InputField label="CIDR" value={filterCidr} onChange={setFilterCidr} placeholder="10.0.0.0/8" />
          </div>
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create Session'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export function EditMirrorModal({ id, onClose, onUpdated }: { id: string; onClose: () => void; onUpdated: (s: MirrorSession) => void }) {
  const [loading, setLoading] = useState(true)
  const [loadErr, setLoadErr] = useState('')
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [labels, setLabels] = useState<Record<string, string>>({})
  const [direction, setDirection] = useState<MirrorDirection>('both')
  const [collectorAddress, setCollectorAddress] = useState('')
  const [collectorPort, setCollectorPort] = useState('4789')
  const [filterProtocol, setFilterProtocol] = useState('')
  const [filterPort, setFilterPort] = useState('')
  const [filterCidr, setFilterCidr] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  useEffect(() => {
    let cancelled = false
    api.getMirrorSession(id).then(s => {
      if (cancelled) return
      setName(s.name)
      setDescription(s.description ?? '')
      setLabels(s.selector?.match_labels ?? {})
      setDirection(s.direction)
      const [addr, port] = (s.collector_target ?? '').split(':')
      setCollectorAddress(addr ?? '')
      setCollectorPort(port || '4789')
      setFilterProtocol(s.filter?.protocol ?? '')
      setFilterPort(s.filter?.dst_port ? String(s.filter.dst_port) : '')
      setFilterCidr(s.filter?.dst_cidr ?? s.filter?.src_cidr ?? '')
      setLoading(false)
    }).catch((e: unknown) => {
      if (cancelled) return
      setLoadErr(extractErrorMessage(e))
      setLoading(false)
    })
    return () => { cancelled = true }
  }, [id])

  const handleSubmit = async () => {
    if (!name.trim() || Object.keys(labels).length === 0 || !collectorAddress.trim()) {
      setErr('Name, at least one source label, and collector address are required')
      return
    }
    setSubmitting(true)
    setErr('')
    try {
      const hasFilter = filterProtocol.trim() || filterPort.trim() || filterCidr.trim()
      const req: CreateMirrorSessionRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        selector: { match_labels: labels },
        direction,
        collector_target: `${collectorAddress.trim()}:${parseInt(collectorPort) || 4789}`,
        filter: hasFilter ? {
          protocol: filterProtocol.trim() || undefined,
          dst_port: filterPort ? parseInt(filterPort) : undefined,
          dst_cidr: filterCidr.trim() || undefined,
        } : undefined,
      }
      const s = await api.updateMirrorSession(id, req)
      onUpdated(s)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  if (loading) {
    return (
      <ModalWrapper title="Edit Mirror Session" onClose={onClose}>
        <div className="text-[var(--zf-muted)] text-sm">Loading...</div>
      </ModalWrapper>
    )
  }
  if (loadErr) {
    return (
      <ModalWrapper title="Edit Mirror Session" onClose={onClose}>
        <p className="text-red-600 text-sm">{loadErr}</p>
      </ModalWrapper>
    )
  }

  return (
    <ModalWrapper title="Edit Mirror Session" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="debug-capture" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="Debug traffic capture" />
        <LabelSelectorInput labels={labels} onChange={setLabels} />
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">Direction</label>
          <select value={direction} onChange={e => setDirection(e.target.value as MirrorDirection)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500">
            <option value="both">Both</option>
            <option value="ingress">Ingress</option>
            <option value="egress">Egress</option>
          </select>
        </div>
        <div className="grid grid-cols-2 gap-2">
          <InputField label="Collector Address" value={collectorAddress} onChange={setCollectorAddress} placeholder="10.0.0.50" />
          <InputField label="Collector Port" value={collectorPort} onChange={setCollectorPort} placeholder="4789" type="number" />
        </div>
        <div className="border border-[var(--zf-hairline)] rounded-lg p-4 space-y-3">
          <div className="text-sm font-medium text-[var(--zf-ink)]">Filters (optional)</div>
          <div className="grid grid-cols-3 gap-2">
            <InputField label="Protocol" value={filterProtocol} onChange={setFilterProtocol} placeholder="tcp" />
            <InputField label="Port" value={filterPort} onChange={setFilterPort} placeholder="80" type="number" />
            <InputField label="CIDR" value={filterCidr} onChange={setFilterCidr} placeholder="10.0.0.0/8" />
          </div>
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Saving...' : 'Save Changes'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default MirrorTabContent
