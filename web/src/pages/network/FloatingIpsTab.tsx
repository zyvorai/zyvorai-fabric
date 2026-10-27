// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo, useState } from 'react'
import { Plus, Trash2, Link2, Unlink } from 'lucide-react'
import * as cloudApi from '../../api/network-cloud'
import type { FloatingIp, CreateFloatingIpRequest } from '../../api/network-cloud'
import type { VM } from '../../api/vm'
import { listVMs } from '../../api/vm'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from './ModalShared'
import { ListControls, DEFAULT_PAGE_SIZE, paginateSlice } from './ListControls'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

interface FloatingIpsTabProps {
  floatingIps: FloatingIp[]
  onDelete: (id: string) => void
  onAdopt?: (id: string) => void
  onAssign?: (id: string, vmName: string) => void
  onUnassign?: (id: string) => void
  onCreate: () => void
}

function FloatingIpsTabContent({ floatingIps, onDelete, onAdopt, onAssign, onUnassign, onCreate }: FloatingIpsTabProps) {
  const readOnly = useReadOnly()
  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)
  const [showAll, setShowAll] = useState(false)
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    const list = [...floatingIps].sort((a, b) => a.address.localeCompare(b.address))
    if (!q) return list
    return list.filter(f => [f.address, f.interface, f.assigned_vm ?? ''].join(' ').toLowerCase().includes(q))
  }, [floatingIps, search])
  const pageItems = paginateSlice(filtered, page, DEFAULT_PAGE_SIZE, showAll)

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <h2 className="text-xl font-semibold">Floating IPs</h2>
        {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-cyan-600 hover:bg-cyan-700 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
          <Plus className="w-4 h-4" /> Add Floating IP
        </button>}
      </div>
      {floatingIps.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No floating IPs configured. Host secondary addresses appear here when discovered, or create one to assign to a VM.</div>
      ) : (
        <>
          <ListControls search={search} onSearchChange={setSearch} searchPlaceholder="Search address, interface, VM…" total={floatingIps.length} filtered={filtered.length} page={page} pageSize={DEFAULT_PAGE_SIZE} onPageChange={setPage} showAll={showAll} onShowAllChange={setShowAll} />
          <div className="overflow-x-auto">
          <table className="w-full">
            <thead className="bg-[var(--zf-surface)]">
              <tr>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Address</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Interface</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Assigned VM</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {pageItems.map(f => (
                <tr key={f.id} className="hover:bg-white/[0.03] transition">
                  <td className="p-4 font-mono text-cyan-400">
                    {f.address}
                    {isHostManaged(f) && <HostBadge />}
                  </td>
                  <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">{f.interface}</td>
                  <td className="p-4 text-[var(--zf-muted)]">{f.assigned_vm ?? '—'}</td>
                  <td className="p-4">
                    {isHostManaged(f) ? (
                      <HostManagedActions readOnly={readOnly}
                        item={f}
                        onDelete={() => onDelete(f.id)}
                        onAdopt={onAdopt ? () => onAdopt(f.id) : undefined}
                      />
                    ) : (
                      <div className="flex items-center gap-2">
                        {!readOnly && f.assigned_vm ? (
                          onUnassign && (
                            <button type="button" onClick={() => onUnassign(f.id)} className="flex items-center gap-1 px-2 py-1 text-xs rounded bg-[var(--zf-canvas-alt)] hover:bg-[var(--zf-hairline)] text-[var(--zf-ink)] transition" title="Unassign from VM">
                              <Unlink className="w-3.5 h-3.5" /> Unassign
                            </button>
                          )
                        ) : (
                          !readOnly && onAssign && (
                            <AssignButton fip={f} onAssign={onAssign} />
                          )
                        )}
                        {!readOnly && (
                        <button type="button" onClick={() => onDelete(f.id)} className="p-2 hover:bg-red-600/20 rounded transition text-red-600" title="Delete">
                          <Trash2 className="w-4 h-4" />
                        </button>
                        )}
                      </div>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {filtered.length === 0 && <div className="p-8 text-center text-[var(--zf-muted)] text-sm">No floating IPs match your search.</div>}
          </div>
        </>
      )}
    </div>
  )
}

function AssignButton({ fip, onAssign }: { fip: FloatingIp; onAssign: (id: string, vmName: string) => void }) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <button type="button" onClick={() => setOpen(true)} className="flex items-center gap-1 px-2 py-1 text-xs rounded bg-cyan-600/20 text-cyan-300 border border-cyan-500/30 hover:bg-cyan-600/40 transition">
        <Link2 className="w-3.5 h-3.5" /> Assign
      </button>
      {open && (
        <AssignFloatingIpModal
          fip={fip}
          onClose={() => setOpen(false)}
          onAssigned={(vmName) => { onAssign(fip.id, vmName); setOpen(false) }}
        />
      )}
    </>
  )
}

