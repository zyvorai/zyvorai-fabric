// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PanelLoading } from '../components/Skeleton'
import { useState, useEffect, useRef, useCallback, Fragment } from 'react'
import { Clock, CheckCircle, AlertCircle, Loader2, XCircle } from 'lucide-react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'
import { AppleTerminalFrame } from '../components/AppleTerminalFrame'
import { AnsiText } from '../components/AnsiText'

interface Job {
  id: string
  name: string
  vm_name: string
  status: string
  progress: number
  phase: string
  current_step: string
  error?: string
  created_at: string
  started_at?: string
  completed_at?: string
}

const STATUS_CONFIG: Record<string, { icon: typeof Clock; color: string; bg: string }> = {
  pending:   { icon: Clock,       color: 'text-amber-800',   bg: 'bg-amber-50' },
  running:   { icon: Loader2,     color: 'text-[var(--zf-link)]', bg: 'bg-[var(--zf-canvas)]' },
  completed: { icon: CheckCircle, color: 'text-emerald-700', bg: 'bg-emerald-50' },
  failed:    { icon: AlertCircle, color: 'text-red-700',     bg: 'bg-red-50' },
  cancelled: { icon: XCircle,     color: 'text-[var(--zf-muted)]',  bg: 'bg-black/[0.04]' },
}

const PIPELINE_STAGES = ['prepare', 'convert', 'validate', 'deploy']

function formatDuration(start: string, end: string): string {
  const sec = Math.round((new Date(end).getTime() - new Date(start).getTime()) / 1000)
  if (sec < 60) return `${sec}s`
  const min = Math.floor(sec / 60)
  if (min < 60) return `${min}m ${sec % 60}s`
  return `${Math.floor(min / 60)}h ${min % 60}m`
}

