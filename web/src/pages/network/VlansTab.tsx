// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useState } from 'react'
import { Plus, Pencil } from 'lucide-react'
import * as api from '../../api/networkd'
import type { VlanConfig, CreateVlanRequest } from '../../api/networkd'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from './ModalShared'
import { ListControls, DEFAULT_PAGE_SIZE, paginateSlice } from './ListControls'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

interface VlansTabProps {
  vlans: VlanConfig[]
  onDelete: (id: string) => void
  onAdopt: (id: string) => void
  onCreate: () => void
  onEdit: (v: VlanConfig) => void
}

function VlansTabContent({ vlans, onDelete, onAdopt, onCreate, onEdit }: VlansTabProps) {
  const readOnly = useReadOnly()
  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)
  const [showAll, setShowAll] = useState(false)

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    const list = [...vlans].sort((a, b) => a.name.localeCompare(b.name))
    if (!q) return list
    return list.filter(v => {
      const hay = [v.name, String(v.vlan_id), v.parent_interface, v.addresses.join(' ')].join(' ').toLowerCase()
      return hay.includes(q)
    })
  }, [vlans, search])

  const pageItems = paginateSlice(filtered, page, DEFAULT_PAGE_SIZE, showAll)

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <h2 className="text-xl font-semibold">VLANs</h2>
        {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-purple-600 hover:bg-purple-700 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
          <Plus className="w-4 h-4" /> Create VLAN
        </button>}
      </div>
      {vlans.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No VLANs configured.</div>
      ) : (
        <>
          <ListControls
            search={search}
            onSearchChange={setSearch}
            searchPlaceholder="Search name, VLAN ID, parent…"
            total={vlans.length}
            filtered={filtered.length}
            page={page}
            pageSize={DEFAULT_PAGE_SIZE}
            onPageChange={setPage}
            showAll={showAll}
            onShowAllChange={setShowAll}
          />
          <div className="overflow-x-auto">
          <table className="w-full">
            <thead className="bg-[var(--zf-surface)]">
              <tr>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">VLAN ID</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Parent</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Addresses</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">DHCP</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {pageItems.map(v => (
                <tr key={v.id} className="hover:bg-white/[0.03] transition">
                  <td className="p-4 font-medium">{v.name}{isHostManaged(v) && <HostBadge />}</td>
                  <td className="p-4 font-mono text-purple-400">{v.vlan_id}</td>
                  <td className="p-4 text-[var(--zf-muted)]">{v.parent_interface}</td>
                  <td className="p-4 text-[var(--zf-muted)] font-mono text-sm">{v.addresses.join(', ') || '-'}</td>
                  <td className="p-4 text-[var(--zf-muted)]">{v.dhcp}</td>
                  <td className="p-4">
                    <div className="flex items-center gap-1">
                      {!readOnly && !isHostManaged(v) && (
                        <button onClick={() => onEdit(v)} className="p-2 hover:bg-white/[0.06] rounded transition" title="Edit VLAN" type="button">
                          <Pencil className="w-4 h-4" />
                        </button>
                      )}
                      <HostManagedActions readOnly={readOnly} item={v} onDelete={() => onDelete(v.id)} onAdopt={() => onAdopt(v.id)} />
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {filtered.length === 0 && (
            <div className="p-8 text-center text-[var(--zf-muted)] text-sm">No VLANs match your search.</div>
          )}
          </div>
        </>
      )}
    </div>
  )
}

export function CreateVlanModal({ onClose, onCreated }: { onClose: () => void; onCreated: (v: VlanConfig) => void }) {
  const [name, setName] = useState('')
  const [vlanId, setVlanId] = useState('')
  const [parent, setParent] = useState('')
  const [addresses, setAddresses] = useState('')
  const [gateway, setGateway] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim() || !vlanId || !parent.trim()) { setErr('Name, VLAN ID, and parent interface are required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateVlanRequest = {
        name: name.trim(),
        vlan_id: parseInt(vlanId),
        parent_interface: parent.trim(),
        addresses: addresses ? addresses.split(',').map(s => s.trim()).filter(Boolean) : [],
        gateway: gateway.trim() || undefined,
      }
      const vlan = await api.createVlan(req)
      onCreated(vlan)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create VLAN" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="vlan100" />
        <InputField label="VLAN ID" value={vlanId} onChange={setVlanId} placeholder="100" type="number" />
        <InputField label="Parent Interface" value={parent} onChange={setParent} placeholder="eth0" />
        <InputField label="Addresses (comma-separated)" value={addresses} onChange={setAddresses} placeholder="192.168.100.1/24" />
        <InputField label="Gateway" value={gateway} onChange={setGateway} placeholder="192.168.100.254" />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-purple-600 hover:bg-purple-700 disabled:opacity-50 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create VLAN'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export function EditVlanModal({ vlan, onClose, onUpdated }: { vlan: VlanConfig; onClose: () => void; onUpdated: (v: VlanConfig) => void }) {
  const [name, setName] = useState(vlan.name)
  const [vlanId, setVlanId] = useState(String(vlan.vlan_id))
  const [parent, setParent] = useState(vlan.parent_interface)
  const [addresses, setAddresses] = useState(vlan.addresses.join(', '))
  const [gateway, setGateway] = useState(vlan.gateway ?? '')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim() || !vlanId || !parent.trim()) { setErr('Name, VLAN ID, and parent interface are required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateVlanRequest = {
        name: name.trim(),
        vlan_id: parseInt(vlanId),
        parent_interface: parent.trim(),
        addresses: addresses ? addresses.split(',').map(s => s.trim()).filter(Boolean) : [],
        gateway: gateway.trim() || undefined,
      }
      const updated = await api.updateVlan(vlan.id, req)
      onUpdated(updated)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Edit VLAN" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="vlan100" />
        <InputField label="VLAN ID" value={vlanId} onChange={setVlanId} placeholder="100" type="number" />
        <InputField label="Parent Interface" value={parent} onChange={setParent} placeholder="eth0" />
        <InputField label="Addresses (comma-separated)" value={addresses} onChange={setAddresses} placeholder="192.168.100.1/24" />
        <InputField label="Gateway" value={gateway} onChange={setGateway} placeholder="192.168.100.254" />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-purple-600 hover:bg-purple-700 disabled:opacity-50 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition">
          {submitting ? 'Saving...' : 'Save Changes'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default VlansTabContent