export function CreateFloatingIpModal({
  interfaceOptions,
  onClose,
  onCreated,
}: {
  interfaceOptions: string[]
  onClose: () => void
  onCreated: (f: FloatingIp) => void
}) {
  const [address, setAddress] = useState('')
  const [iface, setIface] = useState(interfaceOptions[0] ?? '')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!address.trim() || !iface.trim()) { setErr('Address and interface are required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateFloatingIpRequest = { address: address.trim(), interface: iface.trim() }
      const f = await cloudApi.createFloatingIp(req)
      onCreated(f)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create Floating IP" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Address" value={address} onChange={setAddress} placeholder="203.0.113.10" />
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">Interface</label>
          {interfaceOptions.length > 0 ? (
            <select value={iface} onChange={e => setIface(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
              {interfaceOptions.map(name => (
                <option key={name} value={name}>{name}</option>
              ))}
            </select>
          ) : (
            <input
              type="text"
              value={iface}
              onChange={e => setIface(e.target.value)}
              placeholder="eno1"
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500"
            />
          )}
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-cyan-600 hover:bg-cyan-700 disabled:opacity-50 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating…' : 'Create Floating IP'}
        </button>
      </div>
    </ModalWrapper>
  )
}

function AssignFloatingIpModal({
  fip,
  onClose,
  onAssigned,
}: {
  fip: FloatingIp
  onClose: () => void
  onAssigned: (vmName: string) => void
}) {
  const [vms, setVms] = useState<VM[]>([])
  const [vmName, setVmName] = useState('')
  const [loading, setLoading] = useState(true)
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  useEffect(() => {
    listVMs()
      .then(list => { setVms(list); setVmName(list[0]?.name ?? '') })
      .catch((e: unknown) => setErr(extractErrorMessage(e)))
      .finally(() => setLoading(false))
  }, [])

  const handleSubmit = async () => {
    if (!vmName) { setErr('Select a VM'); return }
    setSubmitting(true)
    setErr('')
    try {
      await cloudApi.assignFloatingIp(fip.id, vmName)
      onAssigned(vmName)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title={`Assign ${fip.address}`} onClose={onClose}>
      <div className="space-y-4">
        <p className="text-sm text-[var(--zf-muted)]">Bind this address on <span className="font-mono text-[var(--zf-ink)]">{fip.interface}</span> and record the VM assignment.</p>
        {loading ? (
          <p className="text-sm text-[var(--zf-muted)]">Loading VMs…</p>
        ) : vms.length === 0 ? (
          <p className="text-sm text-amber-400">No VMs available. Create a VM first.</p>
        ) : (
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">VM</label>
            <select value={vmName} onChange={e => setVmName(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
              {vms.map(vm => (
                <option key={vm.name} value={vm.name}>{vm.name} ({vm.state})</option>
              ))}
            </select>
          </div>
        )}
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting || loading || vms.length === 0} className="w-full bg-cyan-600 hover:bg-cyan-700 disabled:opacity-50 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition">
          {submitting ? 'Assigning…' : 'Assign to VM'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default FloatingIpsTabContent