export default function JobMonitor() {
  const toast = useToastContext()
  const [jobs, setJobs] = useState<Job[]>([])
  const [selectedJob, setSelectedJob] = useState<string | null>(null)
  const [logs, setLogs] = useState<string[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [refreshError, setRefreshError] = useState<string | null>(null)
  const [follow, setFollow] = useState(true)
  const logRef = useRef<HTMLDivElement>(null)

  const fetchJobs = useCallback(async () => {
    try {
      const res = await apiFetch('/api/jobs')
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const data = await res.json()
      setJobs(Array.isArray(data) ? data : data.jobs || [])
      setLoadError(null)
      setRefreshError(null)
    } catch (err) {
      const msg = formatUserError(err)
      setJobs((prev) => {
        if (prev.length === 0) {
          setLoadError(msg)
          toastFailure(toast, 'Failed to load jobs', err)
        } else {
          setRefreshError(msg)
        }
        return prev
      })
    } finally {
      setLoading(false)
    }
  }, [toast])

  const fetchLogs = useCallback(async (jobId: string) => {
    try {
      const res = await apiFetch(`/api/jobs/${jobId}/logs`)
      if (!res.ok) return
      const data = await res.json()
      setLogs(Array.isArray(data) ? data : data.lines || data.logs || [])
    } catch { /* Log fetch failures are non-critical during streaming */ }
  }, [])

  useEffect(() => { fetchJobs(); const interval = setInterval(fetchJobs, 3000); return () => clearInterval(interval) }, [fetchJobs])
  useEffect(() => { if (selectedJob) { fetchLogs(selectedJob); const interval = setInterval(() => fetchLogs(selectedJob), 2000); return () => clearInterval(interval) } }, [selectedJob, fetchLogs])
  useEffect(() => { if (follow && logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight }, [logs, follow])

  const selected = jobs.find(j => j.id === selectedJob)
  const stageIndex = selected ? PIPELINE_STAGES.indexOf(selected.phase?.toLowerCase()) : -1

  if (loading && jobs.length === 0 && !loadError) {
    return (
      <div className="space-y-6">
        <PageHeader title="Job Monitor" description="Real-time job tracking with live logs" />
        <PanelLoading label="Loading jobs" />
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title="Job Monitor"
        description="Real-time job tracking with live logs"
        onRefresh={fetchJobs}
        refreshing={loading}
      />
      {loadError && (
        <ErrorBanner
          title="Could not load jobs"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={fetchJobs}
        />
      )}
      {refreshError && !loadError && (
        <div className="bg-amber-50 rounded-lg border border-amber-200 px-4 py-2 text-xs text-amber-800">
          {refreshError} — showing last known data
        </div>
      )}

      {!loadError && (
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Job list */}
        <div className="space-y-2 max-h-[600px] overflow-y-auto">
          {jobs.length === 0 ? (
            <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] text-center text-[var(--zf-muted)] text-sm">No jobs found</div>
          ) : jobs.map(job => {
            const cfg = STATUS_CONFIG[job.status] || STATUS_CONFIG.pending
            const Icon = cfg.icon
            const isSelected = selectedJob === job.id
            return (
              <div key={job.id} role="button" tabIndex={0} onClick={() => setSelectedJob(job.id)} onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setSelectedJob(job.id) } }} className={`bg-[var(--zf-canvas)] rounded-xl border p-4 cursor-pointer transition-all hover:scale-[1.01] ${isSelected ? 'border-[var(--zf-ink)]' : 'border-[var(--zf-hairline)]'}`}>
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm font-semibold text-[var(--zf-ink)] truncate">{job.name || job.vm_name || job.id}</span>
                  <span className={`flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${cfg.bg} ${cfg.color}`}>
                    <Icon className={`w-3.5 h-3.5 ${job.status === 'running' ? 'animate-spin' : ''}`} /> {job.status}
                  </span>
                </div>
                <div className="bg-[var(--zf-hairline)] rounded-full h-1.5 mb-2"><div className={`h-full rounded-full transition-all ${job.status === 'completed' ? 'bg-[var(--zf-success)]' : job.status === 'failed' ? 'bg-[var(--zf-danger)]' : 'bg-[var(--zf-link-fill)]'}`} style={{ width: `${job.progress}%` }} /></div>
                <div className="text-xs text-[var(--zf-muted)]">{job.progress}% {job.current_step && `— ${job.current_step}`}</div>
              </div>
            )
          })}
        </div>

        {/* Detail + logs */}
        <div className="lg:col-span-2 space-y-4">
          {selected ? (
            <>
              <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-5">
                <h3 className="text-base font-semibold text-[var(--zf-ink)] mb-3">{selected.name || selected.vm_name || selected.id}</h3>
                <div className="grid grid-cols-2 lg:grid-cols-4 gap-3 mb-4">
                  <div className="bg-[var(--zf-surface)] rounded-lg p-3 border border-[var(--zf-hairline)]"><div className="text-xs text-[var(--zf-muted)]">Status</div><div className="text-xs font-semibold text-[var(--zf-ink)] capitalize">{selected.status}</div></div>
                  <div className="bg-[var(--zf-surface)] rounded-lg p-3 border border-[var(--zf-hairline)]"><div className="text-xs text-[var(--zf-muted)]">Progress</div><div className="text-xs font-semibold text-[var(--zf-ink)]">{selected.progress}%</div></div>
                  <div className="bg-[var(--zf-surface)] rounded-lg p-3 border border-[var(--zf-hairline)]"><div className="text-xs text-[var(--zf-muted)]">Phase</div><div className="text-xs font-semibold text-[var(--zf-ink)] capitalize">{selected.phase || '-'}</div></div>
                  <div className="bg-[var(--zf-surface)] rounded-lg p-3 border border-[var(--zf-hairline)]"><div className="text-xs text-[var(--zf-muted)]">Duration</div><div className="text-xs font-semibold text-[var(--zf-ink)]">{selected.started_at && selected.completed_at ? formatDuration(selected.started_at, selected.completed_at) : selected.started_at ? formatDuration(selected.started_at, new Date().toISOString()) : '-'}</div></div>
                </div>
                {/* Pipeline stages */}
                <div className="flex items-center gap-0 mb-3">
                  {PIPELINE_STAGES.map((stage, idx) => {
                    let cls = 'bg-[var(--zf-hairline)] text-[var(--zf-muted)]'
                    if (idx < stageIndex) cls = 'bg-[var(--zf-success)] text-white'
                    else if (idx === stageIndex) cls = selected.status === 'failed' ? 'bg-[var(--zf-danger)] text-white' : 'bg-[var(--zf-link-fill)] text-white animate-pulse'
                    return (
                      <Fragment key={stage}>
                        {idx > 0 && <div className={`h-0.5 w-6 ${idx <= stageIndex ? 'bg-[var(--zf-success)]' : 'bg-[var(--zf-hairline)]'}`} />}
                        <div className={`px-2 py-1 text-xs font-semibold rounded uppercase whitespace-nowrap ${cls}`}>{stage}</div>
                      </Fragment>
                    )
                  })}
                </div>
                {selected.error && (
                  <AppleTerminalFrame
                    title="job — error"
                    bodyClassName="max-h-32 overflow-y-auto px-3 py-2 whitespace-pre-wrap text-[#ff453a]"
                  >
                    {selected.error}
                  </AppleTerminalFrame>
                )}
              </div>
              <AppleTerminalFrame
                title={`${selected.name || selected.vm_name || selected.id} — job logs`}
                live={selected.status === 'running'}
                trailing={
                  <label className="flex items-center gap-2 text-xs text-white/50 cursor-pointer shrink-0">
                    <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} className="rounded border-white/20" />
                    Follow
                  </label>
                }
                bodyRef={logRef}
                bodyClassName="max-h-80 overflow-y-auto px-3 py-2 whitespace-pre-wrap"
                empty={logs.length === 0}
                emptyMessage="No logs available — click a running job to stream logs"
              >
                {logs.map((line, i) => (
                  <div key={i}>
                    <AnsiText text={line} />
                  </div>
                ))}
              </AppleTerminalFrame>
            </>
          ) : (
            <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] text-center text-[var(--zf-muted)] text-sm">Select a job to view details and logs</div>
          )}
        </div>
      </div>
      )}
    </div>
  )
}
