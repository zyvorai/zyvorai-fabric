// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useState } from 'react'
import { Plus, Eye } from 'lucide-react'
import * as api from '../../api/networkd'
import type { NetworkFileConfig, CreateNetworkFileRequest } from '../../api/networkd'
import { useReadOnly } from '../../contexts/ReadOnlyContext'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage, DetailModal } from './ModalShared'
import {
  ListControls,
  DEFAULT_PAGE_SIZE,
  netfileTypeOf,
  paginateSlice,
  type NetfileTypeFilter,
} from './ListControls'

interface NetfilesTabProps {
  netfiles: NetworkFileConfig[]
  onDelete: (id: string) => void
  onAdopt: (id: string) => void
  onCreate: () => void
}

export function countNetfileTypes(netfiles: NetworkFileConfig[]) {
  let physical = 0
  let container = 0
  for (const n of netfiles) {
    if (netfileTypeOf(n.description, n.match_name) === 'container') container++
    else physical++
  }
  return { physical, container, total: netfiles.length }
}

function NetfilesTabContent({ netfiles, onDelete, onAdopt, onCreate }: NetfilesTabProps) {
  const readOnly = useReadOnly()
  const [search, setSearch] = useState('')
  const [typeFilter, setTypeFilter] = useState<NetfileTypeFilter>('all')
  const [page, setPage] = useState(1)
  const [showAll, setShowAll] = useState(false)
  const [sortBy, setSortBy] = useState<'name' | 'state'>('name')
  const [viewingId, setViewingId] = useState<string | null>(null)
  const [viewData, setViewData] = useState<NetworkFileConfig | null>(null)
  const [viewLoading, setViewLoading] = useState(false)
  const [viewErr, setViewErr] = useState('')

  const handleView = async (id: string) => {
    setViewingId(id)
    setViewLoading(true)
    setViewErr('')
    setViewData(null)
    try {
      setViewData(await api.getNetworkFile(id))
    } catch (e: unknown) {
      setViewErr(extractErrorMessage(e))
    } finally {
      setViewLoading(false)
    }
  }

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    let list = netfiles.filter(n => {
      if (typeFilter !== 'all' && netfileTypeOf(n.description, n.match_name) !== typeFilter) {
        return false
      }
      if (!q) return true
      const hay = [
        n.match_name,
        n.description ?? '',
        n.addresses.join(' '),
        n.operational_state ?? '',
      ].join(' ').toLowerCase()
      return hay.includes(q)
    })
    list = [...list].sort((a, b) => {
      if (sortBy === 'state') {
        return (a.operational_state ?? '').localeCompare(b.operational_state ?? '')
      }
      return a.match_name.localeCompare(b.match_name)
    })
    return list
  }, [netfiles, search, typeFilter, sortBy])

  const pageItems = paginateSlice(filtered, page, DEFAULT_PAGE_SIZE, showAll)

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <h2 className="text-xl font-semibold">Interface Configuration (.network)</h2>
        <div className="flex items-center gap-2">
          <select
            value={sortBy}
            onChange={e => setSortBy(e.target.value as 'name' | 'state')}
            className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-2 py-1.5 text-xs text-[var(--zf-ink)]"
          >
            <option value="name">Sort by name</option>
            <option value="state">Sort by state</option>
          </select>
          {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-yellow-600 hover:bg-yellow-700 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
            <Plus className="w-4 h-4" /> Configure Interface
          </button>}
        </div>
      </div>
      {netfiles.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No interface configurations. Configure a physical interface to assign IPs, bridge membership, etc.</div>
      ) : (
        <>
          <ListControls
            search={search}
            onSearchChange={setSearch}
            searchPlaceholder="Search interface, address, description…"
            typeFilter={typeFilter}
            onTypeFilterChange={setTypeFilter}
            showTypeFilters
            total={netfiles.length}
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
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Interface</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">State</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Addresses</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">DHCP</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Bridge</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Bond</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">MTU</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {pageItems.map(n => (
                  <tr key={n.id} className="hover:bg-white/[0.03] transition">
                    <td className="p-4 font-medium">
                      <span className="flex items-center gap-2 flex-wrap">
                        {n.match_name}
                        {isHostManaged(n) && <HostBadge />}
                      </span>
                      {n.description && <span className="block text-xs text-[var(--zf-muted)] font-normal">{n.description}</span>}
                    </td>
                    <td className="p-4 text-[var(--zf-muted)] text-sm">{n.operational_state ?? '-'}</td>
                    <td className="p-4 text-[var(--zf-muted)] font-mono text-sm">{n.addresses.join(', ') || '-'}</td>
                    <td className="p-4 text-[var(--zf-muted)]">{n.dhcp}</td>
                    <td className="p-4 text-[var(--zf-muted)]">{n.bridge ?? '-'}</td>
                    <td className="p-4 text-[var(--zf-muted)]">{n.bond ?? '-'}</td>
                    <td className="p-4 text-[var(--zf-muted)]">{n.mtu ?? '-'}</td>
                    <td className="p-4">
                      <div className="flex items-center gap-1">
                        <button onClick={() => handleView(n.id)} className="p-2 hover:bg-white/[0.06] rounded transition" title="View details" type="button">
                          <Eye className="w-4 h-4" />
                        </button>
                        <HostManagedActions readOnly={readOnly} item={n} onDelete={() => onDelete(n.id)} onAdopt={() => onAdopt(n.id)} stateLabel={n.operational_state} />
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {filtered.length === 0 && (
              <div className="p-8 text-center text-[var(--zf-muted)] text-sm">No interfaces match your filters.</div>
            )}
          </div>
        </>
      )}
      {viewingId && (
        <DetailModal
          title="Interface Details"
          data={viewData as unknown as Record<string, unknown> | null}
          loading={viewLoading}
          error={viewErr}
          onClose={() => setViewingId(null)}
        />
      )}
    </div>
  )
}

