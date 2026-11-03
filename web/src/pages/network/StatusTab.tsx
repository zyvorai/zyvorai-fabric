// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useState } from 'react'
import { RefreshCw } from 'lucide-react'
import * as api from '../../api/networkd'
import type { LinkInfo } from '../../api/networkd'
import { ListControls, DEFAULT_PAGE_SIZE, paginateSlice } from './ListControls'
import { DetailModal, extractErrorMessage } from './ModalShared'

interface StatusTabProps {
  links: LinkInfo[]
  onRefresh: () => void
}

function operStateClasses(state: string): string {
  const s = state.toLowerCase()
  if (s === 'routable' || s === 'up') return 'bg-green-500/10 text-emerald-600'
  if (s === 'carrier' || s === 'unknown') return 'bg-blue-500/10 text-[var(--zf-link)]'
  if (s === 'degraded' || s === 'dormant') return 'bg-yellow-500/10 text-amber-600'
  if (s === 'down' || s === 'lowerlayerdown' || s === 'no-carrier') return 'bg-black/[0.04] text-[var(--zf-muted)]'
  return 'bg-black/[0.04] text-[var(--zf-muted)]'
}

function StatusTabContent({ links, onRefresh }: StatusTabProps) {
  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)
  const [showAll, setShowAll] = useState(false)
  const [viewingName, setViewingName] = useState<string | null>(null)
  const [viewData, setViewData] = useState<{ name: string; status: string } | null>(null)
  const [viewLoading, setViewLoading] = useState(false)
  const [viewErr, setViewErr] = useState('')

  const handleView = async (name: string) => {
    setViewingName(name)
    setViewLoading(true)
    setViewErr('')
    setViewData(null)
    try {
      setViewData(await api.getDeviceStatus(name))
    } catch (e: unknown) {
      setViewErr(extractErrorMessage(e))
    } finally {
      setViewLoading(false)
    }
  }

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    const list = [...links].sort((a, b) => a.name.localeCompare(b.name))
    if (!q) return list
    return list.filter(l => {
      const hay = [l.name, l.kind, l.operational_state, l.setup_state].join(' ').toLowerCase()
      return hay.includes(q)
    })
  }, [links, search])

  const pageItems = paginateSlice(filtered, page, DEFAULT_PAGE_SIZE, showAll)

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <div>
          <h2 className="text-xl font-semibold">Link status</h2>
          <p className="text-sm text-[var(--zf-muted)] mt-1">From networkctl or ip link on the host</p>
        </div>
        <button onClick={onRefresh} className="flex items-center gap-2 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] text-[var(--zf-ink)] py-2 px-3 rounded-lg transition text-sm">
          <RefreshCw className="w-4 h-4" /> Refresh
        </button>
      </div>
      {links.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No link data available.</div>
      ) : (
        <>
          <ListControls
            search={search}
            onSearchChange={setSearch}
            searchPlaceholder="Search name, type, state…"
            total={links.length}
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
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Index</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Type</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Operational</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Setup</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {pageItems.map(l => (
                  <tr
                    key={l.index}
                    className="hover:bg-white/[0.03] transition cursor-pointer"
                    onClick={() => handleView(l.name)}
                    title="Click for device status detail"
                  >
                    <td className="p-4 font-mono text-sm">{l.index}</td>
                    <td className="p-4 font-medium">{l.name}</td>
                    <td className="p-4 text-[var(--zf-muted)]">{l.kind}</td>
                    <td className="p-4">
                      <span className={`px-2 py-1 rounded text-xs font-medium ${operStateClasses(l.operational_state)}`}>
                        {l.operational_state}
                      </span>
                    </td>
                    <td className="p-4">
                      <span className={`px-2 py-1 rounded text-xs font-medium ${
                        l.setup_state === 'configured' ? 'bg-green-500/10 text-emerald-600' :
                        l.setup_state === 'configuring' ? 'bg-yellow-500/10 text-amber-600' :
                        'bg-black/[0.04] text-[var(--zf-muted)]'
                      }`}>{l.setup_state || '-'}</span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {filtered.length === 0 && (
              <div className="p-8 text-center text-[var(--zf-muted)] text-sm">No links match your search.</div>
            )}
          </div>
        </>
      )}
      {viewingName && (
        <DetailModal
          title={`Device Status: ${viewingName}`}
          data={viewData as unknown as Record<string, unknown> | null}
          loading={viewLoading}
          error={viewErr}
          onClose={() => setViewingName(null)}
        />
      )}
    </div>
  )
}

export default StatusTabContent
