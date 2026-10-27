// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useState } from 'react'
import { Plus, Pencil, Router, Trash2 } from 'lucide-react'
import * as api from '../../api/networkd'
import type { BridgeConfig, CreateBridgeRequest } from '../../api/networkd'
import * as cloudApi from '../../api/network-cloud'
import type { DhcpServerConfig, CreateDhcpServerRequest } from '../../api/network-cloud'
import { ModalWrapper, InputField, CheckboxField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from './ModalShared'
import { ListControls, DEFAULT_PAGE_SIZE, paginateSlice } from './ListControls'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

interface BridgesTabProps {
  bridges: BridgeConfig[]
  dhcpServers: DhcpServerConfig[]
  onDelete: (id: string) => void
  onAdopt: (id: string) => void
  onCreate: () => void
  onEdit: (b: BridgeConfig) => void
  onConfigureDhcp: (b: BridgeConfig) => void
}

function BridgesTabContent({ bridges, dhcpServers, onDelete, onAdopt, onCreate, onEdit, onConfigureDhcp }: BridgesTabProps) {
  const readOnly = useReadOnly()
  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)
  const [showAll, setShowAll] = useState(false)

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    const list = [...bridges].sort((a, b) => a.name.localeCompare(b.name))
    if (!q) return list
    return list.filter(b => [b.name, b.addresses.join(' '), b.dhcp].join(' ').toLowerCase().includes(q))
  }, [bridges, search])

  const pageItems = paginateSlice(filtered, page, DEFAULT_PAGE_SIZE, showAll)

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <h2 className="text-xl font-semibold">Network Bridges</h2>
        {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-[var(--zf-link)] hover:bg-[var(--zf-link-hover)] text-white py-2 px-4 rounded-lg transition text-sm">
          <Plus className="w-4 h-4" /> Create Bridge
        </button>}
      </div>
      {bridges.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No bridges configured. Create one to get started.</div>
      ) : (
        <>
          <ListControls search={search} onSearchChange={setSearch} searchPlaceholder="Search name, addresses…" total={bridges.length} filtered={filtered.length} page={page} pageSize={DEFAULT_PAGE_SIZE} onPageChange={setPage} showAll={showAll} onShowAllChange={setShowAll} />
          <div className="overflow-x-auto">
          <table className="w-full">
            <thead className="bg-[var(--zf-surface)]">
              <tr>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Addresses</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">STP</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">DHCP</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">MTU</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {pageItems.map(b => (
                <tr key={b.id} className="hover:bg-white/[0.03] transition">
                  <td className="p-4 font-medium">
                    {b.name}
                    {isHostManaged(b) && <HostBadge />}
                  </td>
                  <td className="p-4 text-[var(--zf-muted)] font-mono text-sm">{b.addresses.join(', ') || '-'}</td>
                  <td className="p-4">{b.stp ? <span className="text-emerald-600">on</span> : <span className="text-[var(--zf-muted)]">off</span>}</td>
                  <td className="p-4 text-[var(--zf-muted)]">{b.dhcp}</td>
                  <td className="p-4 text-[var(--zf-muted)]">{b.mtu ?? '-'}</td>
                  <td className="p-4">
                    <div className="flex items-center gap-1">
                      {!readOnly && !isHostManaged(b) && (
                        <button onClick={() => onEdit(b)} className="p-2 hover:bg-white/[0.06] rounded transition" title="Edit bridge" type="button">
                          <Pencil className="w-4 h-4" />
                        </button>
                      )}
                      {!readOnly && !isHostManaged(b) && (
                        <button onClick={() => onConfigureDhcp(b)} className="p-2 hover:bg-white/[0.06] rounded transition" title={dhcpServers.some(d => d.bridge === b.name) ? 'DHCP server configured' : 'Configure DHCP server'} type="button">
                          <Router className={`w-4 h-4 ${dhcpServers.some(d => d.bridge === b.name) ? 'text-emerald-600' : ''}`} />
                        </button>
                      )}
                      <HostManagedActions readOnly={readOnly} item={b} onDelete={() => onDelete(b.id)} onAdopt={() => onAdopt(b.id)} />
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {filtered.length === 0 && <div className="p-8 text-center text-[var(--zf-muted)] text-sm">No bridges match your search.</div>}
          </div>
        </>
      )}
    </div>
  )
}

