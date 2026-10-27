// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useRef, useCallback } from 'react'
import { RefreshCw, HardDrive, ArrowRight, AlertTriangle, CheckCircle, Loader2 } from 'lucide-react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import PageLoadBanner from '../components/PageLoadBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { hintsForError } from '../utils/daemonHints'
import { usePageLoader } from '../hooks/usePageLoader'

interface DiskImage {
  path: string
  name: string
  format: string
  size: number
}

interface JobStatus {
  id: string
  status: string
  progress: number
  error?: string
  output_path?: string
}

const FORMATS = ['qcow2', 'vmdk', 'vhd', 'vhdx', 'raw'] as const

function deriveOutputPath(sourcePath: string, targetFormat: string): string {
  if (!sourcePath) return ''
  const lastDot = sourcePath.lastIndexOf('.')
  const base = lastDot > 0 ? sourcePath.substring(0, lastDot) : sourcePath
  return `${base}.${targetFormat}`
}

function formatBytes(bytes: number): string {
  if (!bytes) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let i = 0
  let val = bytes
  while (val >= 1024 && i < units.length - 1) { val /= 1024; i++ }
  return `${val.toFixed(1)} ${units[i]}`
}

export default function DiskConverter() {
  const [diskImages, setDiskImages] = useState<DiskImage[]>([])
  const { loading: loadingImages, loadError, run } = usePageLoader('Failed to load disk images')
  const [sourcePath, setSourcePath] = useState('')
  const [targetFormat, setTargetFormat] = useState<string>('qcow2')
  const [outputPath, setOutputPath] = useState('')
  const [outputEdited, setOutputEdited] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')
  const [job, setJob] = useState<JobStatus | null>(null)
  const [polling, setPolling] = useState(false)

  const loadDiskImages = useCallback((silent = false) => {
    return run(async () => {
      const res = await apiFetch('/api/images')
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const data = await res.json()
      const raw = Array.isArray(data) ? data : data.images || data.disk_images || []
      setDiskImages(
        raw.map((img: { name?: string; path?: string; format?: string; size?: number; size_bytes?: number }) => ({
          name: img.name || '',
          path: img.path || '',
          format: img.format || '',
          size: img.size ?? img.size_bytes ?? 0,
        })),
      )
    }, silent ? { silent: true } : undefined)
  }, [run])

  useEffect(() => {
    void loadDiskImages(false)
  }, [loadDiskImages])

  useEffect(() => {
    if (!outputEdited) setOutputPath(deriveOutputPath(sourcePath, targetFormat))
  }, [sourcePath, targetFormat, outputEdited])

  const jobRef = useRef<JobStatus | null>(null)
  useEffect(() => { jobRef.current = job }, [job])

  useEffect(() => {
    if (!polling) return
    const currentJob = jobRef.current
    if (!currentJob) return
    if (currentJob.status === 'completed' || currentJob.status === 'failed' || currentJob.status === 'cancelled') { setPolling(false); return }
    const jobId = currentJob.id
    const interval = setInterval(async () => {
      try {
        const res = await apiFetch(`/api/images/convert/${jobId}`)
        if (res.ok) {
          const data = await res.json()
          const updated: JobStatus = { id: jobId, status: data.status || 'unknown', progress: data.progress ?? 0, error: data.error, output_path: data.output_path }
          setJob(updated)
          if (updated.status === 'completed' || updated.status === 'failed' || updated.status === 'cancelled') setPolling(false)
        }
      } catch { /* retry */ }
    }, 2000)
    return () => clearInterval(interval)
  }, [polling])

  const handleConvert = async () => {
    if (!sourcePath.trim()) { setError('Please select or enter a source disk image.'); return }
    if (!outputPath.trim()) { setError('Please enter an output path.'); return }
    setError(''); setSubmitting(true); setJob(null)
    try {
      const res = await apiFetch('/api/images/convert', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ source_path: sourcePath, output_path: outputPath, format: targetFormat }),
      })
      const data = await res.json()
      if (!res.ok) throw new Error(data.message || data.error || 'Conversion request failed')
      const jobId = data.job_id || data.id
      if (!jobId) throw new Error('No job ID returned from server')
      setJob({ id: jobId, status: 'pending', progress: 0 })
      setPolling(true)
    } catch (e) {
      setError(formatUserError(e))
    } finally { setSubmitting(false) }
  }

  const handleReset = () => {
    setSourcePath(''); setTargetFormat('qcow2'); setOutputPath(''); setOutputEdited(false); setError(''); setJob(null); setPolling(false)
  }

  const jobDone = job?.status === 'completed'
  const jobFailed = job?.status === 'failed'
  const jobRunning = job && !jobDone && !jobFailed && job.status !== 'cancelled'

  return (
    <div className="space-y-4">
      <PageHeader title="Disk Format Converter" description="Convert disk images between qcow2, vmdk, vhd, vhdx, and raw formats" />
      <PageLoadBanner title="Could not load disk images" headline={loadError} onRetry={() => void loadDiskImages()} />
      {error && (
        <ErrorBanner title="Conversion error" headline={error} hints={hintsForError(error, 'storage')} onRetry={handleConvert} />
      )}

      <div className="zf-panel-muted p-5">
        <div className="mb-4">
          <label className="block text-xs font-medium text-[var(--zf-muted)] mb-1">Source Disk Image</label>
          {diskImages.length > 0 ? (
            <div className="space-y-2">
              <select value={sourcePath} onChange={(e) => { setSourcePath(e.target.value); setOutputEdited(false) }}
                className="input-field text-sm">
                <option value="">Select a disk image...</option>
                {diskImages.map((img) => (
                  <option key={img.path} value={img.path}>{img.name || img.path} ({img.format?.toUpperCase()}, {formatBytes(img.size)})</option>
                ))}
              </select>
              <div className="text-xs text-[var(--zf-muted)]">Or type a path:</div>
              <input type="text" value={sourcePath} onChange={(e) => { setSourcePath(e.target.value); setOutputEdited(false) }} placeholder="/path/to/disk.vmdk"
                className="input-field text-sm" />
            </div>
          ) : (
            <div className="space-y-2">
              <input type="text" value={sourcePath} onChange={(e) => { setSourcePath(e.target.value); setOutputEdited(false) }} placeholder="/path/to/disk.vmdk"
                className="input-field text-sm" />
              <button onClick={() => void loadDiskImages()} disabled={loadingImages} className="flex items-center gap-1.5 text-xs text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] transition-colors">
                <RefreshCw className={`w-3.5 h-3.5 ${loadingImages ? 'animate-spin' : ''}`} /> Load available disk images
              </button>
            </div>
          )}
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4">
          <div>
            <label className="block text-xs font-medium text-[var(--zf-muted)] mb-1">Target Format</label>
            <select aria-label="Target Format" value={targetFormat} onChange={(e) => { setTargetFormat(e.target.value); setOutputEdited(false) }}
              className="input-field text-sm">
              {FORMATS.map((f) => (<option key={f} value={f}>{f.toUpperCase()}</option>))}
            </select>
          </div>
          <div>
            <label className="block text-xs font-medium text-[var(--zf-muted)] mb-1">Output Path</label>
            <input aria-label="Output Path" type="text" value={outputPath} onChange={(e) => { setOutputPath(e.target.value); setOutputEdited(true) }} placeholder="Auto-generated from source"
              className="input-field text-sm" />
          </div>
        </div>

        {sourcePath && (
          <div className="flex items-center gap-3 mb-4 p-3 rounded-lg bg-[var(--zf-surface)] border border-[var(--zf-hairline)]">
            <span className="text-xs text-[var(--zf-ink)] truncate flex-1">{sourcePath}</span>
            <ArrowRight className="w-4 h-4 text-[var(--zf-link)] flex-shrink-0" />
            <span className="text-xs font-medium text-[var(--zf-link)] flex-shrink-0">{targetFormat.toUpperCase()}</span>
          </div>
        )}

        {error && (
          <div className="bg-red-50 border border-red-200 rounded-lg p-3 mb-4 flex items-start gap-2">
            <AlertTriangle className="w-4 h-4 text-red-700 flex-shrink-0 mt-0.5" />
            <p className="text-sm text-red-700">{error}</p>
          </div>
        )}

        <div className="flex items-center gap-3 mb-4">
          <button onClick={handleConvert} disabled={submitting || !!jobRunning || !sourcePath.trim()}
            className="zf-btn zf-btn-primary">
            {submitting ? <><Loader2 className="w-4 h-4 animate-spin" />Submitting...</> : <><HardDrive className="w-4 h-4" />Convert</>}
          </button>
          {(job || error) && (
            <button onClick={handleReset} className="zf-btn zf-btn-ghost">Reset</button>
          )}
        </div>

        {job && (
          <div className="bg-[var(--zf-surface)] rounded-lg border border-[var(--zf-hairline)] p-4">
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                {jobDone && <CheckCircle className="w-4 h-4 text-emerald-700" />}
                {jobFailed && <AlertTriangle className="w-4 h-4 text-red-700" />}
                {jobRunning && <Loader2 className="w-4 h-4 text-[var(--zf-link)] animate-spin" />}
                <span className="text-sm font-medium text-[var(--zf-ink)]">
                  {jobDone ? 'Conversion Complete' : jobFailed ? 'Conversion Failed' : 'Converting...'}
                </span>
              </div>
              <span className="text-xs text-[var(--zf-muted)] font-mono">{job.id.substring(0, 12)}</span>
            </div>
            <div className="bg-[var(--zf-hairline)] rounded-full h-2 mb-2">
              <div className={`rounded-full h-full transition-all duration-500 ${jobDone ? 'bg-[var(--zf-success)]' : jobFailed ? 'bg-[var(--zf-danger)]' : 'bg-[var(--zf-link-fill)]'}`} style={{ width: `${job.progress}%` }} />
            </div>
            <div className="flex items-center justify-between text-xs text-[var(--zf-muted)]">
              <span>{job.progress}%</span>
              <span className={`capitalize ${jobDone ? 'text-emerald-700' : jobFailed ? 'text-red-700' : 'text-[var(--zf-link)]'}`}>{job.status}</span>
            </div>
            {jobFailed && job.error && <div className="mt-3 p-2 bg-red-50 rounded text-xs text-red-700">{job.error}</div>}
            {jobDone && (
              <div className="mt-3 text-xs text-[var(--zf-muted)]">
                Output: <code className="bg-[var(--zf-canvas)] px-1.5 py-0.5 rounded text-xs text-[var(--zf-ink)]">{job.output_path || outputPath}</code>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
