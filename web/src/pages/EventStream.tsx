// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useRef, useCallback } from 'react'
import { Pause, Play, Trash2, Filter } from 'lucide-react'
import { useEventStream, type VMEventPayload } from '../hooks/useEventStream'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { hintsForError } from '../utils/daemonHints'
import { AppleTerminalFrame, TERM_LEVEL_COLOR } from '../components/AppleTerminalFrame'
import { AnsiText } from '../components/AnsiText'

interface StreamEvent {
  id: number
  timestamp: Date
  type: string
  source: string
  message: string
  level: 'info' | 'warning' | 'error' | 'debug'
}

function levelColor(level: string): string {
  return TERM_LEVEL_COLOR[level] || '#64d2ff'
}

function mapPayload(payload: VMEventPayload): StreamEvent {
  const levelRaw = payload.event_type.toLowerCase()
  const level: StreamEvent['level'] =
    levelRaw.includes('error') || levelRaw.includes('fail')
      ? 'error'
      : levelRaw.includes('warn')
        ? 'warning'
        : levelRaw.includes('debug')
          ? 'debug'
          : 'info'
  return {
    id: ++eventIdCounter,
    timestamp: new Date(payload.timestamp),
    type: payload.event_type,
    source: payload.vm_name,
    message: payload.detail || payload.event_type,
    level,
  }
}

let eventIdCounter = 0

export default function EventStream() {
  const [events, setEvents] = useState<StreamEvent[]>([])
  const [paused, setPaused] = useState(false)
  const [levelFilter, setLevelFilter] = useState<string>('all')
  const [connectionError, setConnectionError] = useState<string | null>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const pausedRef = useRef(false)

  useEffect(() => { pausedRef.current = paused }, [paused])

  const onEvent = useCallback((payload: VMEventPayload) => {
    if (pausedRef.current) return
    setEvents((prev) => [mapPayload(payload), ...prev].slice(0, 500))
    setConnectionError(null)
  }, [])

  const { connected } = useEventStream({ onEvent })

  useEffect(() => {
    if (!connected && events.length === 0) {
      setConnectionError('Connecting to event stream…')
    } else if (connected) {
      setConnectionError(null)
    }
  }, [connected, events.length])

  useEffect(() => {
    if (!paused && containerRef.current) {
      containerRef.current.scrollTop = 0
    }
  }, [events, paused])

  const filtered = events.filter((e) => levelFilter === 'all' || e.level === levelFilter)

  return (
    <div className="space-y-6">
      <PageHeader
        title="Event Stream"
        description="Live VM lifecycle events via authenticated SSE"
      />

      {connectionError && !connected && (
        <ErrorBanner title="Connection error" headline={connectionError} hints={hintsForError(connectionError)} />
      )}

      <div className="flex flex-wrap items-center gap-3">
        <span
          className={`inline-flex items-center gap-2 text-sm ${connected ? 'text-emerald-600' : 'text-amber-600'}`}
        >
          <span className={`w-2 h-2 rounded-full ${connected ? 'bg-emerald-500' : 'bg-amber-500 animate-pulse'}`} />
          {connected ? 'Connected' : 'Reconnecting…'}
        </span>
        <button
          type="button"
          onClick={() => setPaused(!paused)}
          className="zf-btn zf-btn-ghost zf-btn-sm"
        >
          {paused ? <Play className="w-4 h-4" /> : <Pause className="w-4 h-4" />}
          {paused ? 'Resume' : 'Pause'}
        </button>
        <button
          type="button"
          onClick={() => setEvents([])}
          className="zf-btn zf-btn-ghost zf-btn-sm"
        >
          <Trash2 className="w-4 h-4" />
          Clear
        </button>
        <div className="flex items-center gap-2 text-sm text-[var(--zf-muted)]">
          <Filter className="w-4 h-4" />
          <select
            value={levelFilter}
            onChange={(e) => setLevelFilter(e.target.value)}
            className="input-field text-sm py-1"
          >
            <option value="all">All levels</option>
            <option value="info">Info</option>
            <option value="warning">Warning</option>
            <option value="error">Error</option>
            <option value="debug">Debug</option>
          </select>
        </div>
      </div>

      <AppleTerminalFrame
        title="event stream — SSE"
        live={connected && !paused}
        bodyRef={containerRef}
        bodyClassName="max-h-[70vh] overflow-y-auto px-3 py-2"
        empty={filtered.length === 0}
        emptyMessage="Waiting for events…"
      >
        {filtered.map((ev) => (
          <div
            key={ev.id}
            className="flex gap-3 py-1 hover:bg-white/[0.04] rounded px-1 -mx-1"
          >
            <span className="text-white/30 shrink-0 tabular-nums text-xs">
              {ev.timestamp.toLocaleTimeString(undefined, { hour12: false })}
            </span>
            <span
              className="shrink-0 uppercase text-xs font-semibold w-14"
              style={{ color: levelColor(ev.level) }}
            >
              {ev.level}
            </span>
            <span className="text-[#ffd60a]/70 font-medium shrink-0">{ev.source}</span>
            <span className="min-w-0 break-words whitespace-pre-wrap">
              <AnsiText text={ev.message} />
            </span>
          </div>
        ))}
      </AppleTerminalFrame>
    </div>
  )
}
