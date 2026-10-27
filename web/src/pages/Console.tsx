// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react'
import { useParams, useNavigate, useSearchParams } from 'react-router'
import { ArrowLeft, Terminal as TerminalIcon, Monitor } from 'lucide-react'
import Terminal from '../components/Terminal'
import VNCViewer from '../components/VNCViewer'
import { getVM } from '../api/vm'
import ErrorBanner from '../components/ErrorBanner'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { useToastContext } from '../contexts/ToastContext'

export default function Console() {
  const { name } = useParams<{ name: string }>()
  const navigate = useNavigate()
  const toast = useToastContext()
  const [searchParams] = useSearchParams()
  const [mode, setMode] = useState<'terminal' | 'vnc'>(searchParams.get('mode') === 'vnc' ? 'vnc' : 'terminal')
  const [loadError, setLoadError] = useState<string | null>(null)

  useEffect(() => {
    if (!name) return
    let cancelled = false
    setLoadError(null)
    getVM(name)
      .catch((err) => {
        if (!cancelled) {
          const msg = formatUserError(err)
          setLoadError(msg)
          toastFailure(toast, 'Could not open console', err)
        }
      })
    return () => { cancelled = true }
  }, [name, toast])

  if (!name) return null

  return (
    <div>
      {loadError && (
        <ErrorBanner
          title="Console unavailable"
          headline={loadError}
        />
      )}
      <button
        onClick={() => navigate(`/app/vms/${name}`)}
        className="flex items-center gap-2 mb-6 text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition"
      >
        <ArrowLeft className="w-4 h-4" />
        Back to VM Details
      </button>

      <div className="flex items-center justify-between gap-4 mb-6">
        <div className="flex items-center gap-3 min-w-0">
          <div className="icon-tile icon-tile-md icon-tile-blue">
            {mode === 'terminal' ? <TerminalIcon className="w-5 h-5" /> : <Monitor className="w-5 h-5" />}
          </div>
          <div className="min-w-0">
            <h1 className="text-[26px] sm:text-[32px] leading-tight font-semibold tracking-[-0.022em] text-[var(--zf-ink)] truncate">Console: {name}</h1>
            <p className="text-sm text-[var(--zf-muted)] mt-1">Interactive access to this VM</p>
          </div>
        </div>
        <div className="flex items-center gap-1 p-1 bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-xl shrink-0">
          <button
            onClick={() => setMode('terminal')}
            className={`flex items-center gap-2 px-3.5 py-1.5 rounded-lg text-sm font-medium transition-colors ${
              mode === 'terminal'
                ? 'bg-[var(--zf-link)] text-white'
                : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
            }`}
          >
            <TerminalIcon className="w-4 h-4" />
            Terminal
          </button>
          <button
            onClick={() => setMode('vnc')}
            className={`flex items-center gap-2 px-3.5 py-1.5 rounded-lg text-sm font-medium transition-colors ${
              mode === 'vnc'
                ? 'bg-[var(--zf-link)] text-white'
                : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
            }`}
          >
            <Monitor className="w-4 h-4" />
            VNC
          </button>
        </div>
      </div>

      {mode === 'terminal' ? <Terminal vmName={name} /> : <VNCViewer vmName={name} />}
    </div>
  )
}
