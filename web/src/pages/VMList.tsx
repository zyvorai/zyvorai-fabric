// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState, useMemo, useCallback, type MouseEvent } from 'react'
import { Link, useNavigate } from 'react-router'
import { listVMs, startVM, stopVM, deleteVM, VM } from '../api/vm'
import { createBackup } from '../api/backup'
import { Search, X, Tag, Layers, Monitor, LayoutGrid, List, Play, Square, Pause, Terminal, MoreVertical, Cpu, HardDrive, CheckSquare, Trash2, Archive } from 'lucide-react'
import VMCard from '../components/VMCard'
import { getTagColor } from '../components/TagEditor'
import { PageHeader, EmptyState, StatusBadge } from '../components/ui'
import ErrorBanner from '../components/ErrorBanner'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { SkeletonCard } from '../components/Skeleton'
import { useVMActions } from '../hooks/useVMActions'
import { useToastContext } from '../contexts/ToastContext'
import ReadOnlyNotice from '../components/ReadOnlyNotice'
import { usePermissions } from '../hooks/usePermissions'
import ConfirmDialog from '../components/ConfirmDialog'
import CopyButton from '../components/CopyButton'

type ViewMode = 'grid' | 'table'

function VMTableRow({ vm, onUpdate, selected, onSelect, canWrite }: { vm: VM; onUpdate: () => void; selected: boolean; onSelect: (name: string) => void; canWrite: boolean }) {
  const { handleStart, handleStop, handlePause, handleResume, handleBackup } = useVMActions(vm.name, onUpdate)
  const navigate = useNavigate()

  // Double-clicking a row jumps straight to the console -- matches the grid
  // card's same behavior, so it doesn't matter which view someone's in.
  const handleRowDoubleClick = (e: MouseEvent) => {
    if ((e.target as HTMLElement).closest('a, button, input')) return
    navigate(`/app/vms/${vm.name}/console`)
  }

  return (
    <tr
      onDoubleClick={handleRowDoubleClick}
      title="Double-click to open console"
      className={`border-t border-[var(--zf-hairline)] hover:bg-black/[0.02] transition-colors group ${selected ? 'bg-[var(--zf-link)]/5' : ''}`}
    >
      <td className="py-3 px-4 w-10">
        <input
          type="checkbox"
          checked={selected}
          onChange={() => onSelect(vm.name)}
          className="w-3.5 h-3.5 rounded border-[var(--zf-hairline)] cursor-pointer"
        />
      </td>
      <td className="py-3 px-4">
        <div className="flex items-center gap-1 group/name">
          <Link
            to={`/app/vms/${vm.name}`}
            className="font-medium text-[var(--zf-ink)] hover:text-[var(--zf-link)] transition-colors"
          >
            {vm.name}
          </Link>
          <CopyButton text={vm.name} iconOnly className="opacity-0 group-hover/name:opacity-100" successMessage="VM name copied" />
        </div>
      </td>
      <td className="py-3 px-4">
        <StatusBadge status={vm.state} title={vm.state === 'failed' ? vm.last_error : undefined} />
      </td>
      <td className="py-3 px-4">
        <div className="flex items-center gap-1.5 text-sm text-[var(--zf-muted)]">
          <Cpu className="w-3.5 h-3.5 text-[var(--zf-muted)]" />
          {vm.cpus}
        </div>
      </td>
      <td className="py-3 px-4">
        <div className="flex items-center gap-1.5 text-sm text-[var(--zf-muted)]">
          <HardDrive className="w-3.5 h-3.5 text-[var(--zf-muted)]" />
          {vm.memory >= 1024 ? `${(vm.memory / 1024).toFixed(1)} GB` : `${vm.memory} MB`}
        </div>
      </td>
      <td className="py-3 px-4">
        <span className="text-sm text-[var(--zf-muted)] truncate block max-w-[200px]">{vm.image}</span>
      </td>
      <td className="py-3 px-4">
        {vm.tags && vm.tags.length > 0 ? (
          <div className="flex flex-wrap gap-1">
            {vm.tags.slice(0, 2).map((tag) => (
              <span
                key={tag}
                className={`px-1.5 py-0.5 rounded text-xs font-medium ${getTagColor(tag)} text-[var(--zf-ink)]/90`}
              >
                {tag}
              </span>
            ))}
            {vm.tags.length > 2 && (
              <span className="text-xs text-[var(--zf-muted)]">+{vm.tags.length - 2}</span>
            )}
          </div>
        ) : (
          <span className="text-[var(--zf-muted)] text-sm">--</span>
        )}
      </td>
      <td className="py-3 px-4">
        {canWrite ? (
        <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
          {vm.state === 'stopped' || vm.state === 'failed' ? (
            <button onClick={handleStart} className="p-1.5 rounded-md text-emerald-700 hover:bg-emerald-50 transition-colors" title="Start">
              <Play className="w-3.5 h-3.5" />
            </button>
          ) : vm.state === 'paused' ? (
            <button onClick={handleResume} className="p-1.5 rounded-md text-emerald-700 hover:bg-emerald-50 transition-colors" title="Resume">
              <Play className="w-3.5 h-3.5" />
            </button>
          ) : (
            <>
              <button onClick={handleStop} className="p-1.5 rounded-md text-[var(--zf-danger)] hover:bg-red-50 transition-colors" title="Stop">
                <Square className="w-3.5 h-3.5" />
              </button>
              <button onClick={handlePause} className="p-1.5 rounded-md text-amber-700 hover:bg-amber-50 transition-colors" title="Pause">
                <Pause className="w-3.5 h-3.5" />
              </button>
            </>
          )}
          <button onClick={handleBackup} className="p-1.5 rounded-md text-[var(--zf-link)] hover:bg-[var(--zf-canvas)] transition-colors" title="Backup">
            <Archive className="w-3.5 h-3.5" />
          </button>
          <Link to={`/app/vms/${vm.name}/console`} className="p-1.5 rounded-md text-[var(--zf-muted)] hover:text-[var(--zf-link)] hover:bg-[var(--zf-canvas)] transition-colors" title="Console">
            <Terminal className="w-3.5 h-3.5" />
          </Link>
          <Link to={`/app/vms/${vm.name}`} className="p-1.5 rounded-md text-[var(--zf-muted)] hover:text-[var(--zf-ink)] hover:bg-black/[0.04] transition-colors" title="Details">
            <MoreVertical className="w-3.5 h-3.5" />
          </Link>
        </div>
        ) : (
          <Link to={`/app/vms/${vm.name}`} className="text-xs text-[var(--zf-link)] hover:text-[var(--zf-link-hover)]">View</Link>
        )}
      </td>
    </tr>
  )
}

