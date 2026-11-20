// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import type { ReactNode } from 'react'
import { AlertTriangle } from 'lucide-react'

type Tone = 'amber' | 'red'

type Props = {
  title: string
  headline: string
  hints?: string[]
  technicalDetail?: string
  tone?: Tone
  onDismiss?: () => void
  onRetry?: () => void
  retryLabel?: string
  actions?: ReactNode
}

const toneStyles: Record<Tone, { border: string; bg: string; title: string; text: string }> = {
  amber: {
    border: 'border-amber-200',
    bg: 'bg-amber-50',
    title: 'text-amber-900',
    text: 'text-amber-900/90',
  },
  red: {
    border: 'border-red-200',
    bg: 'bg-red-50',
    title: 'text-red-900',
    text: 'text-red-900/90',
  },
}

/** Actionable error panel with optional hints and technical details. */
export default function ErrorBanner({
  title,
  headline,
  hints,
  technicalDetail,
  tone = 'amber',
  onDismiss,
  onRetry,
  retryLabel = 'Retry',
  actions,
}: Props) {
  const s = toneStyles[tone]
  return (
    <div className={`rounded-[12px] border ${s.border} ${s.bg} p-4 mb-4`} role="alert">
      <div className="flex gap-3">
        <AlertTriangle className={`w-5 h-5 shrink-0 mt-0.5 ${s.title}`} />
        <div className="min-w-0 flex-1">
          <div className={`text-sm font-semibold ${s.title}`}>{title}</div>
          <p className={`text-sm mt-1 ${s.text}`}>{headline}</p>
          {hints && hints.length > 0 && (
            <ul className={`mt-2 text-sm list-disc pl-4 space-y-1 ${s.text}`}>
              {hints.map((h) => (
                <li key={h}>{h}</li>
              ))}
            </ul>
          )}
          {technicalDetail && (
            <pre className="mt-2 text-xs overflow-x-auto opacity-80 font-mono whitespace-pre-wrap">
              {technicalDetail}
            </pre>
          )}
          <div className="mt-3 flex flex-wrap gap-2">
            {onRetry && (
              <button type="button" className="zf-btn zf-btn-ghost zf-btn-sm" onClick={onRetry}>
                {retryLabel}
              </button>
            )}
            {onDismiss && (
              <button type="button" className="zf-btn zf-btn-secondary zf-btn-sm" onClick={onDismiss}>
                Dismiss
              </button>
            )}
            {actions}
          </div>
        </div>
      </div>
    </div>
  )
}
