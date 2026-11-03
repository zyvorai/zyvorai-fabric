// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect } from 'react'
import { Star, Search, Monitor, Cpu, HardDrive } from 'lucide-react'
import { Link } from 'react-router'
import { listVMs } from '../api/vm'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, StatusBadge } from '../components/ui'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface FavoriteVM { name: string; added_at: string }

const STORAGE_KEY = 'zyvor_fabric_favorites'
const LEGACY_STORAGE_KEY = 'vmspawnd_favorites'

function loadFavorites(): FavoriteVM[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY) || localStorage.getItem(LEGACY_STORAGE_KEY) || '[]'
    const favs = JSON.parse(raw)
    if (!localStorage.getItem(STORAGE_KEY) && localStorage.getItem(LEGACY_STORAGE_KEY)) {
      localStorage.setItem(STORAGE_KEY, raw)
      localStorage.removeItem(LEGACY_STORAGE_KEY)
    }
    return favs
  } catch {
    return []
  }
}
function saveFavorites(favs: FavoriteVM[]) { localStorage.setItem(STORAGE_KEY, JSON.stringify(favs)) }

export default function FavoriteVMs() {
  const toast = useToastContext()
  const [vms, setVMs] = useState<any[]>([])
  const [favorites, setFavorites] = useState<FavoriteVM[]>(loadFavorites)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [search, setSearch] = useState('')

  const loadVMs = async () => {
    setLoading(true); setError('')
    try {
      setVMs(await listVMs())
    } catch (e: unknown) {
      const msg = formatUserError(e)
      setError(msg)
      toastFailure(toast, 'Failed to load VMs', e)
    } finally { setLoading(false) }
  }

  useEffect(() => { loadVMs() }, [])

  const isFavorite = (name: string) => favorites.some(f => f.name === name)
  const toggleFavorite = (vm: any) => {
    let updated: FavoriteVM[]
    if (isFavorite(vm.name)) { updated = favorites.filter(f => f.name !== vm.name) }
    else { updated = [...favorites, { name: vm.name, added_at: new Date().toISOString() }] }
    setFavorites(updated); saveFavorites(updated)
  }

  const matchesSearch = (vm: any) => !search || vm.name?.toLowerCase().includes(search.toLowerCase()) || (vm.state || '').toLowerCase().includes(search.toLowerCase())
  const pinnedVMs = favorites.map(fav => vms.find(vm => vm.name === fav.name)).filter((vm): vm is any => vm != null && matchesSearch(vm))
  const otherVMs = vms.filter(vm => !isFavorite(vm.name) && matchesSearch(vm))

  const renderVMRow = (vm: any) => {
    const starred = isFavorite(vm.name)
    return (
      <div key={vm.name} className="flex items-center gap-3 p-3 rounded-xl border bg-[var(--zf-canvas)] border-[var(--zf-hairline)] hover:bg-[var(--zf-surface)] hover:border-[var(--zf-hairline)] transition-all">
        <button onClick={() => toggleFavorite(vm)} className="flex-shrink-0 transition-colors" title={starred ? 'Remove from favorites' : 'Add to favorites'}>
          <Star className={`w-5 h-5 ${starred ? 'text-[var(--zf-warning)] fill-[var(--zf-warning)]' : 'text-[var(--zf-muted)] hover:text-[var(--zf-warning)]/60'}`} />
        </button>
        <div className="flex-1 min-w-0">
          <Link to={`/app/vms/${vm.name}`} className="text-sm font-medium text-[var(--zf-ink)] truncate hover:text-[var(--zf-link)] transition-colors">{vm.name}</Link>
          <div className="flex items-center gap-3 text-xs text-[var(--zf-muted)] mt-1">
            <span className="flex items-center gap-1"><Cpu className="h-3 w-3" />{vm.cpus || 0} vCPU</span>
            <span className="flex items-center gap-1"><HardDrive className="h-3 w-3" />{vm.memory ? (vm.memory > 1024 * 1024 ? (vm.memory / 1024 / 1024 / 1024).toFixed(1) : (vm.memory / 1024).toFixed(1)) : '0'} GB</span>
          </div>
        </div>
        <StatusBadge status={vm.state || 'unknown'} />
        <Link to={`/app/vms/${vm.name}/console`} className="zf-btn zf-btn-ghost zf-btn-sm flex-shrink-0">
          <Monitor className="w-3.5 h-3.5" /> Console
        </Link>
      </div>
    )
  }

  return (
    <div>
      <PageHeader
        title="Favorites"
        description={loading ? 'Loading...' : `${favorites.length} pinned, ${vms.length} total VMs`}
        onRefresh={loadVMs}
        refreshing={loading}
      />

      <div className="relative mb-4">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-[var(--zf-muted)]" />
        <input type="text" placeholder="Search VMs..." aria-label="Search VMs" value={search} onChange={(e) => setSearch(e.target.value)}
          className="w-full pl-10 pr-4 py-2.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--zf-link)] text-sm" />
      </div>

      {error && (
        <ErrorBanner
          title="Could not load VMs"
          headline={error}
          hints={hintsForError(error, 'vm')}
          onRetry={loadVMs}
        />
      )}

      {loading ? (
        <div className="space-y-3">{[1, 2, 3].map((i) => (<div key={i} className="h-16 rounded-xl bg-[var(--zf-canvas)] animate-pulse" />))}</div>
      ) : (
        <>
          <div className="mb-6">
            <h3 className="text-sm font-semibold text-[var(--zf-warning)] flex items-center gap-2 mb-3"><Star className="w-4 h-4 fill-[var(--zf-warning)]" />Pinned VMs</h3>
            {pinnedVMs.length === 0 ? (
              <div className="text-center py-8 rounded-xl border border-dashed border-[var(--zf-hairline)] bg-[var(--zf-canvas)]"><Star className="w-8 h-8 text-[var(--zf-muted)] mx-auto mb-2" /><p className="text-sm text-[var(--zf-muted)]">No favorites yet -- star VMs for quick access</p></div>
            ) : (<div className="space-y-2">{pinnedVMs.map(renderVMRow)}</div>)}
          </div>
          <div>
            <h3 className="text-sm font-semibold text-[var(--zf-muted)] flex items-center gap-2 mb-3"><Monitor className="w-4 h-4" />All VMs</h3>
            {otherVMs.length === 0 ? (
              <div className="text-center py-8 text-sm text-[var(--zf-muted)]">{vms.length === 0 ? 'No VMs found.' : 'All VMs are pinned or no matches.'}</div>
            ) : (<div className="space-y-2">{otherVMs.map(renderVMRow)}</div>)}
          </div>
        </>
      )}
    </div>
  )
}
