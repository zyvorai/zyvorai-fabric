// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react'
import { ArrowRight, ArrowLeft, Check, Loader2 } from 'lucide-react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { hintsForError } from '../utils/daemonHints'

type WizardStep = 'source' | 'configure' | 'review'

export default function MigrationWizard() {
  const [step, setStep] = useState<WizardStep>('source')
  const [sourceType, setSourceType] = useState<'local' | 'remote'>('local')
  const [sourcePath, setSourcePath] = useState('')
  const [remoteHost, setRemoteHost] = useState('')
  const [vmName, setVmName] = useState('')
  const [targetFormat, setTargetFormat] = useState('qcow2')
  const [outputDir, setOutputDir] = useState('/var/lib/zyvor-fabricd/images')
  const [cpus, setCpus] = useState(2)
  const [memory, setMemory] = useState(2048)
  const [autoStart, setAutoStart] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [result, setResult] = useState<{ ok: boolean; message: string } | null>(null)

  const steps: { key: WizardStep; label: string }[] = [
    { key: 'source', label: 'Source' },
    { key: 'configure', label: 'Configure' },
    { key: 'review', label: 'Review & Submit' },
  ]

  const currentIdx = steps.findIndex(s => s.key === step)
  const canNext = step === 'source' ? (sourceType === 'local' ? !!sourcePath : !!remoteHost) : step === 'configure' ? !!vmName : true

  const handleSubmit = async () => {
    setSubmitting(true); setResult(null)
    try {
      const res = await apiFetch('/api/migrations', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ source_type: sourceType, source_path: sourcePath, remote_host: remoteHost, vm_name: vmName, target_format: targetFormat, output_dir: outputDir, cpus, memory, auto_start: autoStart }),
      })
      if (!res.ok) {
        const body = await res.json().catch(() => null)
        throw new Error(body?.error || formatHttpErrorBody(res.status, res.statusText, ''))
      }
      setResult({ ok: true, message: 'Migration submitted successfully!' })
    } catch (err) {
      setResult({ ok: false, message: formatUserError(err) })
    } finally { setSubmitting(false) }
  }

  return (
    <div className="max-w-3xl mx-auto space-y-6">
      <PageHeader title="Migration Wizard" description="Step-by-step VM migration" />

      {/* Progress */}
      <div className="flex items-center gap-0">
        {steps.map((s, idx) => (
          <div key={s.key} className="flex items-center flex-1">
            <div className={`w-8 h-8 rounded-full flex items-center justify-center text-xs font-bold ${idx < currentIdx ? 'bg-[var(--zf-success)] text-white' : idx === currentIdx ? 'bg-[var(--zf-link-fill)] text-white' : 'bg-[var(--zf-canvas)] text-[var(--zf-muted)]'}`}>
              {idx < currentIdx ? <Check className="w-4 h-4" /> : idx + 1}
            </div>
            <span className={`ml-2 text-xs font-medium ${idx <= currentIdx ? 'text-[var(--zf-ink)]' : 'text-[var(--zf-muted)]'}`}>{s.label}</span>
            {idx < steps.length - 1 && <div className={`flex-1 h-0.5 mx-3 ${idx < currentIdx ? 'bg-[var(--zf-success)]' : 'bg-[var(--zf-canvas)]'}`} />}
          </div>
        ))}
      </div>

      <div className="zf-panel p-6">
        {step === 'source' && (
          <div className="space-y-4">
            <h2 className="text-lg font-semibold text-[var(--zf-ink)]">Select Source</h2>
            <div className="flex gap-3">
              {(['local', 'remote'] as const).map(t => (
                <button key={t} onClick={() => setSourceType(t)} className={`px-4 py-2.5 rounded-lg text-sm font-medium transition-colors capitalize ${sourceType === t ? 'bg-[var(--zf-link-fill)] text-white' : 'bg-[var(--zf-canvas)] text-[var(--zf-ink)] hover:bg-[var(--zf-hairline)]'}`}>{t} {t === 'local' ? 'File' : 'Host'}</button>
              ))}
            </div>
            {sourceType === 'local' ? (
              <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Disk Image Path</label><input type="text" value={sourcePath} onChange={e => setSourcePath(e.target.value)} placeholder="/path/to/disk.vmdk" aria-label="Source path" className="input-field" /></div>
            ) : (
              <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Remote Host</label><input type="text" value={remoteHost} onChange={e => setRemoteHost(e.target.value)} placeholder="user@hostname:/path" aria-label="Remote host" className="input-field" /></div>
            )}
          </div>
        )}

        {step === 'configure' && (
          <div className="space-y-4">
            <h2 className="text-lg font-semibold text-[var(--zf-ink)]">Configure VM</h2>
            <div className="grid grid-cols-2 gap-4">
              <div><label className="block text-xs text-[var(--zf-muted)] mb-1">VM Name</label><input type="text" value={vmName} onChange={e => setVmName(e.target.value)} placeholder="my-vm" aria-label="VM name" className="input-field" /></div>
              <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Target Format</label><select value={targetFormat} onChange={e => setTargetFormat(e.target.value)} aria-label="Target format" className="input-field"><option value="qcow2">QCOW2</option><option value="raw">RAW</option><option value="vmdk">VMDK</option></select></div>
              <div><label className="block text-xs text-[var(--zf-muted)] mb-1">vCPUs</label><input type="number" value={cpus} onChange={e => setCpus(Number(e.target.value))} min={1} max={64} aria-label="vCPUs" className="input-field" /></div>
              <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Memory (MB)</label><input type="number" value={memory} onChange={e => setMemory(Number(e.target.value))} min={256} step={256} aria-label="Memory" className="input-field" /></div>
            </div>
            <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Output Directory</label><input type="text" value={outputDir} onChange={e => setOutputDir(e.target.value)} aria-label="Output directory" className="input-field font-mono" /></div>
            <label className="flex items-center gap-3 cursor-pointer"><input type="checkbox" checked={autoStart} onChange={e => setAutoStart(e.target.checked)} className="rounded border-[var(--zf-hairline)]" /><span className="text-sm text-[var(--zf-ink)]">Auto-start after migration</span></label>
          </div>
        )}

        {step === 'review' && (
          <div className="space-y-4">
            <h2 className="text-lg font-semibold text-[var(--zf-ink)]">Review & Submit</h2>
            <div className="bg-[var(--zf-canvas)] rounded-lg p-4 space-y-2 text-sm">
              <div className="flex justify-between"><span className="text-[var(--zf-muted)]">Source</span><span className="text-[var(--zf-ink)] font-mono">{sourceType === 'local' ? sourcePath : remoteHost}</span></div>
              <div className="flex justify-between"><span className="text-[var(--zf-muted)]">VM Name</span><span className="text-[var(--zf-ink)]">{vmName}</span></div>
              <div className="flex justify-between"><span className="text-[var(--zf-muted)]">Format</span><span className="text-[var(--zf-ink)]">{targetFormat.toUpperCase()}</span></div>
              <div className="flex justify-between"><span className="text-[var(--zf-muted)]">Resources</span><span className="text-[var(--zf-ink)]">{cpus} vCPU, {memory} MB</span></div>
              <div className="flex justify-between"><span className="text-[var(--zf-muted)]">Output</span><span className="text-[var(--zf-ink)] font-mono">{outputDir}</span></div>
              <div className="flex justify-between"><span className="text-[var(--zf-muted)]">Auto-start</span><span className="text-[var(--zf-ink)]">{autoStart ? 'Yes' : 'No'}</span></div>
            </div>
            {result?.ok && (
              <div className="p-3 rounded-lg text-sm text-emerald-700 bg-emerald-50 border border-emerald-200">{result.message}</div>
            )}
            {result && !result.ok && (
              <ErrorBanner title="Migration failed" headline={result.message} hints={hintsForError(result.message)} />
            )}
          </div>
        )}
      </div>

      <div className="flex items-center justify-between">
        <button onClick={() => setStep(steps[currentIdx - 1].key)} disabled={currentIdx === 0} title="Go back" className="zf-btn zf-btn-ghost"><ArrowLeft className="w-4 h-4" /> Back</button>
        {step === 'review' ? (
          <button onClick={handleSubmit} disabled={submitting || result?.ok} title="Submit migration" className="zf-btn zf-btn-primary">
            {submitting ? <Loader2 className="w-4 h-4 animate-spin" /> : <Check className="w-4 h-4" />} {submitting ? 'Submitting...' : 'Submit Migration'}
          </button>
        ) : (
          <button onClick={() => setStep(steps[currentIdx + 1].key)} disabled={!canNext} title="Next step" className="zf-btn zf-btn-primary">Next <ArrowRight className="w-4 h-4" /></button>
        )}
      </div>
    </div>
  )
}
