// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useState, useEffect, useMemo, useCallback } from 'react'
import { Download, Search, HardDrive, ArrowUpDown, FolderSearch } from 'lucide-react'
import { apiFetch, getToken } from '../api/client'
import PageLoadBanner from '../components/PageLoadBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody } from '../utils/apiError'
import { usePageLoader } from '../hooks/usePageLoader'

interface DiskImage { name: string; path: string; format: string; size_bytes: number; mod_time: string }
type SortField = 'name' | 'size_bytes' | 'mod_time'
type SortDir = 'asc' | 'desc'

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`
}

const formatBadgeColor: Record<string, string> = {
  qcow2: 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
  vmdk: 'text-[var(--zf-ink)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
  vhd: 'text-[var(--zf-ink)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
  vhdx: 'text-[var(--zf-ink)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
  raw: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
  img: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
}

export default function DownloadDisk() {
  const [images, setImages] = useState<DiskImage[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load disk images')
  const [search, setSearch] = useState('')
  const [sortField, setSortField] = useState<SortField>('mod_time')
  const [sortDir, setSortDir] = useState<SortDir>('desc')
  const [customPath, setCustomPath] = useState('')

  const fetchImages = useCallback((extraPath?: string) => {
    return run(async () => {
      let url = '/api/images'
      if (extraPath) url += `?path=${encodeURIComponent(extraPath)}`
      const res = await apiFetch(url)
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const data = await res.json()
      setImages(Array.isArray(data) ? data : data.images || [])
    })
  }, [run])

  useEffect(() => {
    void fetchImages()
  }, [fetchImages])

  const handleSort = (field: SortField) => {
    if (sortField === field) setSortDir(sortDir === 'asc' ? 'desc' : 'asc')
    else { setSortField(field); setSortDir(field === 'name' ? 'asc' : 'desc') }
  }

  const filtered = useMemo(() => {
    let list = [...images]
    if (search) { const q = search.toLowerCase(); list = list.filter((img) => img.name.toLowerCase().includes(q) || img.format.toLowerCase().includes(q) || img.path.toLowerCase().includes(q)) }
    list.sort((a, b) => {
      const cmp =
        sortField === 'name'
          ? a.name.localeCompare(b.name)
          : sortField === 'size_bytes'
            ? a.size_bytes - b.size_bytes
            : new Date(a.mod_time).getTime() - new Date(b.mod_time).getTime()
      return sortDir === 'asc' ? cmp : -cmp
    })
    return list
  }, [images, search, sortField, sortDir])

  const totalSize = images.reduce((sum, img) => sum + img.size_bytes, 0)
  const handleDownload = (path: string) => {
    const token = getToken()
    const params = new URLSearchParams({ path })
    if (token) params.set('token', token)
    window.location.href = `/api/images/download?${params.toString()}`
  }

  const SortButton = ({ field, label }: { field: SortField; label: string }) => (
    <button onClick={() => handleSort(field)} className="flex items-center gap-1 text-xs font-medium text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition-colors">
      {label} <ArrowUpDown className={`w-3.5 h-3.5 ${sortField === field ? 'text-[var(--zf-link)]' : ''}`} />
    </button>
  )

  return (
    <div className="space-y-6">
      <PageHeader
        title="Download Disk"
        description="Browse and download VM disk images"
        onRefresh={() => fetchImages()}
      />

      <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
        <div className="zf-panel-muted px-4 py-3"><p className="text-xs text-[var(--zf-muted)] mb-1">Total Images</p><p className="text-2xl font-bold text-[var(--zf-ink)]">{images.length}</p></div>
        <div className="zf-panel-muted px-4 py-3"><p className="text-xs text-[var(--zf-muted)] mb-1">Total Size</p><p className="text-2xl font-bold text-[var(--zf-ink)]">{formatBytes(totalSize)}</p></div>
        <div className="zf-panel-muted px-4 py-3"><p className="text-xs text-[var(--zf-muted)] mb-1">Formats</p><p className="text-2xl font-bold text-[var(--zf-ink)]">{new Set(images.map((i) => i.format)).size}</p></div>
      </div>

      <div className="zf-panel-muted p-4">
        <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Custom Path</label>
        <div className="flex gap-2">
          <input type="text" value={customPath} onChange={(e) => setCustomPath(e.target.value)} placeholder="/path/to/disk-image.qcow2 or /path/to/directory/"
            className="input-field flex-1 text-sm"
            onKeyDown={(e) => { if (e.key === 'Enter') handleDownload(customPath.trim()) }} />
          <button onClick={() => fetchImages(customPath.trim())} className="zf-btn zf-btn-ghost">
            <FolderSearch className="w-4 h-4" /> Browse
          </button>
          <button onClick={() => handleDownload(customPath.trim())} disabled={!customPath.trim()}
            className="zf-btn zf-btn-primary">
            <Download className="w-4 h-4" /> Download
          </button>
        </div>
      </div>

      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[var(--zf-muted)]" />
        <input type="text" value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Filter by name, format, or path..." aria-label="Filter disk images"
          className="input-field w-full pl-10 pr-4 py-2.5 rounded-xl text-sm" />
      </div>

      <PageLoadBanner title="Could not load disk images" headline={loadError} onRetry={() => void fetchImages()} />
      {loading && <div className="flex items-center justify-center py-12"><div className="w-6 h-6 border-2 border-[var(--zf-link)] border-t-transparent rounded-full animate-spin" /></div>}

      {!loading && filtered.length === 0 && <div className="text-center py-12 text-[var(--zf-muted)]"><HardDrive className="w-10 h-10 mx-auto mb-3 opacity-50" /><p className="text-sm">No disk images found</p></div>}

      {!loading && filtered.length > 0 && (
        <div className="zf-panel overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead><tr className="border-b border-[var(--zf-hairline)]">
                <th className="text-left px-4 py-3"><SortButton field="name" label="Name" /></th>
                <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)]">Format</th>
                <th className="text-left px-4 py-3"><SortButton field="size_bytes" label="Size" /></th>
                <th className="text-left px-4 py-3"><SortButton field="mod_time" label="Modified" /></th>
                <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)]">Path</th>
                <th className="text-right px-4 py-3 text-xs font-medium text-[var(--zf-muted)]">Action</th>
              </tr></thead>
              <tbody>
                {filtered.map((img) => (
                  <tr key={img.path} className="border-b border-[var(--zf-hairline)] hover:bg-black/[0.04] transition-colors">
                    <td className="px-4 py-3 font-medium text-[var(--zf-ink)]">{img.name}</td>
                    <td className="px-4 py-3"><span className={`inline-block px-2 py-0.5 rounded-full text-xs font-medium border ${formatBadgeColor[img.format] || 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'}`}>{img.format}</span></td>
                    <td className="px-4 py-3 text-[var(--zf-ink)] font-mono text-xs">{formatBytes(img.size_bytes)}</td>
                    <td className="px-4 py-3 text-[var(--zf-muted)] text-xs">{new Date(img.mod_time).toLocaleString()}</td>
                    <td className="px-4 py-3 text-[var(--zf-muted)] text-xs font-mono max-w-[200px] truncate" title={img.path}>{img.path}</td>
                    <td className="px-4 py-3 text-right">
                      <button onClick={() => handleDownload(img.path)} className="zf-btn zf-btn-ghost zf-btn-sm">
                        <Download className="w-3.5 h-3.5" /> Download
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  )
}
