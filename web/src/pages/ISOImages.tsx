// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback, useMemo } from 'react'
import { Search, Disc } from 'lucide-react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, EmptyState, Card } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface ISOFile {
  name: string
  path: string
  size_bytes: number
  mod_time: string
}

interface VMWithISO {
  vm: string
  iso_path: string
}

function formatSize(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(i > 1 ? 1 : 0)} ${units[i]}`
}

function formatDate(dateStr: string): string {
  if (!dateStr) return '-'
  const d = new Date(dateStr)
  if (isNaN(d.getTime())) return dateStr
  return d.toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function isVirtioWin(name: string): boolean {
  return name.toLowerCase().includes('virtio-win')
}

export default function ISOImages() {
  const toast = useToastContext()
  const [isos, setISOs] = useState<ISOFile[]>([])
  const [vmsWithISOs, setVMsWithISOs] = useState<VMWithISO[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [search, setSearch] = useState('')

  const fetchISOs = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
      try {
        const resp = await apiFetch('/api/images/iso')
        if (!resp.ok) {
          const body = await resp.text()
          throw new Error(formatHttpErrorBody(resp.status, resp.statusText, body))
        }
        const data = await resp.json()
        const raw = Array.isArray(data) ? data : data.isos || []
        setISOs(
          raw.map((iso: { name: string; path?: string; size_bytes?: number; uploaded?: string }) => ({
            name: iso.name,
            path: iso.path || `/var/lib/zyvor-fabricd/iso/${iso.name}.iso`,
            size_bytes: iso.size_bytes ?? 0,
            mod_time: iso.uploaded || '',
          })),
        )
        setVMsWithISOs(data.vms_with_isos || [])
    } catch (err) {
      const msg = formatUserError(err)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load ISO images', err)
    } finally {
      setLoading(false)
    }
  }, [toast])

  useEffect(() => {
    fetchISOs()
    const interval = setInterval(fetchISOs, 30000)
    return () => clearInterval(interval)
  }, [fetchISOs])

  const filtered = useMemo(() => {
    if (!search) return isos
    const q = search.toLowerCase()
    return isos.filter(
      (iso) => iso.name.toLowerCase().includes(q) || iso.path.toLowerCase().includes(q),
    )
  }, [isos, search])

  const isoVMMap = useMemo(() => {
    const map: Record<string, string[]> = {}
    for (const entry of vmsWithISOs) {
      if (!map[entry.iso_path]) map[entry.iso_path] = []
      map[entry.iso_path].push(entry.vm)
    }
    return map
  }, [vmsWithISOs])

  return (
    <div className="space-y-6">
      <PageHeader
        title="ISO Images"
        description="Installer and virtio ISOs available on the host"
        onRefresh={fetchISOs}
        refreshing={loading}
      />

      {loadError && (
        <ErrorBanner
          title="Could not load ISO images"
          headline={loadError}
          hints={hintsForError(loadError, 'storage')}
          onRetry={fetchISOs}
        />
      )}

      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[var(--zf-muted)]" />
        <input
          type="text"
          placeholder="Search ISOs by name or path…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          aria-label="Search ISOs"
          className="input-field pl-10"
        />
      </div>

      {loading && !loadError ? (
        <Card>
          <div className="p-10 flex flex-col items-center justify-center text-[var(--zf-muted)] gap-3">
            <div className="w-6 h-6 border-2 border-[var(--zf-hairline)] border-t-[var(--zf-ink)] rounded-full animate-spin" />
            <span className="text-sm">Scanning for ISO images…</span>
          </div>
        </Card>
      ) : !loadError && filtered.length === 0 ? (
        <Card>
          <EmptyState
            icon={<Disc className="w-10 h-10" />}
            title={isos.length === 0 ? 'No ISO images found' : 'No ISOs match your search'}
            description="Place ISO files in the configured images directory on the host"
          />
        </Card>
      ) : !loadError ? (
        <Card className="overflow-hidden">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-[var(--zf-hairline)]">
                <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Name</th>
                <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Path</th>
                <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Size</th>
                <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Modified</th>
                <th className="text-left p-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Attached VMs</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]/30">
              {filtered.map((iso) => (
                <tr key={iso.path} className="hover:bg-black/[0.04] transition-colors">
                  <td className="p-3">
                    <div className="font-medium text-[var(--zf-ink)] flex items-center gap-2">
                      {iso.name}
                      {isVirtioWin(iso.name) && (
                        <span className="text-xs px-1.5 py-0.5 rounded text-[var(--zf-muted)] bg-[var(--zf-canvas)] border border-[var(--zf-hairline)]">
                          virtio-win
                        </span>
                      )}
                    </div>
                  </td>
                  <td className="p-3 text-[var(--zf-muted)] font-mono text-xs truncate max-w-xs" title={iso.path}>
                    {iso.path}
                  </td>
                  <td className="p-3 text-[var(--zf-ink)] whitespace-nowrap">{formatSize(iso.size_bytes)}</td>
                  <td className="p-3 text-[var(--zf-muted)] text-xs whitespace-nowrap">{formatDate(iso.mod_time)}</td>
                  <td className="p-3 text-[var(--zf-muted)] text-xs">
                    {(isoVMMap[iso.path] || []).join(', ') || '-'}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      ) : null}
    </div>
  )
}
