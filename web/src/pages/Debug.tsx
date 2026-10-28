// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback, useRef } from 'react'
import { apiFetch } from '../api/client'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { AppleTerminalFrame } from '../components/AppleTerminalFrame'
import { AnsiText } from '../components/AnsiText'

type PanelKey = 'top' | 'iostat' | 'vmstat' | 'netstat'

const PANELS: { key: PanelKey; label: string; endpoint: string }[] = [
  { key: 'top', label: 'Top', endpoint: '/api/system/debug/top' },
  { key: 'iostat', label: 'IOStat', endpoint: '/api/system/debug/iostat' },
  { key: 'vmstat', label: 'VMStat', endpoint: '/api/system/debug/vmstat' },
  { key: 'netstat', label: 'NetStat', endpoint: '/api/system/debug/netstat' },
]

interface PanelState {
  lines: string[]
  loading: boolean
  error: string | null
}

export default function Debug() {
  const [panels, setPanels] = useState<Record<PanelKey, PanelState>>({
    top: { lines: [], loading: false, error: null },
    iostat: { lines: [], loading: false, error: null },
    vmstat: { lines: [], loading: false, error: null },
    netstat: { lines: [], loading: false, error: null },
  })
  const [autoRefresh, setAutoRefresh] = useState(false)
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const fetchPanel = useCallback(async (key: PanelKey) => {
    const panel = PANELS.find((p) => p.key === key)!
    setPanels((prev) => ({ ...prev, [key]: { ...prev[key], loading: true, error: null } }))
    try {
      const res = await apiFetch(panel.endpoint)
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const data = await res.json()
      const lines = Array.isArray(data.lines) ? data.lines : []
      setPanels((prev) => ({ ...prev, [key]: { lines, loading: false, error: null } }))
    } catch (err) {
      setPanels((prev) => ({ ...prev, [key]: { ...prev[key], loading: false, error: formatUserError(err) } }))
    }
  }, [])

  const fetchAll = useCallback(() => {
    PANELS.forEach((p) => fetchPanel(p.key))
  }, [fetchPanel])

  useEffect(() => {
    if (autoRefresh) {
      fetchAll()
      intervalRef.current = setInterval(fetchAll, 3000)
    } else if (intervalRef.current) {
      clearInterval(intervalRef.current)
      intervalRef.current = null
    }
    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current)
        intervalRef.current = null
      }
    }
  }, [autoRefresh, fetchAll])

  return (
    <div className="space-y-6">
      <PageHeader
        title="Debug"
        description="System diagnostic tools"
        onRefresh={fetchAll}
        actions={
        <div className="flex items-center gap-3">
          <button
            onClick={fetchAll}
            title="Refresh all panels"
            className="zf-btn zf-btn-primary zf-btn-sm"
          >
            Refresh All
          </button>
          <label className="flex items-center gap-2 cursor-pointer">
            <input type="checkbox" checked={autoRefresh} onChange={() => setAutoRefresh((v) => !v)} className="sr-only" />
            <div className={`w-10 h-5 rounded-full transition-colors relative ${autoRefresh ? 'bg-[var(--zf-ink)]' : 'bg-[var(--zf-hairline)]'}`}>
              <div className={`w-4 h-4 rounded-full bg-[var(--zf-surface)] absolute top-0.5 transition-transform ${autoRefresh ? 'translate-x-5' : 'translate-x-0.5'}`} />
            </div>
            <span className="text-sm text-[var(--zf-ink)]">Auto-refresh (3s)</span>
          </label>
        </div>
        }
      />

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {PANELS.map((panel) => {
          const state = panels[panel.key]
          return (
            <div key={panel.key} className="zf-panel-muted overflow-hidden">
              <div className="px-4 py-3 border-b border-[var(--zf-hairline)] flex items-center justify-between">
                <h3 className="text-sm font-semibold text-[var(--zf-ink)]">{panel.label}</h3>
                <button
                  onClick={() => fetchPanel(panel.key)}
                  disabled={state.loading}
                  className="zf-btn zf-btn-ghost zf-btn-sm"
                >
                  {state.loading ? 'Loading...' : 'Refresh'}
                </button>
              </div>
              <div className="p-2">
                {state.error ? (
                  <div className="p-4 text-center text-[var(--zf-danger)] text-sm">{state.error}</div>
                ) : (
                  <AppleTerminalFrame
                    title={panel.label.toLowerCase()}
                    bodyClassName="max-h-80 overflow-auto px-3 py-2 whitespace-pre"
                    empty={state.lines.length === 0 && !state.loading}
                    emptyMessage={state.loading ? 'Loading…' : 'Click Refresh to load data'}
                  >
                    <AnsiText text={state.lines.join('\n')} />
                  </AppleTerminalFrame>
                )}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}
