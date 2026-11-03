// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useState } from 'react'
import { Plus, Eye } from 'lucide-react'
import * as api from '../../api/networkd'
import type { TapConfig, CreateTapRequest } from '../../api/networkd'
import { ModalWrapper, InputField, CheckboxField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage, DetailModal } from './ModalShared'
import { ListControls, DEFAULT_PAGE_SIZE, paginateSlice } from './ListControls'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

interface TapsTabProps {
  taps: TapConfig[]
  onDelete: (id: string) => void
  onAdopt: (id: string) => void
  onCreate: () => void
}

function TapsTabContent({ taps, onDelete, onAdopt, onCreate }: TapsTabProps) {
  const readOnly = useReadOnly()
  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)
  const [showAll, setShowAll] = useState(false)
  const [viewingId, setViewingId] = useState<string | null>(null)
  const [viewData, setViewData] = useState<TapConfig | null>(null)
  const [viewLoading, setViewLoading] = useState(false)
  const [viewErr, setViewErr] = useState('')

  const handleView = async (id: string) => {
    setViewingId(id)
    setViewLoading(true)
    setViewErr('')
    setViewData(null)
    try {
      setViewData(await api.getTap(id))
    } catch (e: unknown) {
      setViewErr(extractErrorMessage(e))
    } finally {
      setViewLoading(false)
    }
  }

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    const list = [...taps].sort((a, b) => a.name.localeCompare(b.name))
    if (!q) return list
    return list.filter(t => [t.name, t.bridge ?? '', t.user ?? ''].join(' ').toLowerCase().includes(q))
  }, [taps, search])
  const pageItems = paginateSlice(filtered, page, DEFAULT_PAGE_SIZE, showAll)

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <h2 className="text-xl font-semibold">Tap Devices</h2>
        {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-orange-600 hover:bg-orange-700 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
          <Plus className="w-4 h-4" /> Create Tap
        </button>}
      </div>
      {taps.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No tap devices configured.</div>
      ) : (
        <>
          <ListControls search={search} onSearchChange={setSearch} searchPlaceholder="Search name, bridge, user…" total={taps.length} filtered={filtered.length} page={page} pageSize={DEFAULT_PAGE_SIZE} onPageChange={setPage} showAll={showAll} onShowAllChange={setShowAll} />
          <div className="overflow-x-auto">
          <table className="w-full">
            <thead className="bg-[var(--zf-surface)]">
              <tr>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Bridge</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">User</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">MultiQueue</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">VNet Header</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {pageItems.map(t => (
                <tr key={t.id} className="hover:bg-white/[0.03] transition">
                  <td className="p-4 font-medium">{t.name}{isHostManaged(t) && <HostBadge />}</td>
                  <td className="p-4 text-[var(--zf-muted)]">{t.bridge ?? '-'}</td>
                  <td className="p-4 text-[var(--zf-muted)]">{t.user ?? '-'}</td>
                  <td className="p-4">{t.multi_queue ? <span className="text-emerald-600">yes</span> : <span className="text-[var(--zf-muted)]">no</span>}</td>
                  <td className="p-4">{t.vnet_hdr ? <span className="text-emerald-600">yes</span> : <span className="text-[var(--zf-muted)]">no</span>}</td>
                  <td className="p-4">
                    <div className="flex items-center gap-1">
                      <button onClick={() => handleView(t.id)} className="p-2 hover:bg-white/[0.06] rounded transition" title="View details" type="button">
                        <Eye className="w-4 h-4" />
                      </button>
                      <HostManagedActions readOnly={readOnly} item={t} onDelete={() => onDelete(t.id)} onAdopt={() => onAdopt(t.id)} />
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {filtered.length === 0 && <div className="p-8 text-center text-[var(--zf-muted)] text-sm">No tap devices match your search.</div>}
          </div>
        </>
      )}
      {viewingId && (
        <DetailModal
          title="Tap Details"
          data={viewData as unknown as Record<string, unknown> | null}
          loading={viewLoading}
          error={viewErr}
          onClose={() => setViewingId(null)}
        />
      )}
    </div>
  )
}

export function CreateTapModal({ onClose, onCreated }: { onClose: () => void; onCreated: (t: TapConfig) => void }) {
  const [name, setName] = useState('')
  const [bridge, setBridge] = useState('')
  const [user, setUser] = useState('')
  const [group, setGroup] = useState('')
  const [multiQueue, setMultiQueue] = useState(false)
  const [vnetHdr, setVnetHdr] = useState(true)
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateTapRequest = {
        name: name.trim(),
        bridge: bridge.trim() || undefined,
        user: user.trim() || undefined,
        group: group.trim() || undefined,
        multi_queue: multiQueue || undefined,
        vnet_hdr: vnetHdr || undefined,
      }
      const tap = await api.createTap(req)
      onCreated(tap)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create Tap Device" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="tap0" />
        <InputField label="Bridge (attach to)" value={bridge} onChange={setBridge} placeholder="br0" />
        <InputField label="User" value={user} onChange={setUser} placeholder="qemu" />
        <InputField label="Group" value={group} onChange={setGroup} placeholder="kvm" />
        <CheckboxField label="Multi-queue" checked={multiQueue} onChange={setMultiQueue} />
        <CheckboxField label="VNet header" checked={vnetHdr} onChange={setVnetHdr} />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-orange-600 hover:bg-orange-700 disabled:opacity-50 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create Tap'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default TapsTabContent
