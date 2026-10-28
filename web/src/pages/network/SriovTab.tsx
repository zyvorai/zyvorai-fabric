// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useState } from 'react'
import { Plus } from 'lucide-react'
import * as api from '../../api/networkd'
import type { SriovConfig, CreateSriovRequest } from '../../api/networkd'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from './ModalShared'
import { ListControls, DEFAULT_PAGE_SIZE, paginateSlice } from './ListControls'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

interface SriovTabProps {
  sriov: SriovConfig[]
  onDelete: (id: string) => void
  onAdopt?: (id: string) => void
  onCreate: () => void
}

function SriovTabContent({ sriov, onDelete, onAdopt, onCreate }: SriovTabProps) {
  const readOnly = useReadOnly()
  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)
  const [showAll, setShowAll] = useState(false)
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    const list = [...sriov].sort((a, b) => a.pf_name.localeCompare(b.pf_name))
    if (!q) return list
    return list.filter(s => [s.pf_name, String(s.num_vfs)].join(' ').toLowerCase().includes(q))
  }, [sriov, search])
  const pageItems = paginateSlice(filtered, page, DEFAULT_PAGE_SIZE, showAll)

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <h2 className="text-xl font-semibold">SR-IOV</h2>
        {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-violet-600 hover:bg-violet-700 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
          <Plus className="w-4 h-4" /> Configure SR-IOV
        </button>}
      </div>
      {sriov.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No SR-IOV configurations.</div>
      ) : (
        <>
          <ListControls search={search} onSearchChange={setSearch} searchPlaceholder="Search PF name…" total={sriov.length} filtered={filtered.length} page={page} pageSize={DEFAULT_PAGE_SIZE} onPageChange={setPage} showAll={showAll} onShowAllChange={setShowAll} />
          <div className="overflow-x-auto">
          <table className="w-full">
            <thead className="bg-[var(--zf-surface)]">
              <tr>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">PF Name</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">VFs</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">VF Configs</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {pageItems.map(s => (
                <tr key={s.id} className="hover:bg-white/[0.03] transition">
                  <td className="p-4 font-medium">
                    {s.pf_name}
                    {isHostManaged(s) && <HostBadge />}
                  </td>
                  <td className="p-4 font-mono text-violet-400">{s.num_vfs}</td>
                  <td className="p-4 text-[var(--zf-muted)] text-sm">{s.vf_configs?.length ?? 0} entries</td>
                  <td className="p-4">
                    <HostManagedActions readOnly={readOnly}
                      item={s}
                      onDelete={() => onDelete(s.id)}
                      onAdopt={onAdopt ? () => onAdopt(s.id) : undefined}
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {filtered.length === 0 && <div className="p-8 text-center text-[var(--zf-muted)] text-sm">No SR-IOV configs match your search.</div>}
          </div>
        </>
      )}
    </div>
  )
}

export function CreateSriovModal({ onClose, onCreated }: { onClose: () => void; onCreated: (s: SriovConfig) => void }) {
  const [pfName, setPfName] = useState('')
  const [numVfs, setNumVfs] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!pfName.trim() || !numVfs) { setErr('PF name and number of VFs are required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateSriovRequest = {
        pf_name: pfName.trim(),
        num_vfs: parseInt(numVfs, 10),
        vf_configs: [],
      }
      const cfg = await api.createSriov(req)
      onCreated(cfg)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Configure SR-IOV" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Physical Function (PF)" value={pfName} onChange={setPfName} placeholder="eno2" />
        <InputField label="Number of VFs" value={numVfs} onChange={setNumVfs} placeholder="4" type="number" />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-violet-600 hover:bg-violet-700 disabled:opacity-50 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Configure SR-IOV'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default SriovTabContent
