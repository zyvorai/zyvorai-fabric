// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef, useState } from 'react'
import RFB from '@novnc/novnc'
import { getToken } from '../api/client'
import { Loader2, WifiOff, AlertTriangle, Keyboard, Maximize, Minimize, RotateCw, Monitor } from 'lucide-react'

interface VNCViewerProps {
  vmName: string
}

type Status = 'connecting' | 'connected' | 'disconnected' | 'error'

export default function VNCViewer({ vmName }: VNCViewerProps) {
  const frameRef = useRef<HTMLDivElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const rfbRef = useRef<InstanceType<typeof RFB> | null>(null)
  const [status, setStatus] = useState<Status>('connecting')
  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [fullscreen, setFullscreen] = useState(false)
  const [connectAttempt, setConnectAttempt] = useState(0)

  useEffect(() => {
    if (!containerRef.current) return

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const token = getToken()
    const wsUrl = `${protocol}//${window.location.host}/ws/vnc/${vmName}${token ? `?token=${encodeURIComponent(token)}` : ''}`

    setStatus('connecting')
    setErrorMsg(null)

    const rfb = new RFB(containerRef.current, wsUrl)
    rfb.scaleViewport = true
    rfb.clipViewport = false
    rfb.resizeSession = false

    const onConnect = () => setStatus('connected')
    const onDisconnect = (e: CustomEvent<{ clean: boolean }>) => {
      setStatus('disconnected')
      if (!e.detail.clean) setErrorMsg('Connection lost')
    }
    const onSecurityFailure = (e: CustomEvent<{ reason?: string }>) => {
      setStatus('error')
      setErrorMsg(e.detail.reason || 'Security negotiation failed')
    }

    rfb.addEventListener('connect', onConnect)
    rfb.addEventListener('disconnect', onDisconnect as EventListener)
    rfb.addEventListener('securityfailure', onSecurityFailure as EventListener)

    rfbRef.current = rfb

    return () => {
      rfb.removeEventListener('connect', onConnect)
      rfb.removeEventListener('disconnect', onDisconnect as EventListener)
      rfb.removeEventListener('securityfailure', onSecurityFailure as EventListener)
      rfb.disconnect()
      rfbRef.current = null
    }
  }, [vmName, connectAttempt])

  useEffect(() => {
    const onFsChange = () => setFullscreen(document.fullscreenElement === frameRef.current)
    document.addEventListener('fullscreenchange', onFsChange)
    return () => document.removeEventListener('fullscreenchange', onFsChange)
  }, [])

  const toggleFullscreen = () => {
    if (document.fullscreenElement) {
      document.exitFullscreen()
    } else {
      frameRef.current?.requestFullscreen()
    }
  }

  const statusMeta: Record<Status, { label: string; dot: string; text: string }> = {
    connecting: { label: 'Connecting…', dot: 'bg-amber-400 animate-pulse', text: 'text-amber-400' },
    connected: { label: 'Connected', dot: 'bg-emerald-400', text: 'text-emerald-600' },
    disconnected: { label: 'Disconnected', dot: 'bg-slate-500', text: 'text-[var(--zf-muted)]' },
    error: { label: 'Connection failed', dot: 'bg-red-400', text: 'text-red-600' },
  }
  const meta = statusMeta[status]

  return (
    <div
      ref={frameRef}
      className={`relative rounded-xl border border-[var(--zf-hairline)] overflow-hidden ${fullscreen ? 'bg-black' : 'bg-gradient-to-b from-slate-900 to-black'}`}
    >
      {/* Toolbar */}
      <div className={`flex items-center justify-between gap-3 px-4 py-2.5 border-b border-white/10 ${fullscreen ? 'bg-black/80' : 'bg-slate-900/70'}`}>
        <div className="flex items-center gap-2">
          <span className={`w-2 h-2 rounded-full ${meta.dot}`} />
          <span className={`text-xs font-medium ${meta.text}`}>{meta.label}</span>
        </div>
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={() => rfbRef.current?.sendCtrlAltDel()}
            disabled={status !== 'connected'}
            title="Send Ctrl+Alt+Del"
            className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium text-[var(--zf-ink)] hover:text-[var(--zf-ink)] hover:bg-white/10 disabled:opacity-40 disabled:hover:bg-transparent transition-colors"
          >
            <Keyboard className="w-3.5 h-3.5" />
            Ctrl+Alt+Del
          </button>
          {status !== 'connected' && status !== 'connecting' && (
            <button
              type="button"
              onClick={() => setConnectAttempt((n) => n + 1)}
              title="Reconnect"
              className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium text-[var(--zf-ink)] hover:text-[var(--zf-ink)] hover:bg-white/10 transition-colors"
            >
              <RotateCw className="w-3.5 h-3.5" />
              Reconnect
            </button>
          )}
          <button
            type="button"
            onClick={toggleFullscreen}
            title={fullscreen ? 'Exit fullscreen' : 'Fullscreen'}
            className="p-1.5 rounded-lg text-[var(--zf-ink)] hover:text-[var(--zf-ink)] hover:bg-white/10 transition-colors"
          >
            {fullscreen ? <Minimize className="w-3.5 h-3.5" /> : <Maximize className="w-3.5 h-3.5" />}
          </button>
        </div>
      </div>

      {/* Display */}
      {/* noVNC's own wrapper div sets `height: 100%` on itself (see
          @novnc/novnc's rfb.js) -- a percentage height resolves against
          the *computed* `height` of its ancestors, not against a
          min-height-only box (which computes to `auto`), so every div in
          this chain needs a real `height`, not just `minHeight`. Without
          it the canvas silently renders at 0x0 despite real framebuffer
          data arriving -- "Connected" shows, nothing is ever visible. */}
      <div className="relative" style={{ height: fullscreen ? 'calc(100vh - 45px)' : '500px' }}>
        <div ref={containerRef} className="w-full h-full" style={{ height: fullscreen ? 'calc(100vh - 45px)' : '500px' }} />
        {status !== 'connected' && (
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 pointer-events-none">
            {status === 'connecting' && (
              <>
                <Loader2 className="w-8 h-8 text-[var(--zf-link)] animate-spin" />
                <p className="text-sm text-[var(--zf-muted)]">Connecting to display…</p>
              </>
            )}
            {status === 'disconnected' && (
              <>
                <WifiOff className="w-8 h-8 text-[var(--zf-muted)]" />
                <p className="text-sm text-[var(--zf-muted)]">{errorMsg || 'Disconnected'}</p>
              </>
            )}
            {status === 'error' && (
              <>
                <AlertTriangle className="w-8 h-8 text-red-600" />
                <p className="text-sm text-red-600">{errorMsg || 'Connection failed'}</p>
              </>
            )}
          </div>
        )}
        {status === 'connected' && (
          // A black canvas alone is indistinguishable from a dead connection
          // -- the guest's VGA framebuffer stays blank on any image whose
          // console output goes entirely to a serial tty, which is common.
          <div className="absolute bottom-3 right-3 flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-black/60 backdrop-blur-sm text-[var(--zf-muted)] text-xs pointer-events-none">
            <Monitor className="w-3 h-3" />
            Live display
          </div>
        )}
      </div>
    </div>
  )
}
