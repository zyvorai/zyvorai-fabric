// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Package, Power, Loader2 } from 'lucide-react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface Plugin {
  name: string
  version: string
  description: string
  enabled: boolean
  status: 'running' | 'stopped' | 'error'
  author?: string
  type: string
}

function statusColor(status: string): string {
  switch (status) {
    case 'running': return 'text-emerald-700 bg-emerald-50 border-emerald-200'
    case 'stopped': return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
    case 'error': return 'text-red-700 bg-red-50 border-red-200'
    default: return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
  }
}

function typeColor(_type: string): string {
  return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
}

export default function PluginManager() {
  const toast = useToastContext()
  const [plugins, setPlugins] = useState<Plugin[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [toggling, setToggling] = useState<string | null>(null)

  const fetchPlugins = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const res = await apiFetch('/api/plugins')
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const data = await res.json()
      setPlugins(Array.isArray(data) ? data : data.plugins || [])
    } catch (err) {
      setLoadError(formatUserError(err))
      toastFailure(toast, 'Failed to load plugins', err)
    } finally {
      setLoading(false)
    }
  }, [toast])

  useEffect(() => { fetchPlugins() }, [fetchPlugins])

  const togglePlugin = async (name: string, enabled: boolean) => {
    setToggling(name)
    try {
      const res = await apiFetch(`/api/plugins/${name}/${enabled ? 'disable' : 'enable'}`, { method: 'POST' })
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      setPlugins(prev => prev.map(p => p.name === name ? { ...p, enabled: !enabled, status: !enabled ? 'running' : 'stopped' } : p))
      setActionError(null)
    } catch (err) {
      setActionError(formatUserError(err))
      toastFailure(toast, `Failed to toggle ${name}`, err)
    } finally { setToggling(null) }
  }

  const runningCount = plugins.filter(p => p.status === 'running').length
  const errorCount = plugins.filter(p => p.status === 'error').length

  if (loading && plugins.length === 0 && !loadError) {
    return (
      <div className="space-y-6">
        <PageHeader title="Plugin Manager" description="Manage server extensions and integrations" />
        <div className="flex items-center justify-center h-64 text-[var(--zf-muted)]">
          <div className="animate-spin w-6 h-6 border-2 border-[var(--zf-ink)] border-t-transparent rounded-full mr-3" />
          Loading plugins…
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title="Plugin Manager"
        description="Manage server extensions and integrations"
        onRefresh={fetchPlugins}
        refreshing={loading}
      />

      {loadError && (
        <ErrorBanner
          title="Could not load plugins"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={fetchPlugins}
        />
      )}
      {actionError && (
        <div className="bg-amber-50 rounded-lg border border-amber-200 px-4 py-2 text-sm text-amber-800">{actionError}</div>
      )}

      {!loadError && (
      <>
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        <div className="stat-card-blue rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{plugins.length}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Total Plugins</div>
        </div>
        <div className="stat-card-green rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow-green transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{runningCount}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Running</div>
        </div>
        <div className="stat-card-red rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{errorCount}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Errors</div>
        </div>
        <div className="stat-card-purple rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow-purple transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{new Set(plugins.map(p => p.type)).size}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Types</div>
        </div>
      </div>

      {plugins.length === 0 ? (
        <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] text-center text-[var(--zf-muted)]"><Package className="w-10 h-10 mx-auto mb-3 opacity-50" /><p className="text-sm">No plugins installed</p></div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {plugins.map(plugin => (
            <div key={plugin.name} className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-5 card-glow transition-all hover:scale-[1.01]">
              <div className="flex items-start justify-between mb-3">
                <div className="flex items-center gap-3">
                  <div className="icon-tile icon-tile-md">
                    <Package className="w-5 h-5 text-[var(--zf-ink)]" />
                  </div>
                  <div>
                    <h3 className="text-sm font-semibold text-[var(--zf-ink)]">{plugin.name}</h3>
                    <div className="flex items-center gap-2 mt-0.5">
                      <span className="text-xs text-[var(--zf-muted)]">v{plugin.version}</span>
                      <span className={`px-1.5 py-0.5 rounded text-xs font-medium border capitalize ${typeColor(plugin.type)}`}>{plugin.type}</span>
                    </div>
                  </div>
                </div>
                <span className={`px-2 py-0.5 rounded-full text-xs font-medium border ${statusColor(plugin.status)}`}>{plugin.status}</span>
              </div>

              <p className="text-xs text-[var(--zf-muted)] mb-3">{plugin.description}</p>
              {plugin.author && <p className="text-xs text-[var(--zf-muted)] mb-3">by {plugin.author}</p>}

              <div className="flex items-center justify-between pt-3 border-t border-[var(--zf-hairline)]/60">
                <button onClick={() => togglePlugin(plugin.name, plugin.enabled)} disabled={toggling === plugin.name}
                  className={`flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg transition-colors border ${plugin.enabled ? 'text-red-700 bg-red-50 border-red-200 hover:bg-red-100' : 'text-emerald-700 bg-emerald-50 border-emerald-200 hover:bg-emerald-100'}`}>
                  {toggling === plugin.name ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Power className="w-3.5 h-3.5" />}
                  {plugin.enabled ? 'Disable' : 'Enable'}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
      </>
      )}
    </div>
  )
}