export default function VMList() {
  const [vms, setVMs] = useState<VM[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [refreshError, setRefreshError] = useState<string | null>(null)
  const [searchQuery, setSearchQuery] = useState('')
  const [selectedTags, setSelectedTags] = useState<string[]>([])
  const [groupByTags, setGroupByTags] = useState(false)
  const [selectedVMs, setSelectedVMs] = useState<Set<string>>(new Set())
  const [bulkAction, setBulkAction] = useState<'delete' | null>(null)
  const [bulkLoading, setBulkLoading] = useState(false)
  const toast = useToastContext()
  const { canWrite } = usePermissions()
  const [viewMode, setViewMode] = useState<ViewMode>(() => {
    return (localStorage.getItem('vm-view-mode') as ViewMode) || 'grid'
  })

  useEffect(() => { loadVMs() }, [])
  useEffect(() => { localStorage.setItem('vm-view-mode', viewMode) }, [viewMode])

  const loadVMs = async () => {
    try {
      const data = await listVMs()
      setVMs(data)
      setLoadError(null)
      setRefreshError(null)
    } catch (error) {
      const msg = formatUserError(error)
      setVMs((prev) => {
        if (prev.length === 0) {
          setLoadError(msg)
          toastFailure(toast, 'Failed to load virtual machines', error)
        } else {
          setRefreshError(msg)
        }
        return prev
      })
    } finally {
      setLoading(false)
    }
  }

  const allTags = useMemo(
    () => Array.from(new Set(vms.flatMap((vm) => vm.tags || []))).sort(),
    [vms]
  )

  const filteredVMs = useMemo(() => {
    return vms.filter((vm) => {
      const q = searchQuery.toLowerCase()
      const matchesSearch =
        !q ||
        vm.name.toLowerCase().includes(q) ||
        vm.image.toLowerCase().includes(q) ||
        vm.state.toLowerCase().includes(q) ||
        (vm.tags && vm.tags.some(tag => tag.toLowerCase().includes(q)))
      const matchesTags =
        selectedTags.length === 0 ||
        (vm.tags && selectedTags.every(tag => vm.tags!.includes(tag)))
      return matchesSearch && matchesTags
    })
  }, [vms, searchQuery, selectedTags])

  const groupedVMs = useMemo(() => {
    if (!groupByTags) return {}
    const groups: Record<string, VM[]> = {}
    filteredVMs.forEach((vm) => {
      if (vm.tags && vm.tags.length > 0) {
        vm.tags.forEach((tag) => {
          if (!groups[tag]) groups[tag] = []
          groups[tag].push(vm)
        })
      } else {
        if (!groups['Untagged']) groups['Untagged'] = []
        groups['Untagged'].push(vm)
      }
    })
    return groups
  }, [filteredVMs, groupByTags])

  const toggleTag = (tag: string) => {
    setSelectedTags((prev) =>
      prev.includes(tag) ? prev.filter((t) => t !== tag) : [...prev, tag]
    )
  }

  const toggleSelect = useCallback((name: string) => {
    setSelectedVMs((prev) => {
      const next = new Set(prev)
      if (next.has(name)) next.delete(name)
      else next.add(name)
      return next
    })
  }, [])

  const selectAll = () => {
    if (selectedVMs.size === filteredVMs.length) {
      setSelectedVMs(new Set())
    } else {
      setSelectedVMs(new Set(filteredVMs.map((vm) => vm.name)))
    }
  }

  const clearSelection = () => setSelectedVMs(new Set())

  const bulkStart = async () => {
    setBulkLoading(true)
    const stopped = vms.filter((vm) => selectedVMs.has(vm.name) && vm.state === 'stopped')
    const results = await Promise.allSettled(stopped.map((vm) => startVM(vm.name)))
    const ok = results.filter((r) => r.status === 'fulfilled').length
    toast.success(`Started ${ok} VM${ok !== 1 ? 's' : ''}`)
    clearSelection()
    setBulkLoading(false)
    loadVMs()
  }

  const bulkStop = async () => {
    setBulkLoading(true)
    const running = vms.filter((vm) => selectedVMs.has(vm.name) && vm.state === 'running')
    const results = await Promise.allSettled(running.map((vm) => stopVM(vm.name)))
    const ok = results.filter((r) => r.status === 'fulfilled').length
    toast.success(`Stopped ${ok} VM${ok !== 1 ? 's' : ''}`)
    clearSelection()
    setBulkLoading(false)
    loadVMs()
  }

  const bulkBackup = async () => {
    setBulkLoading(true)
    const names = Array.from(selectedVMs)
    const results = await Promise.allSettled(names.map((n) => createBackup({ vm_name: n, backup_type: 'full' })))
    const ok = results.filter((r) => r.status === 'fulfilled').length
    toast.success(`Backup started for ${ok} VM${ok !== 1 ? 's' : ''}`)
    clearSelection()
    setBulkLoading(false)
  }

  const bulkDelete = async () => {
    setBulkAction(null)
    setBulkLoading(true)
    const names = Array.from(selectedVMs)
    const results = await Promise.allSettled(names.map((n) => deleteVM(n)))
    const ok = results.filter((r) => r.status === 'fulfilled').length
    toast.success(`Deleted ${ok} VM${ok !== 1 ? 's' : ''}`)
    clearSelection()
    setBulkLoading(false)
    loadVMs()
  }

  const selectedCount = selectedVMs.size
  const selectedRunning = vms.filter((vm) => selectedVMs.has(vm.name) && vm.state === 'running').length
  const selectedStopped = vms.filter((vm) => selectedVMs.has(vm.name) && vm.state === 'stopped').length

  const statCounts = useMemo(() => ({
    total: vms.length,
    running: vms.filter((vm) => vm.state === 'running').length,
    stopped: vms.filter((vm) => vm.state === 'stopped').length,
    paused: vms.filter((vm) => vm.state === 'paused').length,
  }), [vms])

  if (loading) {
    return (
      <div>
        <PageHeader title="Virtual Machines" />
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {Array.from({ length: 6 }).map((_, i) => (<SkeletonCard key={i} />))}
        </div>
      </div>
    )
  }

  return (
    <div>
      {loadError && (
        <ErrorBanner
          title="Could not load virtual machines"
          headline={loadError}
          hints={hintsForError(loadError, 'vm')}
          onRetry={() => { setLoading(true); void loadVMs() }}
          tone="red"
        />
      )}
      {!loadError && refreshError && (
        <ErrorBanner
          title="Could not refresh virtual machines"
          headline={refreshError}
          onRetry={() => void loadVMs()}
          tone="amber"
        />
      )}
      {!canWrite && <ReadOnlyNotice />}
      <PageHeader
        title="Virtual Machines"
        onRefresh={() => { setLoading(true); void loadVMs() }}
        refreshing={loading}
        actions={
          <div className="flex items-center gap-2">
            <div className="flex bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg p-0.5">
              <button
                onClick={() => setViewMode('grid')}
                className={`p-1.5 rounded-md transition-colors ${viewMode === 'grid' ? 'bg-[var(--zf-surface)] text-[var(--zf-ink)]' : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'}`}
                title="Grid view"
              >
                <LayoutGrid className="w-4 h-4" />
              </button>
              <button
                onClick={() => setViewMode('table')}
                className={`p-1.5 rounded-md transition-colors ${viewMode === 'table' ? 'bg-[var(--zf-surface)] text-[var(--zf-ink)]' : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'}`}
                title="Table view"
              >
                <List className="w-4 h-4" />
              </button>
            </div>
            <button
              onClick={() => setGroupByTags(!groupByTags)}
              className={`flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm transition ${
                groupByTags ? 'bg-[var(--zf-link)]/20 text-[var(--zf-link)] border border-[var(--zf-link)]/30' : 'bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
              }`}
            >
              <Layers className="w-3.5 h-3.5" />
              Group
            </button>
          </div>
        }
      />

      {/* Stats bar */}
      {vms.length > 0 && (
        <div className="flex items-center gap-4 mb-5 text-sm">
          <span className="text-[var(--zf-muted)]">
            {filteredVMs.length === vms.length ? `${vms.length} VMs` : `${filteredVMs.length} of ${vms.length} VMs`}
          </span>
          <div className="h-3 w-px bg-[var(--zf-hairline)]" />
          <span className="flex items-center gap-1.5">
            <span className="w-2 h-2 rounded-full bg-[var(--zf-success)]" />
            <span className="text-[var(--zf-muted)]">{statCounts.running} running</span>
          </span>
          <span className="flex items-center gap-1.5">
            <span className="w-2 h-2 rounded-full bg-[var(--zf-danger)]" />
            <span className="text-[var(--zf-muted)]">{statCounts.stopped} stopped</span>
          </span>
          {statCounts.paused > 0 && (
            <span className="flex items-center gap-1.5">
              <span className="w-2 h-2 rounded-full bg-[var(--zf-warning)]" />
              <span className="text-[var(--zf-muted)]">{statCounts.paused} paused</span>
            </span>
          )}
        </div>
      )}

      {/* Search + Select All */}
      {vms.length > 0 && (
        <div className="flex items-center gap-3 mb-4">
          <div className="relative max-w-md flex-1">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[var(--zf-muted)]" />
            <input
              type="text"
              placeholder="Search VMs..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg py-2 pl-9 pr-8 text-sm text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:border-[var(--zf-ink)] transition-colors"
            />
            {searchQuery && (
              <button onClick={() => setSearchQuery('')} className="absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition-colors">
                <X className="w-3.5 h-3.5" />
              </button>
            )}
          </div>
          {filteredVMs.length > 1 && (
            <button
              onClick={selectAll}
              className={`flex items-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium transition-colors ${
                selectedVMs.size === filteredVMs.length
                  ? 'bg-[var(--zf-link)]/20 text-[var(--zf-link)] border border-[var(--zf-link)]/30'
                  : 'bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
              }`}
            >
              <CheckSquare className="w-3.5 h-3.5" />
              {selectedVMs.size === filteredVMs.length ? 'Deselect All' : 'Select All'}
            </button>
          )}
        </div>
      )}

      {/* Tag Filter */}
      {allTags.length > 0 && (
        <div className="mb-5">
          <div className="flex items-center gap-2 flex-wrap">
            <Tag className="w-3.5 h-3.5 text-[var(--zf-muted)]" />
            {allTags.map((tag) => {
              const isSelected = selectedTags.includes(tag)
              const vmCount = vms.filter((vm) => vm.tags?.includes(tag)).length
              return (
                <button
                  key={tag}
                  onClick={() => toggleTag(tag)}
                  className={`px-2.5 py-1 rounded-md text-xs font-medium transition ${
                    isSelected
                      ? getTagColor(tag) + ' text-[var(--zf-ink)]/90'
                      : 'bg-[var(--zf-surface)] text-[var(--zf-muted)] hover:bg-black/[0.04] hover:text-[var(--zf-ink)] border border-[var(--zf-hairline)]'
                  }`}
                >
                  {tag}<span className={isSelected ? 'ml-1 text-[var(--zf-ink)]/60' : 'ml-1 text-[var(--zf-muted)]'}>{vmCount}</span>
                </button>
              )
            })}
            {selectedTags.length > 0 && (
              <button onClick={() => setSelectedTags([])} className="text-xs text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] ml-1 transition-colors">Clear</button>
            )}
          </div>
        </div>
      )}

      {/* Content */}
      {vms.length === 0 ? (
        <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)]">
          <EmptyState
            icon={<Monitor className="w-16 h-16" />}
            title="No virtual machines"
            description="Create your first virtual machine to get started"
            action={
              <Link to="/app/create" className="zf-btn zf-btn-primary mt-2">
                Create VM
              </Link>
            }
          />
        </div>
      ) : filteredVMs.length === 0 ? (
        <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)]">
          <EmptyState icon={<Search className="w-12 h-12" />} title="No matching VMs" description="Try adjusting your search or filters" />
        </div>
      ) : groupByTags ? (
        <div className="space-y-8">
          {Object.entries(groupedVMs).map(([tag, vmsInGroup]) => (
            <div key={tag}>
              <div className="flex items-center gap-3 mb-3">
                <span className={`px-3 py-1.5 rounded-md text-sm font-medium ${getTagColor(tag)} text-[var(--zf-ink)]/90`}>{tag}</span>
                <span className="text-sm text-[var(--zf-muted)]">{vmsInGroup.length} {vmsInGroup.length === 1 ? 'VM' : 'VMs'}</span>
              </div>
              {viewMode === 'grid' ? (
                <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
                  {vmsInGroup.map((vm) => (<VMCard key={`${tag}-${vm.name}`} vm={vm} onUpdate={loadVMs} />))}
                </div>
              ) : (
                <VMTable vms={vmsInGroup} onUpdate={loadVMs} selectedVMs={selectedVMs} onSelect={toggleSelect} canWrite={canWrite} />
              )}
            </div>
          ))}
        </div>
      ) : viewMode === 'grid' ? (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
          {filteredVMs.map((vm) => (<VMCard key={vm.name} vm={vm} onUpdate={loadVMs} />))}
        </div>
      ) : (
        <VMTable vms={filteredVMs} onUpdate={loadVMs} selectedVMs={selectedVMs} onSelect={toggleSelect} canWrite={canWrite} />
      )}

      {/* Bulk Action Bar */}
      {canWrite && selectedCount > 0 && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-40 animate-slide-in">
          <div className="flex items-center gap-3 bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-xl px-5 py-3">
            <span className="text-sm font-medium text-[var(--zf-ink)] tabular-nums">{selectedCount} selected</span>
            <div className="w-px h-5 bg-[var(--zf-hairline)]" />
            {selectedStopped > 0 && (
              <button
                onClick={bulkStart}
                disabled={bulkLoading}
                className="flex items-center gap-1.5 px-3 py-1.5 text-emerald-700 bg-emerald-50 hover:bg-emerald-100 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
              >
                <Play className="w-3.5 h-3.5" />
                Start {selectedStopped}
              </button>
            )}
            {selectedRunning > 0 && (
              <button
                onClick={bulkStop}
                disabled={bulkLoading}
                className="flex items-center gap-1.5 px-3 py-1.5 text-red-700 bg-red-50 hover:bg-red-100 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
              >
                <Square className="w-3.5 h-3.5" />
                Stop {selectedRunning}
              </button>
            )}
            <button
              onClick={bulkBackup}
              disabled={bulkLoading}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-link)]/15 text-[var(--zf-link)] hover:bg-[var(--zf-link)]/25 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
            >
              <Archive className="w-3.5 h-3.5" />
              Backup {selectedCount}
            </button>
            <button
              onClick={() => setBulkAction('delete')}
              disabled={bulkLoading}
              className="flex items-center gap-1.5 px-3 py-1.5 text-red-700 bg-red-50 hover:bg-red-100 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
            >
              <Trash2 className="w-3.5 h-3.5" />
              Delete
            </button>
            <div className="w-px h-5 bg-[var(--zf-hairline)]" />
            <button
              onClick={clearSelection}
              className="text-xs text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition-colors"
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {bulkAction === 'delete' && (
        <ConfirmDialog
          title={`Delete ${selectedCount} Virtual Machine${selectedCount !== 1 ? 's' : ''}`}
          message={`Are you sure you want to delete ${selectedCount} VM${selectedCount !== 1 ? 's' : ''}? This action cannot be undone.`}
          confirmLabel={`Delete ${selectedCount} VM${selectedCount !== 1 ? 's' : ''}`}
          variant="danger"
          onConfirm={bulkDelete}
          onCancel={() => setBulkAction(null)}
        />
      )}
    </div>
  )
}

function VMTable({ vms, onUpdate, selectedVMs, onSelect, canWrite }: { vms: VM[]; onUpdate: () => void; selectedVMs: Set<string>; onSelect: (name: string) => void; canWrite: boolean }) {
  return (
    <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
      <table className="w-full text-sm">
        <thead>
          <tr className="text-left text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">
            <th className="py-3 px-4 w-10" />
            <th className="py-3 px-4">Name</th>
            <th className="py-3 px-4">Status</th>
            <th className="py-3 px-4">CPU</th>
            <th className="py-3 px-4">Memory</th>
            <th className="py-3 px-4">Image</th>
            <th className="py-3 px-4">Tags</th>
            <th className="py-3 px-4 w-[140px]">Actions</th>
          </tr>
        </thead>
        <tbody>
          {vms.map((vm) => (
            <VMTableRow key={vm.name} vm={vm} onUpdate={onUpdate} selected={selectedVMs.has(vm.name)} onSelect={onSelect} canWrite={canWrite} />
          ))}
        </tbody>
      </table>
    </div>
  )
}
