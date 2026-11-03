// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useMemo } from 'react'
import {
  Search,
  CheckSquare,
  Square,
  Power,
  Camera,
  Loader2,
  AlertTriangle,
  CheckCircle,
  Clock,
  RefreshCw,
  Monitor,
  MinusSquare,
  Play,
} from 'lucide-react'
import { apiFetch } from '../api/client'
import { listVMs } from '../api/vm'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, EmptyState } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface VM {
  name: string
  state: string
  cpus?: number
  memory?: number
}

type BulkAction = 'start' | 'stop' | 'restart' | 'snapshot'
type TaskStatus = 'pending' | 'running' | 'done' | 'error'

interface TaskProgress {
  vmName: string
  action: BulkAction
  status: TaskStatus
  message?: string
}

export default function BulkOperations() {
  const toast = useToastContext()
  const [vms, setVMs] = useState<VM[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [searchFilter, setSearchFilter] = useState('')
  const [tasks, setTasks] = useState<TaskProgress[]>([])
  const [running, setRunning] = useState(false)

  const fetchVMs = async () => {
    setLoading(true)
    setLoadError(null)
    try {
      setVMs(await listVMs())
    } catch (err) {
      const msg = formatUserError(err)
      setLoadError(msg)
      setVMs([])
      toastFailure(toast, 'Failed to load VMs', err)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    fetchVMs()
  }, [])

  const filtered = useMemo(() => {
    if (!searchFilter.trim()) return vms
    const q = searchFilter.toLowerCase()
    return vms.filter((vm) => vm.name.toLowerCase().includes(q))
  }, [vms, searchFilter])

  const toggleSelect = (name: string) => {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(name)) next.delete(name)
      else next.add(name)
      return next
    })
  }
  const selectAll = () => setSelected(new Set(filtered.map((vm) => vm.name)))
  const deselectAll = () => setSelected(new Set())
  const allFilteredSelected = filtered.length > 0 && filtered.every((vm) => selected.has(vm.name))
  const someSelected = selected.size > 0

  const runBulkAction = async (action: BulkAction) => {
    const targets = Array.from(selected)
    if (targets.length === 0) return
    setRunning(true)
    setTasks(targets.map((vmName) => ({ vmName, action, status: 'pending' })))

    for (const vmName of targets) {
      setTasks((prev) => prev.map((t) => (t.vmName === vmName ? { ...t, status: 'running' } : t)))
      try {
        let url = ''
        let body: string | undefined
        const method = 'POST'
        if (action === 'start') url = `/api/vms/${vmName}/start`
        else if (action === 'stop') url = `/api/vms/${vmName}/stop`
        else if (action === 'restart') url = `/api/vms/${vmName}/restart`
        else if (action === 'snapshot') {
          url = `/api/vms/${vmName}/snapshots`
          body = JSON.stringify({ name: `bulk-snap-${Date.now()}` })
        }

        const res = await apiFetch(url, {
          method,
          headers: body ? { 'Content-Type': 'application/json' } : undefined,
          body,
        })
        if (res.ok) {
          setTasks((prev) =>
            prev.map((t) => (t.vmName === vmName ? { ...t, status: 'done', message: 'Success' } : t)),
          )
        } else {
          const text = await res.text()
          const msg = formatHttpErrorBody(res.status, res.statusText, text)
          setTasks((prev) =>
            prev.map((t) => (t.vmName === vmName ? { ...t, status: 'error', message: msg } : t)),
          )
        }
      } catch (err) {
        setTasks((prev) =>
          prev.map((t) =>
            t.vmName === vmName
              ? { ...t, status: 'error', message: formatUserError(err) }
              : t,
          ),
        )
      }
    }
    setRunning(false)
    await fetchVMs()
  }

  const statusIcon = (status: TaskStatus) => {
    switch (status) {
      case 'pending':
        return <Clock className="w-4 h-4 text-[var(--zf-muted)]" />
      case 'running':
        return <Loader2 className="w-4 h-4 text-[var(--zf-link)] animate-spin" />
      case 'done':
        return <CheckCircle className="w-4 h-4 text-emerald-600" />
      case 'error':
        return <AlertTriangle className="w-4 h-4 text-red-600" />
    }
  }

  const stateColor = (state: string) => {
    const s = state?.toLowerCase() || ''
    if (s === 'running') return 'text-emerald-600'
    if (s === 'paused') return 'text-amber-600'
    return 'text-[var(--zf-muted)]'
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title="Bulk Operations"
        description="Start, stop, restart, or snapshot multiple VMs at once"
        onRefresh={fetchVMs}
        refreshing={loading}
      />

      {loadError && (
        <ErrorBanner
          title="Could not load virtual machines"
          headline={loadError}
          hints={hintsForError(loadError, 'vm')}
          onRetry={fetchVMs}
        />
      )}

      {someSelected && (
        <div className="flex items-center gap-3 bg-[var(--zf-link)]/10 border border-[var(--zf-link)]/30 rounded-xl px-4 py-3">
          <span className="text-sm text-[var(--zf-link)] font-medium">{selected.size} selected</span>
          <div className="flex-1" />
          <button
            type="button"
            onClick={() => runBulkAction('start')}
            disabled={running}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium text-emerald-700 bg-emerald-50 border border-emerald-200 hover:bg-emerald-100 transition-colors disabled:opacity-50"
          >
            <Play className="w-3.5 h-3.5" /> Start
          </button>
          <button
            type="button"
            onClick={() => runBulkAction('stop')}
            disabled={running}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium text-red-700 bg-red-50 border border-red-200 hover:bg-red-100 transition-colors disabled:opacity-50"
          >
            <Power className="w-3.5 h-3.5" /> Stop
          </button>
          <button
            type="button"
            onClick={() => runBulkAction('restart')}
            disabled={running}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium text-amber-800 bg-amber-50 border border-amber-200 hover:bg-amber-100 transition-colors disabled:opacity-50"
          >
            <RefreshCw className="w-3.5 h-3.5" /> Restart
          </button>
          <button
            type="button"
            onClick={() => runBulkAction('snapshot')}
            disabled={running}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium text-[var(--zf-ink)] bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] hover:bg-[var(--zf-surface)] transition-colors disabled:opacity-50"
          >
            <Camera className="w-3.5 h-3.5" /> Snapshot
          </button>
          <button
            type="button"
            onClick={deselectAll}
            className="px-3 py-1.5 rounded-lg text-sm text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition-colors"
          >
            Clear
          </button>
        </div>
      )}

      <div className="flex items-center gap-3">
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[var(--zf-muted)]" />
          <input
            type="text"
            value={searchFilter}
            onChange={(e) => setSearchFilter(e.target.value)}
            placeholder="Filter VMs…"
            aria-label="Filter VMs"
            className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg pl-10 pr-4 py-2 text-sm text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--zf-link)]"
          />
        </div>
        <button
          type="button"
          onClick={allFilteredSelected ? deselectAll : selectAll}
          className="flex items-center gap-2 px-3 py-2 rounded-lg text-sm text-[var(--zf-muted)] hover:text-[var(--zf-ink)] hover:bg-[var(--zf-surface)] transition-colors"
        >
          {allFilteredSelected ? <MinusSquare className="w-4 h-4" /> : <CheckSquare className="w-4 h-4" />}
          {allFilteredSelected ? 'Deselect All' : 'Select All'}
        </button>
      </div>

      {loading && !loadError ? (
        <div className="flex items-center justify-center py-16">
          <Loader2 className="w-6 h-6 text-[var(--zf-link)] animate-spin" />
          <span className="ml-3 text-sm text-[var(--zf-muted)]">Loading VMs…</span>
        </div>
      ) : !loadError && filtered.length === 0 ? (
        <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)]">
          <EmptyState
            icon={<Monitor className="w-10 h-10" />}
            title={vms.length === 0 ? 'No VMs found' : 'No VMs match the filter'}
            description={vms.length === 0 ? 'Create a VM to use bulk operations' : 'Try a different search term'}
          />
        </div>
      ) : !loadError ? (
        <div className="zf-panel-muted overflow-hidden">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-[var(--zf-hairline)]">
                <th className="px-4 py-3 text-left w-10">
                  <span className="sr-only">Select</span>
                </th>
                <th className="px-4 py-3 text-left text-[var(--zf-muted)] font-medium">Name</th>
                <th className="px-4 py-3 text-left text-[var(--zf-muted)] font-medium">State</th>
                <th className="px-4 py-3 text-left text-[var(--zf-muted)] font-medium hidden md:table-cell">CPU</th>
                <th className="px-4 py-3 text-left text-[var(--zf-muted)] font-medium hidden md:table-cell">Memory</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((vm) => {
                const isSelected = selected.has(vm.name)
                return (
                  <tr
                    key={vm.name}
                    onClick={() => toggleSelect(vm.name)}
                    className={`border-b border-[var(--zf-hairline)]/60 cursor-pointer transition-colors ${
                      isSelected ? 'bg-[var(--zf-link)]/10' : 'hover:bg-[var(--zf-canvas)]'
                    }`}
                  >
                    <td className="px-4 py-3">
                      {isSelected ? (
                        <CheckSquare className="w-4 h-4 text-[var(--zf-link)]" />
                      ) : (
                        <Square className="w-4 h-4 text-[var(--zf-muted)]" />
                      )}
                    </td>
                    <td className="px-4 py-3">
                      <div className="flex items-center gap-2">
                        <Monitor className="w-4 h-4 text-[var(--zf-muted)]" />
                        <span className="text-[var(--zf-ink)] font-medium">{vm.name}</span>
                      </div>
                    </td>
                    <td className="px-4 py-3">
                      <span className={`font-medium ${stateColor(vm.state)}`}>{vm.state || 'unknown'}</span>
                    </td>
                    <td className="px-4 py-3 text-[var(--zf-muted)] hidden md:table-cell">{vm.cpus ?? '-'}</td>
                    <td className="px-4 py-3 text-[var(--zf-muted)] hidden md:table-cell">
                      {vm.memory ? `${vm.memory} MB` : '-'}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      ) : null}

      {tasks.length > 0 && (
        <div className="zf-panel-muted p-4">
          <h3 className="text-sm font-semibold text-[var(--zf-ink)] mb-3">
            Batch Progress
            {running && <Loader2 className="inline-block w-4 h-4 ml-2 text-[var(--zf-link)] animate-spin" />}
          </h3>
          <div className="space-y-2 max-h-64 overflow-y-auto">
            {tasks.map((t) => (
              <div key={t.vmName} className="flex items-center gap-3 px-3 py-2 rounded-lg bg-[var(--zf-canvas)]">
                {statusIcon(t.status)}
                <span className="text-sm text-[var(--zf-ink)] font-medium flex-1">{t.vmName}</span>
                <span className="text-xs text-[var(--zf-muted)] capitalize">{t.action}</span>
                {t.message && (
                  <span className={`text-xs max-w-[40%] truncate ${t.status === 'error' ? 'text-red-600' : 'text-emerald-600'}`}>
                    {t.message}
                  </span>
                )}
              </div>
            ))}
          </div>
          {!running && tasks.length > 0 && (
            <button
              type="button"
              onClick={() => setTasks([])}
              className="mt-3 text-xs text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition-colors"
            >
              Clear results
            </button>
          )}
        </div>
      )}
    </div>
  )
}
