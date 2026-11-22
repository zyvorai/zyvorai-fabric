// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { createContext, useContext, ReactNode, useState, useRef, useCallback } from 'react'
import { useEventStream, type VMEventPayload } from '../hooks/useEventStream'
import { WebSocketMessage } from '../hooks/useWebSocket'
import { VM } from '../api/vm'
import { useAuth } from './AuthContext'

interface WebSocketContextType {
  isConnected: boolean
  vmUpdates: Map<string, Partial<VM>>
  subscribe: (callback: (message: WebSocketMessage) => void) => () => void
}

const WebSocketContext = createContext<WebSocketContextType | undefined>(undefined)

function mapEventToMessage(event: VMEventPayload): WebSocketMessage | null {
  const type = event.event_type
  if (type === 'started' || type === 'stopped' || type === 'paused' || type === 'resumed') {
    return {
      type: 'vm_state_changed',
      data: { name: event.vm_name, state: type },
    }
  }
  if (type === 'created') {
    return { type: 'vm_created', data: { name: event.vm_name } }
  }
  if (type === 'deleted') {
    return { type: 'vm_deleted', data: { name: event.vm_name } }
  }
  return null
}

export function WebSocketProvider({ children }: { children: ReactNode }) {
  const { user } = useAuth()
  const [vmUpdates, setVmUpdates] = useState<Map<string, Partial<VM>>>(new Map())
  const subscribersRef = useRef<Set<(message: WebSocketMessage) => void>>(new Set())

  const handleEvent = useCallback((event: VMEventPayload) => {
    const message = mapEventToMessage(event)
    if (message) {
      subscribersRef.current.forEach((callback) => callback(message))
      const vmName = event.vm_name
      if (!vmName) return
      switch (message.type) {
        case 'vm_state_changed':
          setVmUpdates((prev) => {
            const updated = new Map(prev)
            updated.set(vmName, { state: message.data.state as string } as Partial<VM>)
            return updated
          })
          break
        case 'vm_deleted':
          setVmUpdates((prev) => {
            const updated = new Map(prev)
            updated.delete(vmName)
            return updated
          })
          break
        default:
          break
      }
    }
  }, [])

  const { connected: isConnected } = useEventStream({
    enabled: Boolean(user),
    onEvent: handleEvent,
  })

  const subscribe = (callback: (message: WebSocketMessage) => void) => {
    subscribersRef.current.add(callback)
    return () => {
      subscribersRef.current.delete(callback)
    }
  }

  return (
    <WebSocketContext.Provider value={{ isConnected, vmUpdates, subscribe }}>
      {children}
    </WebSocketContext.Provider>
  )
}

export function useWebSocketContext() {
  const context = useContext(WebSocketContext)
  if (!context) {
    throw new Error('useWebSocketContext must be used within WebSocketProvider')
  }
  return context
}