export function CreateBridgeModal({ onClose, onCreated }: { onClose: () => void; onCreated: (b: BridgeConfig) => void }) {
  const [name, setName] = useState('')
  const [addresses, setAddresses] = useState('')
  const [gateway, setGateway] = useState('')
  const [dns, setDns] = useState('')
  const [stp, setStp] = useState(false)
  const [mtu, setMtu] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateBridgeRequest = {
        name: name.trim(),
        stp: stp || undefined,
        mtu: mtu ? parseInt(mtu) : undefined,
        addresses: addresses ? addresses.split(',').map(s => s.trim()).filter(Boolean) : [],
        gateway: gateway.trim() || undefined,
        dns: dns ? dns.split(',').map(s => s.trim()).filter(Boolean) : [],
      }
      const bridge = await api.createBridge(req)
      onCreated(bridge)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create Bridge" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="br0" />
        <InputField label="Addresses (comma-separated)" value={addresses} onChange={setAddresses} placeholder="10.0.0.1/24, 192.168.1.1/24" />
        <InputField label="Gateway" value={gateway} onChange={setGateway} placeholder="10.0.0.254" />
        <InputField label="DNS (comma-separated)" value={dns} onChange={setDns} placeholder="8.8.8.8, 1.1.1.1" />
        <InputField label="MTU" value={mtu} onChange={setMtu} placeholder="1500" type="number" />
        <CheckboxField label="Enable STP (Spanning Tree Protocol)" checked={stp} onChange={setStp} />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link)] hover:bg-[var(--zf-link-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create Bridge'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export function EditBridgeModal({ bridge, onClose, onUpdated }: { bridge: BridgeConfig; onClose: () => void; onUpdated: (b: BridgeConfig) => void }) {
  const [name, setName] = useState(bridge.name)
  const [addresses, setAddresses] = useState(bridge.addresses.join(', '))
  const [gateway, setGateway] = useState(bridge.gateway ?? '')
  const [dns, setDns] = useState(bridge.dns.join(', '))
  const [stp, setStp] = useState(bridge.stp ?? false)
  const [mtu, setMtu] = useState(bridge.mtu ? String(bridge.mtu) : '')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateBridgeRequest = {
        name: name.trim(),
        stp: stp || undefined,
        mtu: mtu ? parseInt(mtu) : undefined,
        addresses: addresses ? addresses.split(',').map(s => s.trim()).filter(Boolean) : [],
        gateway: gateway.trim() || undefined,
        dns: dns ? dns.split(',').map(s => s.trim()).filter(Boolean) : [],
      }
      const updated = await api.updateBridge(bridge.id, req)
      onUpdated(updated)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Edit Bridge" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="br0" />
        <InputField label="Addresses (comma-separated)" value={addresses} onChange={setAddresses} placeholder="10.0.0.1/24, 192.168.1.1/24" />
        <InputField label="Gateway" value={gateway} onChange={setGateway} placeholder="10.0.0.254" />
        <InputField label="DNS (comma-separated)" value={dns} onChange={setDns} placeholder="8.8.8.8, 1.1.1.1" />
        <InputField label="MTU" value={mtu} onChange={setMtu} placeholder="1500" type="number" />
        <CheckboxField label="Enable STP (Spanning Tree Protocol)" checked={stp} onChange={setStp} />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link)] hover:bg-[var(--zf-link-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Saving...' : 'Save Changes'}
        </button>
      </div>
    </ModalWrapper>
  )
}

