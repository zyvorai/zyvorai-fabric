// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useState } from 'react'
import { Plus, RefreshCw, Eye } from 'lucide-react'
import * as api from '../../api/networkd'
import type { PortForwardConfig, CreatePortForwardRequest, Protocol } from '../../api/networkd'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage, DetailModal } from './ModalShared'
import { ListControls, DEFAULT_PAGE_SIZE, paginateSlice } from './ListControls'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

type PfOriginFilter = 'all' | 'managed' | 'host' | 'zyvor-fabricd'

function portForwardOrigin(pf: PortForwardConfig): Exclude<PfOriginFilter, 'all'> {
  if (!isHostManaged(pf)) return 'managed'
  if (pf.id.startsWith('host:nft-ext')) return 'host'
  if (pf.id.startsWith('host:nft')) return 'zyvor-fabricd'
  return 'host'
}

interface PortForwardsTabProps {
  portForwards: PortForwardConfig[]
  onDelete: (id: string) => void
  onAdopt?: (id: string) => void
  onCreate: () => void
  onSync: () => void
}

function PortForwardsTabContent({ portForwards, onDelete, onAdopt, onCreate, onSync }: PortForwardsTabProps) {
  const readOnly = useReadOnly()
  const [search, setSearch] = useState('')
  const [originFilter, setOriginFilter] = useState<PfOriginFilter>('all')
  const [page, setPage] = useState(1)
  const [showAll, setShowAll] = useState(false)
  const [viewingId, setViewingId] = useState<string | null>(null)
  const [viewData, setViewData] = useState<PortForwardConfig | null>(null)
  const [viewLoading, setViewLoading] = useState(false)
  const [viewErr, setViewErr] = useState('')

  const handleView = async (id: string) => {
    setViewingId(id)
    setViewLoading(true)
    setViewErr('')
    setViewData(null)
    try {
      setViewData(await api.getPortForward(id))
    } catch (e: unknown) {
      setViewErr(extractErrorMessage(e))
    } finally {
      setViewLoading(false)
    }
  }

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    let list = [...portForwards].sort((a, b) => a.name.localeCompare(b.name))
    if (originFilter !== 'all') {
      list = list.filter(pf => portForwardOrigin(pf) === originFilter)
    }
    if (!q) return list
    return list.filter(pf => {
      const hay = [
        pf.name,
        pf.description ?? '',
        pf.protocol,
        String(pf.host_port),
        pf.guest_ip,
        String(pf.guest_port),
      ].join(' ').toLowerCase()
      return hay.includes(q)
    })
  }, [portForwards, search, originFilter])

  const pageItems = paginateSlice(filtered, page, DEFAULT_PAGE_SIZE, showAll)

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <h2 className="text-xl font-semibold">Port Forwards (nftables DNAT)</h2>
        <div className="flex gap-2">
          {!readOnly && <button onClick={onSync} className="flex items-center gap-2 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
            <RefreshCw className="w-4 h-4" /> Sync Rules
          </button>}
          {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-red-600 hover:bg-red-700 text-white py-2 px-4 rounded-lg transition text-sm">
            <Plus className="w-4 h-4" /> Add Port Forward
          </button>}
        </div>
      </div>
      {portForwards.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No port forwards configured. Add one to expose a VM service to the host network.</div>
      ) : (
        <>
          <ListControls
            search={search}
            onSearchChange={setSearch}
            searchPlaceholder="Search name, port, guest IP…"
            total={portForwards.length}
            filtered={filtered.length}
            page={page}
            pageSize={DEFAULT_PAGE_SIZE}
            onPageChange={setPage}
            showAll={showAll}
            onShowAllChange={setShowAll}
          />
          <div className="px-4 pb-2 flex flex-wrap gap-1">
            {([
              ['all', 'All'],
              ['managed', 'Managed'],
              ['zyvor-fabricd', 'zyvor-fabricd nft'],
              ['host', 'External'],
            ] as const).map(([key, label]) => (
              <button
                key={key}
                type="button"
                onClick={() => { setOriginFilter(key); setPage(1) }}
                className={`px-3 py-1 rounded-lg text-xs font-medium transition ${
                  originFilter === key
                    ? 'bg-red-600/30 text-red-300 border border-red-500/40'
                    : 'bg-[var(--zf-surface)] text-[var(--zf-muted)] border border-[var(--zf-hairline)] hover:text-[var(--zf-ink)]'
                }`}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="overflow-x-auto">
          <table className="w-full">
            <thead className="bg-[var(--zf-surface)]">
              <tr>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Protocol</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Host Port</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Guest IP:Port</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Enabled</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {pageItems.map(pf => (
                <tr key={pf.id} className="hover:bg-white/[0.03] transition">
                  <td className="p-4 font-medium">
                    <span className="flex items-center gap-2 flex-wrap">
                      {pf.name}
                      {isHostManaged(pf) && <HostBadge />}
                    </span>
                    {pf.description && (
                      <span className="block text-xs text-[var(--zf-muted)] font-normal mt-0.5">{pf.description}</span>
                    )}
                  </td>
                  <td className="p-4">
                    <span className="px-2 py-1 rounded text-xs font-medium bg-red-500/10 text-red-600 border border-red-500/20">{pf.protocol}</span>
                  </td>
                  <td className="p-4 font-mono text-sm text-[var(--zf-link)]">{pf.host_port}</td>
                  <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">{pf.guest_ip}:{pf.guest_port}</td>
                  <td className="p-4">{pf.enabled ? <span className="text-emerald-600">yes</span> : <span className="text-[var(--zf-muted)]">no</span>}</td>
                  <td className="p-4">
                    <div className="flex items-center gap-1">
                      <button onClick={() => handleView(pf.id)} className="p-2 hover:bg-white/[0.06] rounded transition" title="View details" type="button">
                        <Eye className="w-4 h-4" />
                      </button>
                      <HostManagedActions readOnly={readOnly}
                        item={pf}
                        onDelete={() => onDelete(pf.id)}
                        onAdopt={onAdopt ? () => onAdopt(pf.id) : undefined}
                      />
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {filtered.length === 0 && (
            <div className="p-8 text-center text-[var(--zf-muted)] text-sm">No port forwards match your filters.</div>
          )}
          </div>
        </>
      )}
      {viewingId && (
        <DetailModal
          title="Port Forward Details"
          data={viewData as unknown as Record<string, unknown> | null}
          loading={viewLoading}
          error={viewErr}
          onClose={() => setViewingId(null)}
        />
      )}
    </div>
  )
}

export function CreatePortForwardModal({ onClose, onCreated }: { onClose: () => void; onCreated: (pf: PortForwardConfig) => void }) {
  const [name, setName] = useState('')
  const [protocol, setProtocol] = useState<Protocol>('tcp')
  const [hostPort, setHostPort] = useState('')
  const [guestIp, setGuestIp] = useState('')
  const [guestPort, setGuestPort] = useState('')
  const [iface, setIface] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim() || !hostPort || !guestIp.trim() || !guestPort) {
      setErr('Name, host port, guest IP, and guest port are required')
      return
    }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreatePortForwardRequest = {
        name: name.trim(),
        protocol,
        host_port: parseInt(hostPort),
        guest_ip: guestIp.trim(),
        guest_port: parseInt(guestPort),
        interface: iface.trim() || undefined,
      }
      const pf = await api.createPortForward(req)
      onCreated(pf)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Add Port Forward" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="web-server" />
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">Protocol</label>
          <select aria-label="Protocol" value={protocol} onChange={e => setProtocol(e.target.value as Protocol)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500">
            <option value="tcp">TCP</option>
            <option value="udp">UDP</option>
            <option value="both">Both (TCP + UDP)</option>
          </select>
        </div>
        <InputField label="Host Port" value={hostPort} onChange={setHostPort} placeholder="8080" type="number" />
        <InputField label="Guest IP" value={guestIp} onChange={setGuestIp} placeholder="192.168.100.10" />
        <InputField label="Guest Port" value={guestPort} onChange={setGuestPort} placeholder="80" type="number" />
        <InputField label="Interface (optional)" value={iface} onChange={setIface} placeholder="eth0" />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-red-600 hover:bg-red-700 disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Add Port Forward'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default PortForwardsTabContent
