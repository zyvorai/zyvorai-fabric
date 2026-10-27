// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useRef, useState } from 'react'
import { Bot, Plus, Upload, X } from 'lucide-react'
import { AgentManifest, deployAgent } from '../../api/agents'
import { Modal } from '../../components/ui'
import { useToastContext } from '../../contexts/ToastContext'
import { toastFailure } from '../../utils/toastError'
import { formatBytes } from '../../utils/format'

interface DeployAgentModalProps {
  open: boolean
  onClose: () => void
  onDeployed: () => void
}

function readFileAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      const result = reader.result as string
      const comma = result.indexOf(',')
      resolve(comma >= 0 ? result.slice(comma + 1) : result)
    }
    reader.onerror = () => reject(reader.error ?? new Error('Failed to read file'))
    reader.readAsDataURL(file)
  })
}

export default function DeployAgentModal({ open, onClose, onDeployed }: DeployAgentModalProps) {
  const toast = useToastContext()
  const fileInputRef = useRef<HTMLInputElement>(null)

  const [name, setName] = useState('')
  const [template, setTemplate] = useState('')
  const [file, setFile] = useState<File | null>(null)
  const [credentials, setCredentials] = useState<string[]>([])
  const [egressHosts, setEgressHosts] = useState<string[]>([])
  const [allowPrivateNetworks, setAllowPrivateNetworks] = useState(false)
  const [runtimePort, setRuntimePort] = useState('8080')
  const [ttlSeconds, setTtlSeconds] = useState('')
  const [maxConcurrentSessions, setMaxConcurrentSessions] = useState('')
  const [idleHibernateSeconds, setIdleHibernateSeconds] = useState('')
  const [warmPoolSize, setWarmPoolSize] = useState('0')
  const [perUserHome, setPerUserHome] = useState(true)
  const [submitting, setSubmitting] = useState(false)
  const [formError, setFormError] = useState<string | null>(null)

  const reset = () => {
    setName('')
    setTemplate('')
    setFile(null)
    setCredentials([])
    setEgressHosts([])
    setAllowPrivateNetworks(false)
    setRuntimePort('8080')
    setTtlSeconds('')
    setMaxConcurrentSessions('')
    setIdleHibernateSeconds('')
    setWarmPoolSize('0')
    setPerUserHome(true)
    setFormError(null)
  }

  const handleClose = () => {
    if (submitting) return
    reset()
    onClose()
  }

  const addCredentialRow = () => setCredentials((prev) => [...prev, ''])
  const updateCredentialRow = (i: number, value: string) =>
    setCredentials((prev) => prev.map((c, idx) => (idx === i ? value : c)))
  const removeCredentialRow = (i: number) => setCredentials((prev) => prev.filter((_, idx) => idx !== i))

  const addEgressHostRow = () => setEgressHosts((prev) => [...prev, ''])
  const updateEgressHostRow = (i: number, value: string) =>
    setEgressHosts((prev) => prev.map((h, idx) => (idx === i ? value : h)))
  const removeEgressHostRow = (i: number) => setEgressHosts((prev) => prev.filter((_, idx) => idx !== i))

  const parseOptionalNumber = (label: string, value: string): number | undefined | null => {
    if (!value.trim()) return undefined
    const n = Number(value)
    if (!Number.isFinite(n)) throw new Error(`${label} must be a number`)
    return n
  }

  const validate = (): string | null => {
    if (!name.trim()) return 'Name is required'
    if (!template.trim()) return 'Template is required'
    if (!file) return 'Select a bundle file to upload'
    const port = Number(runtimePort)
    if (!Number.isFinite(port) || port <= 0) return 'Runtime port must be a positive number'
    const pool = Number(warmPoolSize)
    if (!Number.isInteger(pool) || pool < 0 || pool > 64) return 'Warm pool size must be an integer between 0 and 64'
    for (const [label, value] of [
      ['TTL (seconds)', ttlSeconds],
      ['Max concurrent sessions', maxConcurrentSessions],
      ['Idle hibernate (seconds)', idleHibernateSeconds],
    ] as const) {
      if (value.trim() && !Number.isFinite(Number(value))) return `${label} must be a number`
    }
    return null
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    const err = validate()
    if (err) {
      setFormError(err)
      return
    }
    setFormError(null)
    setSubmitting(true)
    try {
      const bundle_base64 = await readFileAsBase64(file!)
      const manifest: AgentManifest = {
        template: template.trim(),
        credentials: credentials.map((c) => c.trim()).filter(Boolean),
        egress_allow_hosts: egressHosts.map((h) => h.trim()).filter(Boolean),
        allow_private_networks: allowPrivateNetworks,
        runtime_port: Number(runtimePort),
        ttl_seconds: parseOptionalNumber('TTL', ttlSeconds) ?? undefined,
        max_concurrent_sessions: parseOptionalNumber('Max concurrent sessions', maxConcurrentSessions) ?? undefined,
        idle_hibernate_seconds: parseOptionalNumber('Idle hibernate', idleHibernateSeconds) ?? undefined,
        warm_pool_size: Number(warmPoolSize),
        ...(perUserHome ? { home_volume: { per_user: true } } : {}),
      }
      await deployAgent({ name: name.trim(), bundle_base64, manifest })
      toast.success(`Deployed ${name.trim()}`)
      reset()
      onDeployed()
    } catch (e2) {
      toastFailure(toast, 'Failed to deploy agent', e2)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Modal open={open} onClose={handleClose} className="max-w-lg">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          <div className="icon-tile icon-tile-md icon-tile-blue">
            <Bot className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-lg font-bold text-[var(--zf-ink)]">Deploy agent</h2>
            <p className="text-xs text-[var(--zf-muted)]">Upload a bundle built with `fabric-agent build`</p>
          </div>
        </div>
        {!submitting && (
          <button
            type="button"
            onClick={handleClose}
            className="p-2 hover:bg-black/[0.04] rounded transition text-[var(--zf-muted)] hover:text-[var(--zf-ink)]"
          >
            <X className="w-4 h-4" />
          </button>
        )}
      </div>

      <form onSubmit={handleSubmit} className="space-y-4 max-h-[70vh] overflow-y-auto pr-1">
        {formError && (
          <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">{formError}</div>
        )}

        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Name</label>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="research-agent"
            disabled={submitting}
            className="input-field font-mono text-sm"
            required
            autoFocus
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Template</label>
          <input
            type="text"
            value={template}
            onChange={(e) => setTemplate(e.target.value)}
            placeholder="node22-agent"
            disabled={submitting}
            className="input-field font-mono text-sm"
            required
          />
          <p className="text-xs text-[var(--zf-muted)] mt-1">
            FluxVM sandbox template name (registered separately from VM golden-image templates).
          </p>
        </div>

        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Bundle file</label>
          <input
            ref={fileInputRef}
            type="file"
            accept=".mjs,.js"
            className="hidden"
            onChange={(e) => setFile(e.target.files?.[0] ?? null)}
          />
          <button
            type="button"
            onClick={() => fileInputRef.current?.click()}
            disabled={submitting}
            className="zf-btn zf-btn-ghost w-full justify-start"
          >
            <Upload className="w-3.5 h-3.5" />
            {file ? `${file.name} (${formatBytes(file.size)})` : 'Choose bundle file…'}
          </button>
          <p className="text-xs text-[var(--zf-muted)] mt-1">
            Build one with <code>fabric-agent build agent.ts --out agent.bundle.mjs</code>. Keep it well under
            ~1.5MB — the request body is capped at 2MB.
          </p>
        </div>

        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Credentials</label>
          <div className="space-y-2">
            {credentials.map((c, i) => (
              <div key={i} className="flex items-center gap-2">
                <input
                  type="text"
                  value={c}
                  onChange={(e) => updateCredentialRow(i, e.target.value)}
                  placeholder="anthropic"
                  disabled={submitting}
                  className="input-field text-sm flex-1"
                />
                <button
                  type="button"
                  onClick={() => removeCredentialRow(i)}
                  disabled={submitting}
                  className="p-1.5 rounded-md text-[var(--zf-muted)] hover:text-red-600 hover:bg-red-50 transition-colors"
                  title="Remove"
                >
                  <X className="w-3.5 h-3.5" />
                </button>
              </div>
            ))}
          </div>
          <button
            type="button"
            onClick={addCredentialRow}
            disabled={submitting}
            className="mt-2 flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-[var(--zf-surface)] border border-[var(--zf-hairline)] text-[var(--zf-ink)] hover:border-[var(--zf-hairline)] transition-colors"
          >
            <Plus className="w-3.5 h-3.5" />
            Add credential
          </button>
        </div>

        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Egress allow hosts</label>
          <div className="space-y-2">
            {egressHosts.map((h, i) => (
              <div key={i} className="flex items-center gap-2">
                <input
                  type="text"
                  value={h}
                  onChange={(e) => updateEgressHostRow(i, e.target.value)}
                  placeholder="api.anthropic.com"
                  disabled={submitting}
                  className="input-field text-sm flex-1"
                />
                <button
                  type="button"
                  onClick={() => removeEgressHostRow(i)}
                  disabled={submitting}
                  className="p-1.5 rounded-md text-[var(--zf-muted)] hover:text-red-600 hover:bg-red-50 transition-colors"
                  title="Remove"
                >
                  <X className="w-3.5 h-3.5" />
                </button>
              </div>
            ))}
          </div>
          <button
            type="button"
            onClick={addEgressHostRow}
            disabled={submitting}
            className="mt-2 flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-[var(--zf-surface)] border border-[var(--zf-hairline)] text-[var(--zf-ink)] hover:border-[var(--zf-hairline)] transition-colors"
          >
            <Plus className="w-3.5 h-3.5" />
            Add egress host
          </button>
        </div>

        <label className="flex items-center gap-2 text-sm text-[var(--zf-ink)]">
          <input
            type="checkbox"
            checked={allowPrivateNetworks}
            onChange={(e) => setAllowPrivateNetworks(e.target.checked)}
            disabled={submitting}
          />
          Allow private/link-local egress destinations
        </label>

        <label className="flex items-center gap-2 text-sm text-[var(--zf-ink)]">
          <input
            type="checkbox"
            checked={perUserHome}
            onChange={(e) => setPerUserHome(e.target.checked)}
            disabled={submitting}
          />
          Per-user home volume (multi-tenant: each user gets their own disk)
        </label>

        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Runtime port</label>
            <input
              type="number"
              min={1}
              value={runtimePort}
              onChange={(e) => setRuntimePort(e.target.value)}
              disabled={submitting}
              className="input-field text-sm"
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Warm pool size</label>
            <input
              type="number"
              min={0}
              max={64}
              value={warmPoolSize}
              onChange={(e) => setWarmPoolSize(e.target.value)}
              disabled={submitting}
              className="input-field text-sm"
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">
              TTL (seconds) <span className="font-normal text-[var(--zf-muted)]">optional</span>
            </label>
            <input
              type="number"
              min={1}
              value={ttlSeconds}
              onChange={(e) => setTtlSeconds(e.target.value)}
              disabled={submitting}
              className="input-field text-sm"
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">
              Max concurrent <span className="font-normal text-[var(--zf-muted)]">optional</span>
            </label>
            <input
              type="number"
              min={1}
              value={maxConcurrentSessions}
              onChange={(e) => setMaxConcurrentSessions(e.target.value)}
              disabled={submitting}
              className="input-field text-sm"
            />
          </div>
          <div className="col-span-2">
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">
              Idle hibernate (seconds) <span className="font-normal text-[var(--zf-muted)]">optional</span>
            </label>
            <input
              type="number"
              min={1}
              value={idleHibernateSeconds}
              onChange={(e) => setIdleHibernateSeconds(e.target.value)}
              disabled={submitting}
              className="input-field text-sm"
            />
          </div>
        </div>

        <div className="flex justify-end gap-2 pt-2">
          <button type="button" onClick={handleClose} disabled={submitting} className="zf-btn zf-btn-ghost">
            Cancel
          </button>
          <button type="submit" disabled={submitting} className="zf-btn zf-btn-primary">
            {submitting && <div className="w-3.5 h-3.5 border-2 border-white/30 border-t-white rounded-full animate-spin" />}
            {submitting ? 'Deploying…' : 'Deploy'}
          </button>
        </div>
      </form>
    </Modal>
  )
}