/// Configures (or removes) the DHCP server for one bridge -- one dnsmasq
/// instance per bridge, handing out leases plus (via zone_hosts_dir on the
/// backend) serving DNS Zone/Policy records to anything on that bridge.
/// An existing config shows as a read-only summary with Edit/Remove
/// actions; Edit reopens the same form pre-filled, submitting through
/// updateDhcpServer (PUT) instead of create.
export function DhcpServerModal({ bridge, existing, onClose, onCreated, onDeleted }: {
  bridge: BridgeConfig
  existing: DhcpServerConfig | null
  onClose: () => void
  onCreated: (d: DhcpServerConfig) => void
  onDeleted: (id: string) => void
}) {
  const defaultGateway = bridge.addresses[0]?.split('/')[0] ?? ''
  const [editing, setEditing] = useState(!existing)
  const [gateway, setGateway] = useState(existing?.gateway ?? defaultGateway)
  const [poolOffset, setPoolOffset] = useState(String(existing?.pool_offset ?? 100))
  const [poolSize, setPoolSize] = useState(String(existing?.pool_size ?? 100))
  const [dnsServers, setDnsServers] = useState((existing?.dns_servers ?? []).join(', '))
  const [domain, setDomain] = useState(existing?.domain ?? '')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!gateway.trim()) { setErr('Gateway is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateDhcpServerRequest = {
        bridge: bridge.name,
        gateway: gateway.trim(),
        pool_offset: parseInt(poolOffset) || 100,
        pool_size: parseInt(poolSize) || 100,
        dns_servers: dnsServers ? dnsServers.split(',').map(s => s.trim()).filter(Boolean) : undefined,
        domain: domain.trim() || undefined,
      }
      const saved = existing ? await cloudApi.updateDhcpServer(existing.id, req) : await cloudApi.createDhcpServer(req)
      onCreated(saved)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  const handleDelete = async () => {
    if (!existing) return
    setSubmitting(true)
    setErr('')
    try {
      await cloudApi.deleteDhcpServer(existing.id)
      onDeleted(existing.id)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
      setSubmitting(false)
    }
  }

  if (existing && !editing) {
    return (
      <ModalWrapper title={`DHCP Server — ${bridge.name}`} onClose={onClose}>
        <div className="space-y-4">
          <p className="text-sm text-[var(--zf-muted)]">
            This bridge's dnsmasq instance hands out leases in{' '}
            <span className="font-mono text-[var(--zf-ink)]">{existing.gateway?.split('.').slice(0, 3).join('.')}.{existing.pool_offset}–{existing.pool_offset + existing.pool_size - 1}</span>
            {' '}and also serves DNS Zone/Policy records to anything on this bridge.
          </p>
          <div className="grid grid-cols-2 gap-3 text-sm">
            <div><div className="text-[var(--zf-muted)] text-xs mb-1">Gateway</div><div className="font-mono">{existing.gateway ?? '—'}</div></div>
            <div><div className="text-[var(--zf-muted)] text-xs mb-1">Lease time</div><div>{existing.default_lease_time_sec}s</div></div>
            <div><div className="text-[var(--zf-muted)] text-xs mb-1">DNS servers</div><div className="font-mono">{existing.dns_servers.join(', ') || '—'}</div></div>
            <div><div className="text-[var(--zf-muted)] text-xs mb-1">Domain</div><div>{existing.domain ?? '—'}</div></div>
          </div>
          {err && <p className="text-red-600 text-sm">{err}</p>}
          <div className="flex gap-2">
            <button onClick={() => setEditing(true)} className="flex-1 flex items-center justify-center gap-2 bg-[var(--zf-canvas-alt)] hover:bg-[var(--zf-hairline)] text-[var(--zf-ink)] py-2 px-4 rounded-lg transition">
              <Pencil className="w-4 h-4" /> Edit
            </button>
            <button onClick={handleDelete} disabled={submitting} className="flex-1 flex items-center justify-center gap-2 bg-red-600/20 hover:bg-red-600/30 text-red-600 disabled:opacity-50 py-2 px-4 rounded-lg transition">
              <Trash2 className="w-4 h-4" /> {submitting ? 'Removing...' : 'Remove'}
            </button>
          </div>
        </div>
      </ModalWrapper>
    )
  }

  return (
    <ModalWrapper title={existing ? `Edit DHCP Server — ${bridge.name}` : `Configure DHCP — ${bridge.name}`} onClose={onClose}>
      <div className="space-y-4">
        <p className="text-sm text-[var(--zf-muted)]">
          {existing
            ? 'Restarts this bridge’s dnsmasq instance with the updated settings.'
            : 'Starts a dnsmasq instance bound to this bridge, handing out leases and serving DNS Zone/Policy records to VMs on it.'}
        </p>
        <InputField label="Gateway" value={gateway} onChange={setGateway} placeholder={defaultGateway || '10.0.0.1'} />
        <div className="grid grid-cols-2 gap-2">
          <InputField label="Pool Start Offset" value={poolOffset} onChange={setPoolOffset} placeholder="100" type="number" />
          <InputField label="Pool Size" value={poolSize} onChange={setPoolSize} placeholder="100" type="number" />
        </div>
        <InputField label="DNS Servers (comma-separated, optional)" value={dnsServers} onChange={setDnsServers} placeholder="defaults to this bridge's own gateway" />
        <InputField label="Domain (optional)" value={domain} onChange={setDomain} placeholder="vms.local" />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <div className="flex gap-2">
          {existing && (
            <button onClick={() => setEditing(false)} className="flex-1 bg-[var(--zf-canvas-alt)] hover:bg-[var(--zf-hairline)] text-[var(--zf-ink)] py-2 px-4 rounded-lg transition">
              Cancel
            </button>
          )}
          <button onClick={handleSubmit} disabled={submitting} className="flex-1 bg-[var(--zf-link)] hover:bg-[var(--zf-link-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
            {submitting ? 'Saving...' : existing ? 'Save Changes' : 'Start DHCP Server'}
          </button>
        </div>
      </div>
    </ModalWrapper>
  )
}

export default BridgesTabContent
