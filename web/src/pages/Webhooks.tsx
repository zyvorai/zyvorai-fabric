// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Bell, Plus, Trash2, Send, CheckCircle, XCircle, Loader2, Globe, ToggleLeft, ToggleRight } from 'lucide-react'
import { apiFetch } from '../api/client'
import PageLoadBanner from '../components/PageLoadBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody } from '../utils/apiError'
import { usePageLoader } from '../hooks/usePageLoader'
import { useToastContext } from '../contexts/ToastContext'
import { toastFailure } from '../utils/toastError'
import CopyButton from '../components/CopyButton'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'

interface Webhook {
  id: string
  url: string
  events: string[]
  type: string
  enabled: boolean
}

const availableEvents = ['vm.started', 'vm.stopped', 'vm.created', 'vm.deleted', 'backup.completed', 'backup.failed']
const webhookTypes = ['generic', 'slack', 'discord']

export default function Webhooks() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [webhooks, setWebhooks] = useState<Webhook[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load webhooks')
  const [showAdd, setShowAdd] = useState(false)
  const [testingId, setTestingId] = useState<string | null>(null)
  const [testResult, setTestResult] = useState<{ id: string; ok: boolean; msg: string } | null>(null)

  const [newUrl, setNewUrl] = useState('')
  const [newEvents, setNewEvents] = useState<string[]>([])
  const [newType, setNewType] = useState('generic')
  const [addError, setAddError] = useState('')
  const [adding, setAdding] = useState(false)

  const fetchWebhooks = useCallback(() => {
    return run(async () => {
      const res = await apiFetch('/api/webhooks')
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const data = await res.json()
      setWebhooks(Array.isArray(data) ? data : data.webhooks || [])
    })
  }, [run])

  useEffect(() => {
    void fetchWebhooks()
  }, [fetchWebhooks])

  const handleAdd = async () => {
    if (!newUrl.trim()) { setAddError('URL is required'); return }
    if (newEvents.length === 0) { setAddError('Select at least one event'); return }
    setAdding(true)
    setAddError('')
    try {
      const res = await apiFetch('/api/webhooks', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ url: newUrl.trim(), events: newEvents, type: newType, enabled: true }),
      })
      if (!res.ok) {
        const body = await res.json().catch(() => ({ error: 'Failed' }))
        throw new Error(body.error || `HTTP ${res.status}`)
      }
      setNewUrl('')
      setNewEvents([])
      setNewType('generic')
      setShowAdd(false)
      fetchWebhooks()
    } catch (err) {
      setAddError(err instanceof Error ? err.message : 'Failed to add webhook')
    } finally {
      setAdding(false)
    }
  }

  const handleDelete = async (id: string, url: string) => {
    const ok = await confirm('Delete Webhook', `Delete the webhook for ${url}? This cannot be undone.`, { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return

    try {
      const res = await apiFetch(`/api/webhooks/${id}`, { method: 'DELETE' })
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      setWebhooks((prev) => prev.filter((w) => w.id !== id))
      toast.success('Webhook deleted')
    } catch (err) {
      toastFailure(toast, 'Failed to delete webhook', err)
    }
  }

  const handleTest = async (id: string) => {
    setTestingId(id)
    setTestResult(null)
    try {
      const res = await apiFetch('/api/webhooks/test', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ webhook_id: id }),
      })
      setTestResult({ id, ok: res.ok, msg: res.ok ? 'Test delivered' : `Failed (${res.status})` })
    } catch (err) {
      setTestResult({ id, ok: false, msg: err instanceof Error ? err.message : 'Test failed' })
    } finally {
      setTestingId(null)
    }
  }

  const toggleEvent = (event: string) => {
    setNewEvents((prev) => prev.includes(event) ? prev.filter((e) => e !== event) : [...prev, event])
  }

  return (
    <div className="max-w-3xl mx-auto space-y-6">
      <PageHeader
        title="Webhook Configuration"
        onRefresh={() => void fetchWebhooks()}
        refreshing={loading}
        actions={
          <button
            onClick={() => setShowAdd(!showAdd)}
            className="zf-btn zf-btn-primary zf-btn-sm"
          >
            <Plus className="w-4 h-4" />
            Add Webhook
          </button>
        }
      />

      <PageLoadBanner title="Could not load webhooks" headline={loadError} onRetry={() => void fetchWebhooks()} />

      {showAdd && (
        <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-xl p-5 space-y-4">
          <h3 className="text-sm font-semibold text-[var(--zf-ink)]">New Webhook</h3>

          <div>
            <label className="block text-xs font-medium text-[var(--zf-muted)] mb-1.5">Webhook URL</label>
            <input
              type="url"
              value={newUrl}
              onChange={(e) => setNewUrl(e.target.value)}
              placeholder="https://hooks.example.com/webhook"
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] focus:outline-none focus:ring-2 focus:ring-[var(--zf-link)]/30 focus:border-[var(--zf-link)]"
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-[var(--zf-muted)] mb-1.5">Type</label>
            <div className="flex gap-2">
              {webhookTypes.map((t) => (
                <button
                  key={t}
                  onClick={() => setNewType(t)}
                  className={`px-3 py-1.5 text-xs font-medium rounded-lg border transition-colors capitalize ${
                    newType === t
                      ? 'bg-[var(--zf-ink)] text-white border-[var(--zf-ink)]'
                      : 'text-[var(--zf-muted)] bg-[var(--zf-surface)] border-[var(--zf-hairline)] hover:border-[var(--zf-hairline)]'
                  }`}
                >
                  {t}
                </button>
              ))}
            </div>
          </div>

          <div>
            <label className="block text-xs font-medium text-[var(--zf-muted)] mb-1.5">Events</label>
            <div className="flex flex-wrap gap-2">
              {availableEvents.map((ev) => (
                <label key={ev} className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={newEvents.includes(ev)}
                    onChange={() => toggleEvent(ev)}
                    className="rounded border-[var(--zf-hairline)] bg-[var(--zf-surface)] text-[var(--zf-link)] focus:ring-[var(--zf-link)]/30"
                  />
                  <span className="text-sm text-[var(--zf-ink)] font-mono">{ev}</span>
                </label>
              ))}
            </div>
          </div>

          {addError && (
            <p className="text-sm text-red-600">{addError}</p>
          )}

          <div className="flex gap-2">
            <button
              onClick={handleAdd}
              disabled={adding}
              className="zf-btn zf-btn-primary"
            >
              {adding ? <Loader2 className="w-4 h-4 animate-spin" /> : <Plus className="w-4 h-4" />}
              {adding ? 'Adding...' : 'Add'}
            </button>
            <button
              onClick={() => { setShowAdd(false); setAddError('') }}
              className="zf-btn zf-btn-ghost"
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {loading ? (
        <div className="flex items-center justify-center py-16">
          <div className="w-6 h-6 border-2 border-[var(--zf-ink)] border-t-transparent rounded-full animate-spin" />
        </div>
      ) : webhooks.length === 0 ? (
        <div className="text-center py-16 text-[var(--zf-muted)]">
          <Bell className="w-10 h-10 mx-auto mb-3 opacity-50" />
          <p className="text-sm">No webhooks configured yet.</p>
        </div>
      ) : (
        <div className="space-y-3">
          {webhooks.map((wh) => (
            <div key={wh.id} className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-xl p-4">
              <div className="flex items-start justify-between gap-4">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-1.5">
                    <Globe className="w-4 h-4 text-[var(--zf-muted)] flex-shrink-0" />
                    <span className="text-sm text-[var(--zf-ink)] font-mono truncate">{wh.url}</span>
                    <CopyButton text={wh.url} iconOnly successMessage="Webhook URL copied" />
                  </div>
                  <div className="flex items-center gap-3 flex-wrap">
                    <span className="text-xs px-2 py-0.5 rounded capitalize bg-[var(--zf-canvas)] text-[var(--zf-muted)] border border-[var(--zf-hairline)]">
                      {wh.type || 'generic'}
                    </span>
                    {wh.events?.map((ev) => (
                      <span key={ev} className="text-xs px-2 py-0.5 bg-[var(--zf-surface)] text-[var(--zf-muted)] rounded font-mono">{ev}</span>
                    ))}
                    <div className="flex items-center gap-1 text-xs">
                      {wh.enabled ? (
                        <><ToggleRight className="w-4 h-4 text-emerald-600" /><span className="text-emerald-600">Enabled</span></>
                      ) : (
                        <><ToggleLeft className="w-4 h-4 text-[var(--zf-muted)]" /><span className="text-[var(--zf-muted)]">Disabled</span></>
                      )}
                    </div>
                  </div>
                  {testResult?.id === wh.id && (
                    <div className={`mt-2 flex items-center gap-1.5 text-xs ${testResult.ok ? 'text-emerald-600' : 'text-red-600'}`}>
                      {testResult.ok ? <CheckCircle className="w-3.5 h-3.5" /> : <XCircle className="w-3.5 h-3.5" />}
                      {testResult.msg}
                    </div>
                  )}
                </div>

                <div className="flex items-center gap-2 flex-shrink-0">
                  <button
                    onClick={() => handleTest(wh.id)}
                    disabled={testingId === wh.id}
                    className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs bg-[var(--zf-surface)] hover:bg-black/[0.04] text-[var(--zf-ink)] rounded-lg transition-colors border border-[var(--zf-hairline)]"
                    title="Test webhook"
                  >
                    {testingId === wh.id ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Send className="w-3.5 h-3.5" />}
                    Test
                  </button>
                  <button
                    onClick={() => handleDelete(wh.id, wh.url)}
                    className="p-1.5 text-[var(--zf-muted)] hover:text-red-600 hover:bg-red-50 rounded-lg transition-colors"
                    title="Delete webhook"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {confirmState && (
        <ConfirmDialog
          title={confirmState.title}
          message={confirmState.message}
          confirmLabel={confirmState.confirmLabel}
          variant={confirmState.variant}
          onConfirm={confirmState.onConfirm}
          onCancel={cancel}
        />
      )}
    </div>
  )
}
