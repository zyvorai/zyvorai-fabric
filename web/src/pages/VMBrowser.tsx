// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Search, Monitor, Cpu, HardDrive } from 'lucide-react'
import { Link } from 'react-router'
import { listVMs, VM } from '../api/vm'
import PageLoadBanner from '../components/PageLoadBanner'
import { PageHeader } from '../components/ui'
import { usePageLoader } from '../hooks/usePageLoader'

function stateColor(state: string): string {
  const s = (state || '').toLowerCase()
  if (s === 'running') return 'text-emerald-700 bg-emerald-50 border-emerald-200'
  if (s === 'stopped' || s === 'shutoff') return 'text-red-700 bg-red-50 border-red-200'
  if (s === 'paused') return 'text-amber-800 bg-amber-50 border-amber-200'
  return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
}

/** `mib` is a VM's allocated memory in MiB (`VM.memory`'s actual unit). */
function fmtMem(mib: number): string {
  if (!mib) return '0'
  if (mib >= 1024) return `${(mib / 1024).toFixed(1)} GB`
  return `${mib.toFixed(0)} MB`
}

export default function VMBrowser() {
  const [vms, setVMs] = useState<VM[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load VMs')
  const [search, setSearch] = useState('')

  const loadVMs = useCallback(() => {
    return run(async () => {
      setVMs(await listVMs())
    })
  }, [run])

  useEffect(() => {
    void loadVMs()
  }, [loadVMs])

  const filtered = vms.filter(vm => {
    if (!search) return true
    const q = search.toLowerCase()
    return vm.name?.toLowerCase().includes(q) || (vm.state || '').toLowerCase().includes(q) || (vm.image || '').toLowerCase().includes(q)
  })

  const runningCount = vms.filter(v => (v.state || '').toLowerCase() === 'running').length

  return (
    <div className="space-y-6">
      <PageHeader
        title="VM Browser"
        description={loading ? 'Loading…' : `${vms.length} VMs, ${runningCount} running`}
        onRefresh={() => void loadVMs()}
        refreshing={loading}
      />
      <PageLoadBanner title="Could not load VMs" headline={loadError} onRetry={() => void loadVMs()} />

      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-[var(--zf-muted)]" />
        <input type="text" placeholder="Search VMs by name, state, or image..." aria-label="Search VMs" value={search} onChange={(e) => setSearch(e.target.value)}
          className="w-full pl-10 pr-4 py-2.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--zf-ink)]/10 focus:border-[var(--zf-ink)] text-sm" />
      </div>

      {loading && !loadError ? (
        <div className="space-y-3">{[1, 2, 3].map(i => <div key={i} className="h-20 rounded-xl bg-[var(--zf-canvas)] animate-pulse" />)}</div>
      ) : filtered.length === 0 ? (
        <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] text-center text-[var(--zf-muted)]"><Monitor className="w-10 h-10 mx-auto mb-3 opacity-50" /><p className="text-sm">{vms.length === 0 ? 'No VMs found' : 'No VMs match your search'}</p></div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filtered.map(vm => (
            <Link key={vm.name} to={`/app/vms/${vm.name}`} className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-4 transition-all hover:scale-[1.01] card-glow block">
              <div className="flex items-center justify-between mb-3">
                <span className="text-sm font-semibold text-[var(--zf-ink)] truncate">{vm.name}</span>
                <span className={`px-2.5 py-0.5 rounded-full text-xs font-medium border ${stateColor(vm.state)}`}>{vm.state || 'unknown'}</span>
              </div>
              {vm.image && <div className="text-xs text-[var(--zf-muted)] truncate mb-3 font-mono">{vm.image}</div>}
              <div className="flex items-center gap-4 text-xs text-[var(--zf-muted)]">
                <span className="flex items-center gap-1"><Cpu className="h-3 w-3" />{vm.cpus || 0} vCPU</span>
                <span className="flex items-center gap-1"><HardDrive className="h-3 w-3" />{fmtMem(vm.memory || 0)}</span>
                {vm.ip && <span className="font-mono">{vm.ip}</span>}
              </div>
            </Link>
          ))}
        </div>
      )}
    </div>
  )
}
