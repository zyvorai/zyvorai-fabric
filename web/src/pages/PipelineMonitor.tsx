// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback, Fragment } from 'react'
import { RefreshCw, Workflow } from 'lucide-react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, EmptyState, StatusBadge } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface PipelineJob {
  id: string
  vm_name: string
  source: string
  status: string
  progress: number
  pipeline_stage: string
  duration: string
  output_path: string
  error?: string
}

const PIPELINE_STAGES = ['inspect', 'prepare', 'convert', 'validate', 'deploy']

function getProgressBarColor(status: string): string {
  switch (status.toLowerCase()) {
    case 'completed': return 'bg-emerald-500'
    case 'running': return 'bg-[var(--zf-link-fill)]'
    case 'failed': return 'bg-red-500'
    default: return 'bg-[var(--zf-muted)]'
  }
}

function getStageIndex(stage: string): number {
  const idx = PIPELINE_STAGES.indexOf(stage.toLowerCase())
  return idx >= 0 ? idx : -1
}

export default function PipelineMonitor() {
  const toast = useToastContext()
  const [jobs, setJobs] = useState<PipelineJob[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)

  const fetchJobs = useCallback(async (silent = false) => {
    if (!silent) setLoadError(null)
    try {
      const response = await apiFetch('/api/pipeline/jobs')
      if (!response.ok) {
        const body = await response.text()
        throw new Error(formatHttpErrorBody(response.status, response.statusText, body))
      }
      const data = await response.json()
      setJobs(Array.isArray(data) ? data : data.jobs || [])
      setLoadError(null)
    } catch (err) {
      const msg = formatUserError(err)
      if (!silent || jobs.length === 0) {
        setLoadError(msg)
        if (!silent) toastFailure(toast, 'Failed to load pipeline jobs', err)
      }
    } finally {
      if (!silent) setLoading(false)
    }
  }, [toast, jobs.length])

  useEffect(() => {
    void fetchJobs(false)
    const interval = setInterval(() => void fetchJobs(true), 3000)
    return () => clearInterval(interval)
  }, [fetchJobs])

  return (
    <div className="space-y-6">
      <PageHeader
        title="Pipeline Monitor"
        description="Migration and conversion pipeline jobs (auto-refresh 3s)"
        onRefresh={fetchJobs}
        refreshing={loading}
      />

      {loadError && (
        <ErrorBanner
          title="Could not load pipeline jobs"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={fetchJobs}
        />
      )}

      <div className="zf-panel-muted overflow-hidden">
      <div className="p-5">
        {loading && !loadError && (
          <div className="flex items-center justify-center py-10 text-sm text-[var(--zf-muted)]">
            <RefreshCw className="w-4 h-4 mr-2 animate-spin" /> Loading pipeline jobs…
          </div>
        )}

        {!loading && jobs.length === 0 && !loadError && (
          <EmptyState
            icon={<Workflow className="w-10 h-10" />}
            title="No active pipeline jobs"
            description="Jobs appear here when migrations or conversions are running"
          />
        )}

        {jobs.map((job) => {
          const currentStage = getStageIndex(job.pipeline_stage)
          return (
            <div key={job.id} className="p-4 border-b border-[var(--zf-hairline)]/60 last:border-b-0">
              <div className="flex justify-between items-start mb-3">
                <div>
                  <div className="text-sm font-semibold text-[var(--zf-ink)] mb-0.5">{job.vm_name || job.id}</div>
                  <div className="text-xs text-[var(--zf-muted)]">Source: {job.source} | ID: {job.id}</div>
                </div>
                <StatusBadge status={job.status} />
              </div>

              <div className="mb-3">
                <div className="flex justify-between mb-1">
                  <span className="text-xs text-[var(--zf-muted)]">Progress</span>
                  <span className="text-xs font-semibold text-[var(--zf-ink)]">{job.progress}%</span>
                </div>
                <div className="bg-[var(--zf-hairline)] rounded-full h-2 overflow-hidden">
                  <div className={`h-full rounded-full transition-all duration-500 ${getProgressBarColor(job.status)}`} style={{ width: `${job.progress}%` }} />
                </div>
              </div>

              <div className="mb-3">
                <div className="text-xs text-[var(--zf-muted)] mb-2">Pipeline Stage</div>
                <div className="flex items-center gap-0">
                  {PIPELINE_STAGES.map((stage, idx) => {
                    let circleClasses = 'bg-[var(--zf-hairline)] text-[var(--zf-muted)]'
                    let lineClasses = 'h-0.5 w-8 bg-[var(--zf-hairline)]'
                    if (idx < currentStage) circleClasses = 'bg-emerald-500 text-white'
                    else if (idx === currentStage) circleClasses = job.status === 'failed' ? 'bg-red-500 text-white' : 'bg-[var(--zf-link-fill)] text-white animate-pulse'
                    if (idx <= currentStage) lineClasses = 'h-0.5 w-8 bg-emerald-500'
                    return (
                      <Fragment key={stage}>
                        {idx > 0 && <div className={lineClasses} />}
                        <div className={`px-2 py-1 text-xs font-semibold rounded uppercase whitespace-nowrap ${circleClasses}`}>{stage}</div>
                      </Fragment>
                    )
                  })}
                </div>
              </div>

              <div className="flex gap-4 flex-wrap">
                {job.duration && <div><span className="text-xs text-[var(--zf-muted)]">Duration: </span><span className="text-xs font-semibold text-[var(--zf-ink)]">{job.duration}</span></div>}
                {job.output_path && <div><span className="text-xs text-[var(--zf-muted)]">Output: </span><span className="text-xs font-semibold text-[var(--zf-ink)] font-mono">{job.output_path}</span></div>}
              </div>

              {job.error && (
                <div className="mt-2 px-3 py-2 bg-red-50 border border-red-200 rounded-lg">
                  <p className="text-xs text-red-700 font-mono">{job.error}</p>
                </div>
              )}
            </div>
          )
        })}
      </div>
      </div>
    </div>
  )
}
