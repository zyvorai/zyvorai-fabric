// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { apiFetch } from '../api/client'
import { listVMs } from '../api/vm'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, Card } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface VM { name: string; state?: string }
interface Schedule { id: string; name: string; cron: string; next_run?: string; vms: string[]; enabled: boolean; output_dir: string; retention: number; format: string; compression: boolean }
type Preset = 'daily2am' | 'weeklySunday' | 'every6h' | 'custom'
const presetCrons: Record<Exclude<Preset, 'custom'>, string> = { daily2am: '0 2 * * *', weeklySunday: '0 3 * * 0', every6h: '0 */6 * * *' }

function parseCronToHuman(cron: string): string {
  const parts = cron.trim().split(/\s+/)
  if (parts.length !== 5) return cron
  const [minute, hour, , , dow] = parts
  if (minute === '0' && hour !== '*' && dow === '*') { const h = parseInt(hour, 10); return `Daily at ${h === 0 ? 12 : h > 12 ? h - 12 : h}:00 ${h >= 12 ? 'PM' : 'AM'}` }
  if (minute === '0' && hour !== '*' && dow !== '*') { const days = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday']; const h = parseInt(hour, 10); return `Weekly on ${days[parseInt(dow, 10)] || dow} at ${h === 0 ? 12 : h > 12 ? h - 12 : h}:00 ${h >= 12 ? 'PM' : 'AM'}` }
  if (hour.startsWith('*/')) return `Every ${hour.replace('*/', '')} hours`
  return cron
}

export default function BackupScheduler() {
  const toast = useToastContext()
  const [vms, setVMs] = useState<VM[]>([])
  const [selectedVMs, setSelectedVMs] = useState<Set<string>>(new Set())
  const [schedules, setSchedules] = useState<Schedule[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [successMsg, setSuccessMsg] = useState<string | null>(null)
  const [preset, setPreset] = useState<Preset>('daily2am')
  const [customCron, setCustomCron] = useState('0 2 * * *')
  const [outputDir, setOutputDir] = useState('/var/lib/zyvor-fabricd/backups')
  const [retention, setRetention] = useState(7)
  const [format, setFormat] = useState('qcow2')
  const [compression, setCompression] = useState(true)
  const [scheduleName, setScheduleName] = useState('')
  const activeCron = preset === 'custom' ? customCron : presetCrons[preset]

  const fetchVMs = useCallback(async () => {
    try {
      setVMs(await listVMs())
    } catch (err) {
      setVMs([])
      throw err
    }
  }, [])

  const fetchSchedules = useCallback(async () => {
    try {
      const resp = await apiFetch('/api/schedules')
      if (!resp.ok) {
        const body = await resp.text()
        throw new Error(formatHttpErrorBody(resp.status, resp.statusText, body))
      }
      const data = await resp.json()
      setSchedules(Array.isArray(data) ? data : data.schedules || [])
    } catch (err) {
      setSchedules([])
      throw err
    }
  }, [])

  const loadAll = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      await Promise.all([fetchVMs(), fetchSchedules()])
    } catch (err) {
      const msg = formatUserError(err)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load backup scheduler data', err)
    } finally {
      setLoading(false)
    }
  }, [fetchVMs, fetchSchedules, toast])

  useEffect(() => {
    loadAll()
  }, [loadAll])

  const toggleVM = (name: string) => { setSelectedVMs((prev) => { const next = new Set(prev); if (next.has(name)) next.delete(name); else next.add(name); return next }) }
  const toggleAll = () => { if (selectedVMs.size === vms.length) setSelectedVMs(new Set()); else setSelectedVMs(new Set(vms.map((v) => v.name))) }

  const handleCreate = async () => {
    if (selectedVMs.size === 0) { setActionError('Select at least one VM'); return }
    if (!scheduleName.trim()) { setActionError('Enter a schedule name'); return }
    setCreating(true); setActionError(null); setSuccessMsg(null)
    try {
      const resp = await apiFetch('/api/schedules', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name: scheduleName, cron: activeCron, vms: Array.from(selectedVMs), output_dir: outputDir, retention, format, compression }) })
      if (!resp.ok) {
        const body = await resp.text()
        throw new Error(formatHttpErrorBody(resp.status, resp.statusText, body))
      }
      setSuccessMsg('Schedule created successfully')
      toast.success('Backup schedule created')
      setScheduleName('')
      setSelectedVMs(new Set())
      fetchSchedules()
    } catch (err) {
      const msg = formatUserError(err)
      setActionError(msg)
      toastFailure(toast, 'Failed to create schedule', err)
    } finally { setCreating(false) }
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title="Backup Scheduler"
        description="Create and manage automated VM backup schedules"
        onRefresh={loadAll}
        refreshing={loading}
      />
      {loadError && (
        <ErrorBanner
          title="Could not load scheduler data"
          headline={loadError}
          hints={hintsForError(loadError, 'storage')}
          onRetry={loadAll}
        />
      )}
      {actionError && (
        <div className="rounded-xl px-4 py-3 text-sm text-red-700 bg-red-50 border border-red-200">{actionError}</div>
      )}
      {successMsg && <div className="rounded-xl px-4 py-3 text-sm text-emerald-700 bg-emerald-50 border border-emerald-200">{successMsg}</div>}

      {loading && !loadError && (
        <div className="flex items-center justify-center h-64">
          <div className="w-8 h-8 border-2 border-[var(--zf-hairline)] border-t-[var(--zf-ink)] rounded-full animate-spin" />
        </div>
      )}

      {!loading && !loadError && (
      <>
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <Card><div className="p-5">
          <h3 className="text-sm font-semibold text-[var(--zf-ink)] mb-3">Select VMs</h3>
          {vms.length === 0 ? <p className="text-sm text-[var(--zf-muted)]">No VMs available.</p> : (
            <><button onClick={toggleAll} className="text-xs text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] mb-3">{selectedVMs.size === vms.length ? 'Deselect All' : 'Select All'}</button>
            <div className="space-y-1.5 max-h-64 overflow-y-auto">
              {vms.map((vm) => (
                <label key={vm.name} className="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-black/[0.04] cursor-pointer transition-colors">
                  <input type="checkbox" checked={selectedVMs.has(vm.name)} onChange={() => toggleVM(vm.name)} className="w-4 h-4 rounded border-[var(--zf-hairline)]" />
                  <span className="text-sm text-[var(--zf-ink)]">{vm.name}</span>
                  {vm.state && <span className={`text-xs px-1.5 py-0.5 rounded border ${vm.state === 'running' ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'}`}>{vm.state}</span>}
                </label>
              ))}
            </div></>
          )}
        </div></Card>

        <Card><div className="p-5 space-y-4">
          <h3 className="text-sm font-semibold text-[var(--zf-ink)] mb-1">Schedule Configuration</h3>
          <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Schedule Name</label><input aria-label="Schedule Name" type="text" value={scheduleName} onChange={(e) => setScheduleName(e.target.value)} placeholder="e.g. nightly-backup" className="input-field" /></div>
          <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Frequency</label>
            <div className="grid grid-cols-2 gap-2">
              {([['daily2am', 'Daily 2 AM'], ['weeklySunday', 'Weekly Sun 3 AM'], ['every6h', 'Every 6h'], ['custom', 'Custom']] as [Preset, string][]).map(([id, label]) => (
                <button key={id} onClick={() => setPreset(id)} className={`px-3 py-2 rounded-lg text-xs font-medium transition-colors border ${preset === id ? 'bg-[var(--zf-link)]/10 text-[var(--zf-link)] border-[var(--zf-link)]/30' : 'bg-[var(--zf-canvas)] text-[var(--zf-muted)] border-[var(--zf-hairline)] hover:bg-black/[0.04]'}`}>{label}</button>
              ))}
            </div>
          </div>
          {preset === 'custom' && <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Cron Expression</label><input aria-label="Cron Expression" type="text" value={customCron} onChange={(e) => setCustomCron(e.target.value)} placeholder="0 2 * * *" className="input-field font-mono" /></div>}
          <div className="bg-[var(--zf-canvas)] rounded-lg px-3 py-2 text-xs text-[var(--zf-muted)]"><span className="text-[var(--zf-muted)]">Schedule:</span> <span className="text-[var(--zf-ink)] font-medium">{parseCronToHuman(activeCron)}</span></div>
          <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Output Directory</label><input aria-label="Output Directory" type="text" value={outputDir} onChange={(e) => setOutputDir(e.target.value)} className="input-field font-mono" /></div>
          <div className="grid grid-cols-2 gap-3">
            <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Retention (keep last N)</label><input aria-label="Retention (keep last N)" type="number" value={retention} onChange={(e) => setRetention(Math.max(1, parseInt(e.target.value, 10) || 1))} min={1} max={365} className="input-field" /></div>
            <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Format</label><select aria-label="Format" value={format} onChange={(e) => setFormat(e.target.value)} className="input-field"><option value="qcow2">QCOW2</option><option value="raw">RAW</option><option value="vmdk">VMDK</option></select></div>
          </div>
          <label className="flex items-center gap-3 cursor-pointer"><input type="checkbox" checked={compression} onChange={() => setCompression(!compression)} className="sr-only" /><div className={`relative w-10 h-5 rounded-full transition-colors ${compression ? 'bg-[var(--zf-ink)]' : 'bg-[var(--zf-hairline)]'}`}><div className={`absolute top-0.5 w-4 h-4 rounded-full bg-[var(--zf-surface)] transition-transform ${compression ? 'translate-x-5' : 'translate-x-0.5'}`} /></div><span className="text-sm text-[var(--zf-ink)]">Enable compression</span></label>
          <button onClick={handleCreate} disabled={creating} className="zf-btn zf-btn-primary w-full">{creating ? 'Creating...' : 'Create Schedule'}</button>
        </div></Card>
      </div>

      <Card><div className="p-5">
        <h3 className="text-sm font-semibold text-[var(--zf-ink)] mb-4">Existing Schedules</h3>
        {schedules.length === 0 ? <p className="text-sm text-[var(--zf-muted)]">No backup schedules configured yet.</p> : (
          <div className="space-y-3">
            {schedules.map((sched) => (
              <div key={sched.id} className="flex flex-col sm:flex-row sm:items-center gap-3 bg-[var(--zf-canvas)] rounded-xl px-4 py-3 border border-[var(--zf-hairline)]/60">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2"><span className="text-sm font-medium text-[var(--zf-ink)]">{sched.name}</span><span className={`text-xs px-1.5 py-0.5 rounded border ${sched.enabled ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'}`}>{sched.enabled ? 'Enabled' : 'Disabled'}</span></div>
                  <div className="text-xs text-[var(--zf-muted)] mt-0.5"><span className="font-mono">{sched.cron}</span><span className="mx-1.5">-</span><span>{parseCronToHuman(sched.cron)}</span></div>
                </div>
                <div className="text-xs text-[var(--zf-muted)] sm:text-right"><div>VMs: {sched.vms?.length || 0}</div>{sched.next_run && <div>Next: {sched.next_run}</div>}</div>
              </div>
            ))}
          </div>
        )}
      </div></Card>
      </>
      )}
    </div>
  )
}
