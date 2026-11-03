// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback, useRef } from 'react'
import { X } from 'lucide-react'
import { apiFetch } from '../api/client'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { useToastContext } from '../contexts/ToastContext'
import { useEventStream, type VMEventPayload } from '../hooks/useEventStream'

interface Notification {
  id: string
  type: 'vm_started' | 'vm_stopped' | 'alert' | 'warning'
  message: string
  detail?: string
  timestamp: Date
  read: boolean
}

const typeConfig: Record<Notification['type'], { bg: string; border: string; icon: string; label: string }> = {
  vm_started: { bg: 'bg-emerald-50', border: 'border-emerald-200', icon: '\u2713', label: 'VM Started' },
  vm_stopped: { bg: 'bg-red-50', border: 'border-red-200', icon: '\u2717', label: 'VM Stopped' },
  alert: { bg: 'bg-amber-50', border: 'border-amber-200', icon: '\u26A0', label: 'Alert' },
  warning: { bg: 'bg-blue-50', border: 'border-blue-100', icon: '\u24D8', label: 'System Warning' },
}

const typeTextColor: Record<Notification['type'], string> = {
  vm_started: 'text-emerald-700',
  vm_stopped: 'text-red-700',
  alert: 'text-amber-800',
  warning: 'text-[var(--zf-link)]',
}

function formatTimeAgo(date: Date): string {
  const seconds = Math.floor((Date.now() - date.getTime()) / 1000)
  if (seconds < 60) return 'just now'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}

let nextId = 1

export default function NotificationCenter() {
  const toast = useToastContext()
  const [notifications, setNotifications] = useState<Notification[]>([])
  const [pollError, setPollError] = useState<string | null>(null)
  const knownAlertsRef = useRef<Set<string>>(new Set())
  const initialLoadRef = useRef(true)

  const addNotification = useCallback(
    (type: Notification['type'], message: string, detail?: string) => {
      const n: Notification = { id: `notif-${nextId++}`, type, message, detail, timestamp: new Date(), read: false }
      setNotifications((prev) => [n, ...prev].slice(0, 200))
    },
    []
  )

  useEffect(() => {
    const poll = async () => {
      try {
        const resp = await apiFetch('/api/system/alerts')
        if (!resp.ok) {
          const body = await resp.text()
          throw new Error(formatHttpErrorBody(resp.status, resp.statusText, body))
        }
        const data = await resp.json()
        setPollError(null)
        const alerts: any[] = data.alerts || (Array.isArray(data) ? data : [])
        for (const a of alerts) {
          const key = a.id || a.name || a.message || ''
          if (!key || knownAlertsRef.current.has(key)) continue
          knownAlertsRef.current.add(key)
          if (!initialLoadRef.current) {
            addNotification(a.severity === 'warning' ? 'warning' : 'alert', a.name || 'Alert fired', a.message)
          }
        }
      } catch (err) {
        const msg = formatUserError(err)
        setPollError((prev) => {
          if (!prev) toastFailure(toast, 'Failed to load alerts', err)
          return msg
        })
      }
      initialLoadRef.current = false
    }
    poll()
    const interval = setInterval(poll, 10000)
    return () => clearInterval(interval)
  }, [addNotification, toast])

  const onVMEvent = useCallback(
    (event: VMEventPayload) => {
      if (event.event_type === 'started') {
        addNotification('vm_started', `${event.vm_name} started`, event.detail)
      } else if (event.event_type === 'stopped') {
        addNotification('vm_stopped', `${event.vm_name} stopped`, event.detail)
      }
    },
    [addNotification]
  )
  useEventStream({ onEvent: onVMEvent })

  const unreadCount = notifications.filter((n) => !n.read).length

  const markAsRead = (id: string) => {
    setNotifications((prev) => prev.map((n) => (n.id === id ? { ...n, read: true } : n)))
  }

  const dismiss = (id: string) => {
    setNotifications((prev) => prev.filter((n) => n.id !== id))
  }

  const clearAll = () => setNotifications([])

  return (
    <div className="space-y-6">
      <PageHeader
        title="Notification Center"
        description="VM events, alerts, and system warnings"
        actions={
          notifications.length > 0 ? (
            <button onClick={clearAll} className="px-3 py-1.5 rounded-lg text-xs font-medium text-[var(--zf-muted)] hover:text-[var(--zf-ink)] hover:bg-[var(--zf-canvas)] transition-colors">
              Clear All
            </button>
          ) : undefined
        }
      />
      {unreadCount > 0 && (
        <div className="text-xs text-[var(--zf-muted)]">{unreadCount} unread</div>
      )}
      {pollError && (
        <div className="bg-amber-50 rounded-lg border border-amber-200 px-4 py-2 text-xs text-amber-800">
          {pollError} — alert polling paused until next retry
        </div>
      )}

      <div className="flex gap-3 flex-wrap">
        {(['vm_started', 'vm_stopped', 'alert', 'warning'] as Notification['type'][]).map((type) => {
          const count = notifications.filter((n) => n.type === type).length
          const cfg = typeConfig[type]
          return (
            <div key={type} className={`${cfg.bg} border ${cfg.border} rounded-lg px-3 py-1.5 text-xs font-medium ${typeTextColor[type]}`}>
              {cfg.icon} {cfg.label}: {count}
            </div>
          )
        })}
      </div>

      {notifications.length === 0 ? (
        <div className="zf-panel-muted p-12 text-center">
          <p className="text-[var(--zf-muted)] text-sm">No notifications yet.</p>
          <p className="text-[var(--zf-muted)] text-xs mt-1">VM events, alerts, and warnings will appear here.</p>
        </div>
      ) : (
        <div className="space-y-2">
          {notifications.map((n) => {
            const cfg = typeConfig[n.type]
            return (
              <div key={n.id} onClick={() => markAsRead(n.id)}
                className={`${cfg.bg} border ${cfg.border} rounded-xl px-4 py-3 flex items-start gap-3 cursor-pointer transition-opacity ${n.read ? 'opacity-60' : 'opacity-100'}`}>
                <span className={`text-lg mt-0.5 ${typeTextColor[n.type]}`}>{cfg.icon}</span>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className={`text-xs font-semibold ${typeTextColor[n.type]}`}>{cfg.label}</span>
                    {!n.read && <span className="w-1.5 h-1.5 rounded-full bg-[var(--zf-link-fill)]" />}
                  </div>
                  <p className="text-sm text-[var(--zf-ink)] mt-0.5">{n.message}</p>
                  {n.detail && <p className="text-xs text-[var(--zf-muted)] mt-0.5 truncate">{n.detail}</p>}
                  <p className="text-xs text-[var(--zf-muted)] mt-1">{formatTimeAgo(n.timestamp)}</p>
                </div>
                <button onClick={(e) => { e.stopPropagation(); dismiss(n.id) }}
                  className="text-[var(--zf-muted)] hover:text-[var(--zf-muted)] transition-colors p-1" title="Dismiss">
                  <X className="w-4 h-4" />
                </button>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
