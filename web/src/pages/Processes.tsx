// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback, useRef } from 'react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { AppleTerminalFrame } from '../components/AppleTerminalFrame'
import { formatHttpErrorBody, formatUserError, sanitizeErrorText } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface ProcessDetail {
  error?: string
  cmdline?: string
  io_read_bytes?: number
  io_write_bytes?: number
  fds?: number
  voluntary_ctx_switches?: number
  involuntary_ctx_switches?: number
}

interface ProcessRow {
  pid: number
  name: string
  cpu_percent?: number
  memory_mb?: number
  memory_bytes?: number
  state?: string
  threads?: number
  num_threads?: number
}

async function parseJsonResponse<T>(res: Response): Promise<T> {
  const contentType = res.headers.get('content-type') ?? ''
  const text = await res.text()
  const trimmed = text.trimStart()
  const looksJson = contentType.includes('json') || trimmed.startsWith('{') || trimmed.startsWith('[')
  if (!looksJson) {
    throw new Error(sanitizeErrorText(text) || 'The API returned a non-JSON response')
  }
  try {
    return JSON.parse(text) as T
  } catch {
    throw new Error(sanitizeErrorText(text) || 'Failed to parse API response as JSON')
  }
}

function stateBadge(state: string): { label: string; classes: string } {
  switch (state) {
    case 'R': return { label: 'Running', classes: 'text-emerald-700 bg-emerald-50 border-emerald-200' }
    case 'S': return { label: 'Sleeping', classes: 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]' }
    case 'D': return { label: 'Disk Wait', classes: 'text-amber-800 bg-amber-50 border-amber-200' }
    case 'Z': return { label: 'Zombie', classes: 'text-red-700 bg-red-50 border-red-200' }
    case 'T': return { label: 'Stopped', classes: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]' }
    default: return { label: state, classes: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]' }
  }
}

function cpuBarColor(cpu: number): string {
  if (cpu >= 80) return 'bg-[var(--zf-danger)]'
  if (cpu >= 50) return 'bg-[var(--zf-warning)]'
  if (cpu >= 20) return 'bg-[var(--zf-link-fill)]'
  return 'bg-[var(--zf-success)]'
}

