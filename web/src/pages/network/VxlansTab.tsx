// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useState } from 'react'
import { Plus } from 'lucide-react'
import * as api from '../../api/networkd'
import type { VxlanConfig, CreateVxlanRequest, DhcpMode } from '../../api/networkd'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from './ModalShared'
import { ListControls, DEFAULT_PAGE_SIZE, paginateSlice } from './ListControls'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

interface VxlansTabProps {
  vxlans: VxlanConfig[]
  onDelete: (id: string) => void
  onAdopt?: (id: string) => void
  onCreate: () => void
}

function VxlansTabContent({ vxlans, onDelete, onAdopt, onCreate }: VxlansTabProps) {
  const readOnly = useReadOnly()
  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)
  const [showAll, setShowAll] = useState(false)
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    const list = [...vxlans].sort((a, b) => a.name.localeCompare(b.name))
    if (!q) return list
    return list.filter(v => [v.name, String(v.vni), v.remote ?? '', v.parent_interface ?? '', v.addresses.join(' ')].join(' ').toLowerCase().includes(q))
  }, [vxlans, search])
  const pageItems = paginateSlice(filtered, page, DEFAULT_PAGE_SIZE, showAll)

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <h2 className="text-xl font-semibold">VXLAN</h2>
        {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-indigo-600 hover:bg-indigo-700 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
          <Plus className="w-4 h-4" /> Create VXLAN
        </button>}
      </div>
      {vxlans.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No VXLAN interfaces configured.</div>
      ) : (
        <>
          <ListControls search={search} onSearchChange={setSearch} searchPlaceholder="Search name, VNI, remote…" total={vxlans.length} filtered={filtered.length} page={page} pageSize={DEFAULT_PAGE_SIZE} onPageChange={setPage} showAll={showAll} onShowAllChange={setShowAll} />
          <div className="overflow-x-auto">
          <table className="w-full">
            <thead className="bg-[var(--zf-surface)]">
              <tr>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">VNI</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Remote</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Parent</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Addresses</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {pageItems.map(v => (
                <tr key={v.id} className="hover:bg-white/[0.03] transition">
                  <td className="p-4 font-medium">
                    {v.name}
                    {isHostManaged(v) && <HostBadge />}
                  </td>
                  <td className="p-4 font-mono text-indigo-400">{v.vni}</td>
                  <td className="p-4 text-[var(--zf-muted)] font-mono text-sm">{v.remote ?? '-'}</td>
                  <td className="p-4 text-[var(--zf-muted)]">{v.parent_interface ?? '-'}</td>
                  <td className="p-4 text-[var(--zf-muted)] font-mono text-sm">{v.addresses.join(', ') || '-'}</td>
                  <td className="p-4">
                    <HostManagedActions readOnly={readOnly}
                      item={v}
                      onDelete={() => onDelete(v.id)}
                      onAdopt={onAdopt ? () => onAdopt(v.id) : undefined}
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {filtered.length === 0 && <div className="p-8 text-center text-[var(--zf-muted)] text-sm">No VXLAN interfaces match your search.</div>}
          </div>
        </>
      )}
    </div>
  )
}

export function CreateVxlanModal({ onClose, onCreated }: { onClose: () => void; onCreated: (v: VxlanConfig) => void }) {
  const [name, setName] = useState('')
  const [vni, setVni] = useState('')
  const [remote, setRemote] = useState('')
  const [local, setLocal] = useState('')
  const [port, setPort] = useState('4789')
  const [parent, setParent] = useState('')
  const [addresses, setAddresses] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim() || !vni) { setErr('Name and VNI are required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateVxlanRequest = {
        name: name.trim(),
        vni: parseInt(vni, 10),
        remote: remote.trim() || undefined,
        local: local.trim() || undefined,
        port: port ? parseInt(port, 10) : undefined,
        parent_interface: parent.trim() || undefined,
        addresses: addresses ? addresses.split(',').map(s => s.trim()).filter(Boolean) : [],
        dhcp: 'no' as DhcpMode,
      }
      const vxlan = await api.createVxlan(req)
      onCreated(vxlan)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create VXLAN" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="vxlan0" />
        <InputField label="VNI" value={vni} onChange={setVni} placeholder="100" type="number" />
        <InputField label="Remote IP" value={remote} onChange={setRemote} placeholder="10.0.0.2" />
        <InputField label="Local IP (optional)" value={local} onChange={setLocal} placeholder="10.0.0.1" />
        <InputField label="UDP Port" value={port} onChange={setPort} placeholder="4789" type="number" />
        <InputField label="Parent Interface (optional)" value={parent} onChange={setParent} placeholder="eth0" />
        <InputField label="Addresses (comma-separated)" value={addresses} onChange={setAddresses} placeholder="10.10.0.1/24" />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-indigo-600 hover:bg-indigo-700 disabled:opacity-50 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create VXLAN'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default VxlansTabContent