export function CreateNetfileModal({ onClose, onCreated }: { onClose: () => void; onCreated: (n: NetworkFileConfig) => void }) {
  const [matchName, setMatchName] = useState('')
  const [addresses, setAddresses] = useState('')
  const [gateway, setGateway] = useState('')
  const [dns, setDns] = useState('')
  const [dhcp, setDhcp] = useState('no')
  const [bridge, setBridge] = useState('')
  const [bond, setBond] = useState('')
  const [mtu, setMtu] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!matchName.trim()) { setErr('Interface name is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateNetworkFileRequest = {
        match_name: matchName.trim(),
        addresses: addresses ? addresses.split(',').map(s => s.trim()).filter(Boolean) : [],
        gateway: gateway.trim() || undefined,
        dns: dns ? dns.split(',').map(s => s.trim()).filter(Boolean) : [],
        dhcp: (dhcp as CreateNetworkFileRequest['dhcp']) || undefined,
        bridge: bridge.trim() || undefined,
        bond: bond.trim() || undefined,
        mtu: mtu ? parseInt(mtu) : undefined,
      }
      const netfile = await api.createNetworkFile(req)
      onCreated(netfile)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Configure Interface" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Interface Name" value={matchName} onChange={setMatchName} placeholder="enp3s0" />
        <InputField label="Addresses (comma-separated)" value={addresses} onChange={setAddresses} placeholder="192.168.1.10/24" />
        <InputField label="Gateway" value={gateway} onChange={setGateway} placeholder="192.168.1.1" />
        <InputField label="DNS (comma-separated)" value={dns} onChange={setDns} placeholder="8.8.8.8, 1.1.1.1" />
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">DHCP</label>
          <select aria-label="DHCP" value={dhcp} onChange={e => setDhcp(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500">
            <option value="no">no</option>
            <option value="yes">yes</option>
            <option value="ipv4">ipv4</option>
            <option value="ipv6">ipv6</option>
          </select>
        </div>
        <InputField label="Bridge (attach to)" value={bridge} onChange={setBridge} placeholder="br0" />
        <InputField label="Bond (attach to)" value={bond} onChange={setBond} placeholder="bond0" />
        <InputField label="MTU" value={mtu} onChange={setMtu} placeholder="1500" type="number" />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-yellow-600 hover:bg-yellow-700 disabled:opacity-50 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Configure Interface'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default NetfilesTabContent
