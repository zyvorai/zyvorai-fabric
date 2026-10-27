// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useState } from 'react'
import { Plus, Pencil } from 'lucide-react'
import * as api from '../../api/networkd'
import type { BondConfig, CreateBondRequest, BondMode } from '../../api/networkd'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from './ModalShared'
import { ListControls, DEFAULT_PAGE_SIZE, paginateSlice } from './ListControls'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

interface BondsTabProps {
  bonds: BondConfig[]
  onDelete: (id: string) => void
  onAdopt: (id: string) => void
  onCreate: () => void
  onEdit: (b: BondConfig) => void
}

function BondsTabContent({ bonds, onDelete, onAdopt, onCreate, onEdit }: BondsTabProps) {
  const readOnly = useReadOnly()
  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)
  const [showAll, setShowAll] = useState(false)

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    const list = [...bonds].sort((a, b) => a.name.localeCompare(b.name))
    if (!q) return list
    return list.filter(b => {
      const hay = [b.name, b.mode, b.slave_interfaces.join(' '), b.addresses.join(' ')].join(' ').toLowerCase()
      return hay.includes(q)
    })
  }, [bonds, search])

  const pageItems = paginateSlice(filtered, page, DEFAULT_PAGE_SIZE, showAll)

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <h2 className="text-xl font-semibold">Bonds</h2>
        {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-cyan-600 hover:bg-cyan-700 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
          <Plus className="w-4 h-4" /> Create Bond
        </button>}
      </div>
      {bonds.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No bonds configured.</div>
      ) : (
        <>
          <ListControls
            search={search}
            onSearchChange={setSearch}
            searchPlaceholder="Search name, mode, slaves…"
            total={bonds.length}
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
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Mode</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Slaves</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Addresses</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">DHCP</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {pageItems.map(b => (
                <tr key={b.id} className="hover:bg-white/[0.03] transition">
                  <td className="p-4 font-medium">{b.name}{isHostManaged(b) && <HostBadge />}</td>
                  <td className="p-4">
                    <span className="px-2 py-1 rounded text-xs font-medium bg-cyan-500/10 text-cyan-400 border border-cyan-500/20">{b.mode}</span>
                  </td>
                  <td className="p-4 text-[var(--zf-muted)] font-mono text-sm">{b.slave_interfaces.join(', ') || '-'}</td>
                  <td className="p-4 text-[var(--zf-muted)] font-mono text-sm">{b.addresses.join(', ') || '-'}</td>
                  <td className="p-4 text-[var(--zf-muted)]">{b.dhcp}</td>
                  <td className="p-4">
                    <div className="flex items-center gap-1">
                      {!readOnly && !isHostManaged(b) && (
                        <button onClick={() => onEdit(b)} className="p-2 hover:bg-white/[0.06] rounded transition" title="Edit bond" type="button">
                          <Pencil className="w-4 h-4" />
                        </button>
                      )}
                      <HostManagedActions readOnly={readOnly} item={b} onDelete={() => onDelete(b.id)} onAdopt={() => onAdopt(b.id)} />
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {filtered.length === 0 && (
            <div className="p-8 text-center text-[var(--zf-muted)] text-sm">No bonds match your search.</div>
          )}
          </div>
        </>
      )}
    </div>
  )
}

export function CreateBondModal({ onClose, onCreated }: { onClose: () => void; onCreated: (b: BondConfig) => void }) {
  const [name, setName] = useState('')
  const [mode, setMode] = useState<BondMode>('802.3ad')
  const [slaves, setSlaves] = useState('')
  const [miiMonitor, setMiiMonitor] = useState('100')
  const [addresses, setAddresses] = useState('')
  const [gateway, setGateway] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateBondRequest = {
        name: name.trim(),
        mode,
        mii_monitor_sec: miiMonitor ? parseInt(miiMonitor) : undefined,
        slave_interfaces: slaves ? slaves.split(',').map(s => s.trim()).filter(Boolean) : [],
        addresses: addresses ? addresses.split(',').map(s => s.trim()).filter(Boolean) : [],
        gateway: gateway.trim() || undefined,
      }
      const bond = await api.createBond(req)
      onCreated(bond)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create Bond" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="bond0" />
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">Mode</label>
          <select value={mode} onChange={e => setMode(e.target.value as BondMode)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500">
            <option value="802.3ad">802.3ad (LACP)</option>
            <option value="active-backup">active-backup</option>
            <option value="balance-rr">balance-rr</option>
            <option value="balance-xor">balance-xor</option>
            <option value="broadcast">broadcast</option>
            <option value="balance-tlb">balance-tlb</option>
            <option value="balance-alb">balance-alb</option>
          </select>
        </div>
        <InputField label="Slave Interfaces (comma-separated)" value={slaves} onChange={setSlaves} placeholder="eth0, eth1" />
        <InputField label="MII Monitor (ms)" value={miiMonitor} onChange={setMiiMonitor} placeholder="100" type="number" />
        <InputField label="Addresses (comma-separated)" value={addresses} onChange={setAddresses} placeholder="10.0.0.1/24" />
        <InputField label="Gateway" value={gateway} onChange={setGateway} placeholder="10.0.0.254" />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-cyan-600 hover:bg-cyan-700 disabled:opacity-50 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create Bond'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export function EditBondModal({ bond, onClose, onUpdated }: { bond: BondConfig; onClose: () => void; onUpdated: (b: BondConfig) => void }) {
  const [name, setName] = useState(bond.name)
  const [mode, setMode] = useState<BondMode>(bond.mode)
  const [slaves, setSlaves] = useState(bond.slave_interfaces.join(', '))
  const [miiMonitor, setMiiMonitor] = useState(bond.mii_monitor_sec != null ? String(bond.mii_monitor_sec) : '')
  const [addresses, setAddresses] = useState(bond.addresses.join(', '))
  const [gateway, setGateway] = useState(bond.gateway ?? '')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateBondRequest = {
        name: name.trim(),
        mode,
        mii_monitor_sec: miiMonitor ? parseInt(miiMonitor) : undefined,
        slave_interfaces: slaves ? slaves.split(',').map(s => s.trim()).filter(Boolean) : [],
        addresses: addresses ? addresses.split(',').map(s => s.trim()).filter(Boolean) : [],
        gateway: gateway.trim() || undefined,
      }
      const updated = await api.updateBond(bond.id, req)
      onUpdated(updated)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Edit Bond" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="bond0" />
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">Mode</label>
          <select value={mode} onChange={e => setMode(e.target.value as BondMode)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500">
            <option value="802.3ad">802.3ad (LACP)</option>
            <option value="active-backup">active-backup</option>
            <option value="balance-rr">balance-rr</option>
            <option value="balance-xor">balance-xor</option>
            <option value="broadcast">broadcast</option>
            <option value="balance-tlb">balance-tlb</option>
            <option value="balance-alb">balance-alb</option>
          </select>
        </div>
        <InputField label="Slave Interfaces (comma-separated)" value={slaves} onChange={setSlaves} placeholder="eth0, eth1" />
        <InputField label="MII Monitor (ms)" value={miiMonitor} onChange={setMiiMonitor} placeholder="100" type="number" />
        <InputField label="Addresses (comma-separated)" value={addresses} onChange={setAddresses} placeholder="10.0.0.1/24" />
        <InputField label="Gateway" value={gateway} onChange={setGateway} placeholder="10.0.0.254" />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-cyan-600 hover:bg-cyan-700 disabled:opacity-50 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition">
          {submitting ? 'Saving...' : 'Save Changes'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default BondsTabContent
