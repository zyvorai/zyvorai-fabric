// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef, useState } from 'react'
import { Terminal as XTerm } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { getToken } from '../api/client'
import { RotateCw } from 'lucide-react'
import { AppleTerminalFrame } from './AppleTerminalFrame'

interface TerminalProps {
  vmName: string
}

type Status = 'connecting' | 'connected' | 'disconnected'

export default function Terminal({ vmName }: TerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null)
  const xtermRef = useRef<XTerm | null>(null)
  const wsRef = useRef<WebSocket | null>(null)
  const [status, setStatus] = useState<Status>('connecting')
  const [connectAttempt, setConnectAttempt] = useState(0)

  useEffect(() => {
    if (!terminalRef.current) return
    setStatus('connecting')
    terminalRef.current.replaceChildren()

    const term = new XTerm({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
      theme: {
        background: '#1c1c1e',
        foreground: '#f5f5f7',
        cursor: '#f5f5f7',
        selectionBackground: '#0a84ff66',
        black: '#8e8e93',
        red: '#ff453a',
        green: '#32d74b',
        yellow: '#ffd60a',
        blue: '#0a84ff',
        magenta: '#bf5af2',
        cyan: '#64d2ff',
        white: '#f5f5f7',
        brightBlack: '#636366',
        brightRed: '#ff6961',
        brightGreen: '#30db5b',
        brightYellow: '#ffd426',
        brightBlue: '#409cff',
        brightMagenta: '#da8fff',
        brightCyan: '#70d7ff',
        brightWhite: '#ffffff',
      },
    })

    term.open(terminalRef.current)
    xtermRef.current = term

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const token = getToken()
    const wsUrl = `${protocol}//${window.location.host}/ws/console/${vmName}${token ? `?token=${encodeURIComponent(token)}` : ''}`
    const ws = new WebSocket(wsUrl)
    ws.binaryType = 'arraybuffer'

    ws.onopen = () => {
      setStatus('connected')
      term.write('Connected to VM console\r\n')
    }

    ws.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        term.write(new Uint8Array(event.data))
      } else {
        term.write(event.data)
      }
    }

    ws.onerror = (error) => {
      console.error('WebSocket error:', error)
      term.write('\r\nWebSocket error\r\n')
    }

    ws.onclose = () => {
      setStatus('disconnected')
      term.write('\r\nConnection closed\r\n')
    }

    term.onData((data) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(data)
      }
    })

    wsRef.current = ws

    return () => {
      ws.close()
      term.dispose()
    }
  }, [vmName, connectAttempt])

  const statusMeta: Record<Status, { label: string; color: string }> = {
    connecting: { label: 'Connecting…', color: '#ffd60a' },
    connected: { label: 'Connected', color: '#28c840' },
    disconnected: { label: 'Disconnected', color: '#8e8e93' },
  }
  const meta = statusMeta[status]

  return (
    <AppleTerminalFrame
      title={`${vmName} — tty`}
      live={status === 'connected'}
      trailing={
        <div className="flex items-center gap-2 shrink-0">
          <span className="text-xs" style={{ color: meta.color }}>{meta.label}</span>
          {status === 'disconnected' && (
            <button
              type="button"
              onClick={() => setConnectAttempt((n) => n + 1)}
              className="flex items-center gap-1.5 px-2 py-1 rounded-md text-xs font-medium text-white/70 hover:text-white hover:bg-white/10 transition-colors"
            >
              <RotateCw className="w-3.5 h-3.5" />
              Reconnect
            </button>
          )}
        </div>
      }
      bodyClassName="p-2"
    >
      <div
        ref={terminalRef}
        className="w-full"
        style={{ minHeight: '500px' }}
      />
    </AppleTerminalFrame>
  )
}
