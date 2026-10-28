// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PanelLoading } from '../components/Skeleton'
import { useState, useEffect, useMemo } from 'react'
import { Search, HardDrive } from 'lucide-react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, Card, CardBody } from '../components/ui'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface DiskImage { name: string; path: string; format: string; size_bytes: number; mod_time?: string }

function formatBytes(bytes: number): string {
  if (!bytes) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`
}

const formatBadge = 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'

export default function DiskImages() {
  const toast = useToastContext()
  const [images, setImages] = useState<DiskImage[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [search, setSearch] = useState('')
  const [selected, setSelected] = useState<Set<string>>(new Set())

  const fetchImages = async () => {
    setLoading(true); setError(null)
    try {
      const res = await apiFetch('/api/images')
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const data = await res.json()
      setImages(Array.isArray(data) ? data : data.images || data.disk_images || [])
    } catch (err: unknown) {
      const msg = formatUserError(err)
      setError(msg)
      toastFailure(toast, 'Failed to load disk images', err)
    } finally { setLoading(false) }
  }

  useEffect(() => { fetchImages() }, [])

  const filtered = useMemo(() => {
    if (!search) return images
    const q = search.toLowerCase()
    return images.filter(img => img.name.toLowerCase().includes(q) || img.format.toLowerCase().includes(q) || img.path.toLowerCase().includes(q))
  }, [images, search])

  const totalSize = images.reduce((sum, img) => sum + (img.size_bytes || 0), 0)
  const toggleSelect = (path: string) => setSelected(prev => { const next = new Set(prev); if (next.has(path)) next.delete(path); else next.add(path); return next })

  if (loading) return <PanelLoading label="Loading disk images" />

  return (
    <div className="space-y-6">
      <PageHeader
        title="Disk Images"
        description="Browse and manage VM disk images"
        onRefresh={fetchImages}
        refreshing={loading}
      />

      {error && (
        <ErrorBanner
          title="Could not load disk images"
          headline={error}
          hints={hintsForError(error, 'storage')}
          onRetry={fetchImages}
        />
      )}

      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
        <Card><CardBody className="px-4 py-3"><div className="text-2xl font-bold text-[var(--zf-ink)]">{images.length}</div><div className="text-xs text-[var(--zf-muted)] mt-1">Total Images</div></CardBody></Card>
        <Card><CardBody className="px-4 py-3"><div className="text-2xl font-bold text-[var(--zf-ink)]">{formatBytes(totalSize)}</div><div className="text-xs text-[var(--zf-muted)] mt-1">Total Size</div></CardBody></Card>
        <Card><CardBody className="px-4 py-3"><div className="text-2xl font-bold text-[var(--zf-ink)]">{new Set(images.map(i => i.format)).size}</div><div className="text-xs text-[var(--zf-muted)] mt-1">Formats</div></CardBody></Card>
        <Card><CardBody className="px-4 py-3"><div className="text-2xl font-bold text-[var(--zf-ink)]">{selected.size}</div><div className="text-xs text-[var(--zf-muted)] mt-1">Selected</div></CardBody></Card>
      </div>

      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[var(--zf-muted)]" />
        <input type="text" value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Search by name, format, or path..." aria-label="Search disk images"
          className="input-field pl-10" />
      </div>

      {filtered.length === 0 ? (
        <Card><CardBody className="p-10 text-center text-[var(--zf-muted)]"><HardDrive className="w-10 h-10 mx-auto mb-3 opacity-50" /><p className="text-sm">{images.length === 0 ? 'No disk images found' : 'No images match your search'}</p></CardBody></Card>
      ) : (
        <Card className="overflow-hidden">
          <table className="w-full text-sm">
            <thead><tr className="border-b border-[var(--zf-hairline)]">
              <th className="px-4 py-3 text-left w-10"><span className="sr-only">Select</span></th>
              <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">Name</th>
              <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">Format</th>
              <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">Size</th>
              <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">Path</th>
            </tr></thead>
            <tbody>
              {filtered.map(img => (
                <tr key={img.path} onClick={() => toggleSelect(img.path)} className={`border-b border-[var(--zf-hairline)]/60 cursor-pointer transition-colors ${selected.has(img.path) ? 'bg-[var(--zf-link)]/10' : 'hover:bg-black/[0.04]'}`}>
                  <td className="px-4 py-3"><input type="checkbox" checked={selected.has(img.path)} onChange={() => toggleSelect(img.path)} className="rounded border-[var(--zf-hairline)]" /></td>
                  <td className="px-4 py-3 text-[var(--zf-ink)] font-medium">{img.name}</td>
                  <td className="px-4 py-3"><span className={`px-2 py-0.5 rounded-full text-xs font-medium border ${formatBadge}`}>{img.format}</span></td>
                  <td className="px-4 py-3 text-[var(--zf-ink)] font-mono text-xs">{formatBytes(img.size_bytes)}</td>
                  <td className="px-4 py-3 text-[var(--zf-muted)] text-xs font-mono truncate max-w-[250px]" title={img.path}>{img.path}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
    </div>
  )
}
