// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { useEffect, useRef, useState } from 'react'
import { getToken } from '../api/client'

export interface VMEventPayload {
  id: string
  event_type: string
  vm_name: string
  detail?: string
  timestamp: string
}

interface UseEventStreamOptions {
  url?: string
  enabled?: boolean
  onEvent?: (event: VMEventPayload) => void
}

function parseSseBlock(block: string): { event?: string; data?: string } {
  let event: string | undefined
  let data: string | undefined
  for (const line of block.split('\n')) {
    if (line.startsWith('event:')) {
      event = line.slice(6).trim()
    } else if (line.startsWith('data:')) {
      data = (data ? `${data}\n` : '') + line.slice(5).trim()
    }
  }
  return { event, data }
}

/**
 * Authenticated SSE client for `/api/events/stream` (JWT in Authorization header).
 */
export function useEventStream({
  url = '/api/events/stream',
  enabled = true,
  onEvent,
}: UseEventStreamOptions = {}) {
  const [connected, setConnected] = useState(false)
  const onEventRef = useRef(onEvent)
  onEventRef.current = onEvent

  useEffect(() => {
    if (!enabled) {
      setConnected(false)
      return
    }

    let closed = false
    let attempt = 0
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null
    let abort: AbortController | null = null

    const scheduleReconnect = () => {
      if (closed) return
      attempt = Math.min(attempt + 1, 6)
      const delay = Math.min(15000, 500 * 2 ** attempt) + Math.floor(Math.random() * 250)
      reconnectTimer = setTimeout(connect, delay)
    }

    const connect = async () => {
      if (closed) return
      abort?.abort()
      abort = new AbortController()
      const token = getToken()
      const headers: Record<string, string> = { Accept: 'text/event-stream' }
      if (token) headers.Authorization = `Bearer ${token}`

      try {
        const res = await fetch(url, { headers, signal: abort.signal })
        if (!res.ok || !res.body) {
          setConnected(false)
          scheduleReconnect()
          return
        }

        setConnected(true)
        attempt = 0
        const reader = res.body.getReader()
        const decoder = new TextDecoder()
        let buffer = ''

        while (!closed) {
          const { done, value } = await reader.read()
          if (done) break
          buffer += decoder.decode(value, { stream: true })
          const parts = buffer.split('\n\n')
          buffer = parts.pop() ?? ''
          for (const part of parts) {
            const trimmed = part.trim()
            if (!trimmed || trimmed.startsWith(':')) continue
            const { data } = parseSseBlock(trimmed)
            if (!data) continue
            try {
              const parsed = JSON.parse(data) as VMEventPayload
              if (parsed && typeof parsed.vm_name === 'string') {
                onEventRef.current?.(parsed)
              }
            } catch {
              /* ignore malformed event */
            }
          }
        }
      } catch (err) {
        if (!closed && !(err instanceof DOMException && err.name === 'AbortError')) {
          setConnected(false)
          scheduleReconnect()
        }
        return
      }

      if (!closed) {
        setConnected(false)
        scheduleReconnect()
      }
    }

    connect()

    return () => {
      closed = true
      setConnected(false)
      if (reconnectTimer) clearTimeout(reconnectTimer)
      abort?.abort()
    }
  }, [enabled, url])

  return { connected }
}
