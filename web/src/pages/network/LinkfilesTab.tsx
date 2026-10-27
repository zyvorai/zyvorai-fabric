// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useState } from 'react'
import { Plus, FolderOpen, ChevronDown, ChevronRight } from 'lucide-react'
import * as api from '../../api/networkd'
import type { LinkFileConfig, CreateLinkFileRequest } from '../../api/networkd'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from './ModalShared'
import { ListControls, DEFAULT_PAGE_SIZE, paginateSlice } from './ListControls'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

interface LinkfilesTabProps {
  linkfiles: LinkFileConfig[]
  onDelete: (id: string) => void
  onCreate: () => void
}

function ManagedFilesPanel() {
  const [open, setOpen] = useState(false)
  const [files, setFiles] = useState<string[] | null>(null)
  const [loading, setLoading] = useState(false)
  const [err, setErr] = useState('')

  const toggle = async () => {
    const next = !open
    setOpen(next)
    if (next && files === null) {
      setLoading(true)
      setErr('')
      try {
        setFiles(await api.listManagedFiles())
      } catch (e: unknown) {
        setErr(extractErrorMessage(e))
      } finally {
        setLoading(false)
      }
    }
  }

  return (
    <div className="border-b border-[var(--zf-hairline)]">
      <button
        type="button"
        onClick={toggle}
        className="w-full flex items-center gap-2 px-6 py-3 text-sm text-[var(--zf-ink)] hover:bg-white/[0.03] transition"
      >
        {open ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
        <FolderOpen className="w-4 h-4" />
        Managed config files on host
      </button>
      {open && (
        <div className="px-6 pb-4">
          {loading && <p className="text-[var(--zf-muted)] text-sm">Loading…</p>}
          {err && <p className="text-red-600 text-sm">{err}</p>}
          {files && files.length === 0 && <p className="text-[var(--zf-muted)] text-sm">No managed config files found.</p>}
          {files && files.length > 0 && (
            <ul className="space-y-1 max-h-64 overflow-y-auto">
              {files.map(f => (
                <li key={f} className="font-mono text-xs text-[var(--zf-muted)] truncate">{f}</li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  )
}

function LinkfilesTabContent({ linkfiles, onDelete, onCreate }: LinkfilesTabProps) {
  const readOnly = useReadOnly()
  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)
  const [showAll, setShowAll] = useState(false)

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    const list = [...linkfiles].sort((a, b) => (a.name ?? a.match_mac ?? '').localeCompare(b.name ?? b.match_mac ?? ''))
    if (!q) return list
    return list.filter(l => {
      const hay = [l.source_file, l.match_mac, l.match_original_name, l.match_driver, l.match_path, l.name, l.mac_address].filter(Boolean).join(' ').toLowerCase()
      return hay.includes(q)
    })
  }, [linkfiles, search])

  const pageItems = paginateSlice(filtered, page, DEFAULT_PAGE_SIZE, showAll)

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <h2 className="text-xl font-semibold">Link Configuration (.link)</h2>
        {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-pink-600 hover:bg-pink-700 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
          <Plus className="w-4 h-4" /> Create Link File
        </button>}
      </div>
      <ManagedFilesPanel />
      {linkfiles.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No link files configured. Use these to rename interfaces, set MTU, MAC, or Wake-on-LAN.</div>
      ) : (
        <>
          <ListControls search={search} onSearchChange={setSearch} searchPlaceholder="Search match, rename, MAC…" total={linkfiles.length} filtered={filtered.length} page={page} pageSize={DEFAULT_PAGE_SIZE} onPageChange={setPage} showAll={showAll} onShowAllChange={setShowAll} />
          <div className="overflow-x-auto">
          <table className="w-full">
            <thead className="bg-[var(--zf-surface)]">
              <tr>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Match</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Rename To</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">MTU</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">MAC Override</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">WoL</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {pageItems.map(l => (
                <tr key={l.id} className="hover:bg-white/[0.03] transition">
                  <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">
                    {l.source_file ?? l.match_mac ?? l.match_original_name ?? l.match_driver ?? l.match_path ?? '-'}
                    {isHostManaged(l) && <HostBadge />}
                  </td>
                  <td className="p-4 font-medium">{l.name ?? '-'}</td>
                  <td className="p-4 text-[var(--zf-muted)]">{l.mtu ?? '-'}</td>
                  <td className="p-4 text-[var(--zf-muted)] font-mono text-sm">{l.mac_address ?? '-'}</td>
                  <td className="p-4 text-[var(--zf-muted)]">{l.wake_on_lan ?? '-'}</td>
                  <td className="p-4">
                    <HostManagedActions readOnly={readOnly} item={l} onDelete={() => onDelete(l.id)} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {filtered.length === 0 && <div className="p-8 text-center text-[var(--zf-muted)] text-sm">No link files match your search.</div>}
          </div>
        </>
      )}
    </div>
  )
}

export function CreateLinkfileModal({ onClose, onCreated }: { onClose: () => void; onCreated: (l: LinkFileConfig) => void }) {
  const [matchMac, setMatchMac] = useState('')
  const [matchOrigName, setMatchOrigName] = useState('')
  const [name, setName] = useState('')
  const [mtu, setMtu] = useState('')
  const [macAddress, setMacAddress] = useState('')
  const [wol, setWol] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!matchMac.trim() && !matchOrigName.trim()) { setErr('At least one match criterion is required (MAC or original name)'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateLinkFileRequest = {
        match_mac: matchMac.trim() || undefined,
        match_original_name: matchOrigName.trim() || undefined,
        name: name.trim() || undefined,
        mtu: mtu ? parseInt(mtu) : undefined,
        mac_address: macAddress.trim() || undefined,
        wake_on_lan: wol.trim() || undefined,
      }
      const linkfile = await api.createLinkFile(req)
      onCreated(linkfile)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create Link File" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Match MAC Address" value={matchMac} onChange={setMatchMac} placeholder="00:11:22:33:44:55" />
        <InputField label="Match Original Name" value={matchOrigName} onChange={setMatchOrigName} placeholder="en*" />
        <InputField label="Rename To" value={name} onChange={setName} placeholder="lan0" />
        <InputField label="MTU" value={mtu} onChange={setMtu} placeholder="9000" type="number" />
        <InputField label="Override MAC Address" value={macAddress} onChange={setMacAddress} placeholder="52:54:00:aa:bb:cc" />
        <InputField label="Wake-on-LAN" value={wol} onChange={setWol} placeholder="magic" />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-pink-600 hover:bg-pink-700 disabled:opacity-50 text-[var(--zf-ink)] py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create Link File'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default LinkfilesTabContent