export default function Processes() {
  const toast = useToastContext()
  const [processes, setProcesses] = useState<ProcessRow[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [search, setSearch] = useState('')
  const [selectedPid, setSelectedPid] = useState<number | null>(null)
  const [detail, setDetail] = useState<ProcessDetail | null>(null)
  const [detailLoading, setDetailLoading] = useState(false)
  const hadLoadErrorRef = useRef(false)

  const fetchProcesses = useCallback(async (opts?: { showToast?: boolean }) => {
    try {
      const res = await apiFetch('/api/system/processes')
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const data = await parseJsonResponse<ProcessRow[] | { processes?: ProcessRow[] }>(res)
      setProcesses(Array.isArray(data) ? data : data.processes ?? [])
      setLoadError(null)
      hadLoadErrorRef.current = false
    } catch (err) {
      const msg = formatUserError(err)
      setLoadError(msg)
      if (opts?.showToast) {
        toastFailure(toast, 'Failed to load processes', err)
      }
      hadLoadErrorRef.current = true
    } finally {
      setLoading(false)
    }
  }, [toast])

  useEffect(() => {
    fetchProcesses()
    const interval = setInterval(fetchProcesses, 3000)
    return () => clearInterval(interval)
  }, [fetchProcesses])

  const fetchDetail = useCallback(async (pid: number) => {
    setSelectedPid(pid)
    setDetailLoading(true)
    setDetail(null)
    try {
      const res = await apiFetch(`/api/system/process?pid=${pid}`)
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const data = await parseJsonResponse<ProcessDetail>(res)
      setDetail(data)
    } catch (err) {
      setDetail({ error: formatUserError(err) })
    } finally {
      setDetailLoading(false)
    }
  }, [])

  const filtered = processes.filter((p) => {
    if (!search) return true
    const q = search.toLowerCase()
    return (
      String(p.pid).includes(q) ||
      (p.name && p.name.toLowerCase().includes(q)) ||
      (p.state && p.state.toLowerCase().includes(q))
    )
  })

  const totalCount = processes.length
  const runningCount = processes.filter((p) => p.state === 'R').length
  const sleepingCount = processes.filter((p) => p.state === 'S').length

  return (
    <div className="space-y-6">
      <PageHeader
        title="Processes"
        description="System process monitor (auto-refresh 3s)"
        onRefresh={() => fetchProcesses({ showToast: true })}
        refreshing={loading}
      />

      {loadError && (
        <ErrorBanner
          title="Could not load processes"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={() => fetchProcesses({ showToast: true })}
        />
      )}

      {loading && !loadError ? (
        <div className="flex items-center justify-center h-64 text-[var(--zf-muted)]">
          <div className="animate-spin w-6 h-6 border-2 border-[var(--zf-link)] border-t-transparent rounded-full mr-3" />
          Loading processes…
        </div>
      ) : !loadError ? (
        <>

      <div className="grid grid-cols-3 gap-4">
        <div className="bg-[var(--zf-canvas)] rounded-xl p-5 border border-[var(--zf-hairline)]">
          <div className="text-xs text-[var(--zf-muted)] mb-1">Total Processes</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{totalCount}</div>
        </div>
        <div className="bg-[var(--zf-canvas)] rounded-xl p-5 border border-[var(--zf-hairline)]">
          <div className="text-xs text-[var(--zf-muted)] mb-1">Running</div>
          <div className="text-2xl font-bold text-emerald-700">{runningCount}</div>
        </div>
        <div className="bg-[var(--zf-canvas)] rounded-xl p-5 border border-[var(--zf-hairline)]">
          <div className="text-xs text-[var(--zf-muted)] mb-1">Sleeping</div>
          <div className="text-2xl font-bold text-[var(--zf-link)]">{sleepingCount}</div>
        </div>
      </div>

      <div>
        <input
          type="text"
          placeholder="Filter by PID, name, or state..."
          aria-label="Filter processes"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="input-field"
        />
      </div>

      {filtered.length === 0 ? (
        <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] text-center text-[var(--zf-muted)] text-sm">
          No processes match your filter
        </div>
      ) : (
        <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-[var(--zf-hairline)]">
                  <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">PID</th>
                  <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Name</th>
                  <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">CPU %</th>
                  <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Memory MB</th>
                  <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">State</th>
                  <th className="text-left px-4 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Threads</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((proc) => {
                  const badge = stateBadge(proc.state ?? '?')
                  const cpuPct = Math.min(proc.cpu_percent ?? 0, 100)
                  return (
                    <tr
                      key={proc.pid}
                      onClick={() => fetchDetail(proc.pid)}
                      className={`border-b border-[var(--zf-hairline)]/60 hover:bg-black/[0.04] transition-colors cursor-pointer ${selectedPid === proc.pid ? 'bg-black/[0.04]' : ''}`}
                    >
                      <td className="px-4 py-3 text-[var(--zf-ink)] font-mono">{proc.pid}</td>
                      <td className="px-4 py-3 text-[var(--zf-ink)] font-medium">{proc.name}</td>
                      <td className="px-4 py-3">
                        <div className="flex items-center gap-2">
                          <div className="w-20 h-2 bg-[var(--zf-hairline)] rounded-full overflow-hidden">
                            <div
                              className={`h-full rounded-full ${cpuBarColor(cpuPct)}`}
                              style={{ width: `${cpuPct}%` }}
                            />
                          </div>
                          <span className="text-[var(--zf-ink)] text-xs w-12">{cpuPct.toFixed(1)}%</span>
                        </div>
                      </td>
                      <td className="px-4 py-3 text-[var(--zf-ink)]">{(proc.memory_mb ?? (proc.memory_bytes ? (proc.memory_bytes / 1024 / 1024).toFixed(1) : 0))}</td>
                      <td className="px-4 py-3">
                        <span className={`rounded-full px-2.5 py-0.5 text-xs font-medium border ${badge.classes}`}>
                          {badge.label}
                        </span>
                      </td>
                      <td className="px-4 py-3 text-[var(--zf-ink)]">{proc.threads ?? proc.num_threads ?? '-'}</td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {selectedPid !== null && (
        <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-5">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-base font-semibold text-[var(--zf-ink)]">Process Detail - PID {selectedPid}</h3>
            <button
              onClick={() => { setSelectedPid(null); setDetail(null) }}
              className="zf-btn zf-btn-ghost zf-btn-sm"
            >
              Close
            </button>
          </div>
          {detailLoading ? (
            <div className="flex items-center text-[var(--zf-muted)] text-sm">
              <div className="animate-spin w-4 h-4 border-2 border-[var(--zf-link)] border-t-transparent rounded-full mr-2" />
              Loading...
            </div>
          ) : detail?.error ? (
            <div className="text-red-700 text-sm">{detail.error}</div>
          ) : detail ? (
            <div className="grid grid-cols-2 lg:grid-cols-3 gap-4">
              {detail.cmdline && (
                <div className="col-span-full">
                  <AppleTerminalFrame
                    title={`pid ${selectedPid} — cmdline`}
                    bodyClassName="max-h-32 overflow-auto px-3 py-2 whitespace-pre-wrap break-all"
                  >
                    {detail.cmdline}
                  </AppleTerminalFrame>
                </div>
              )}
              {detail.io_read_bytes !== undefined && (
                <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg p-3">
                  <div className="text-xs text-[var(--zf-muted)] mb-1">IO Read Bytes</div>
                  <div className="text-sm font-semibold text-[var(--zf-ink)]">{Number(detail.io_read_bytes).toLocaleString()}</div>
                </div>
              )}
              {detail.io_write_bytes !== undefined && (
                <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg p-3">
                  <div className="text-xs text-[var(--zf-muted)] mb-1">IO Write Bytes</div>
                  <div className="text-sm font-semibold text-[var(--zf-ink)]">{Number(detail.io_write_bytes).toLocaleString()}</div>
                </div>
              )}
              {detail.fds !== undefined && (
                <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg p-3">
                  <div className="text-xs text-[var(--zf-muted)] mb-1">File Descriptors</div>
                  <div className="text-sm font-semibold text-[var(--zf-ink)]">{detail.fds}</div>
                </div>
              )}
              {detail.voluntary_ctx_switches !== undefined && (
                <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg p-3">
                  <div className="text-xs text-[var(--zf-muted)] mb-1">Voluntary Ctx Switches</div>
                  <div className="text-sm font-semibold text-[var(--zf-ink)]">{Number(detail.voluntary_ctx_switches).toLocaleString()}</div>
                </div>
              )}
              {detail.involuntary_ctx_switches !== undefined && (
                <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg p-3">
                  <div className="text-xs text-[var(--zf-muted)] mb-1">Involuntary Ctx Switches</div>
                  <div className="text-sm font-semibold text-[var(--zf-ink)]">{Number(detail.involuntary_ctx_switches).toLocaleString()}</div>
                </div>
              )}
            </div>
          ) : null}
        </div>
      )}
        </>
      ) : null}
    </div>
  )
}
