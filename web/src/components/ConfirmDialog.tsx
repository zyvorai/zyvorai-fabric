// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef } from 'react'
import { AlertTriangle } from 'lucide-react'

interface ConfirmDialogProps {
  title: string
  message: string
  confirmLabel?: string
  cancelLabel?: string
  variant?: 'danger' | 'warning' | 'info'
  onConfirm: () => void
  onCancel: () => void
}

export default function ConfirmDialog({
  title,
  message,
  confirmLabel = 'Confirm',
  cancelLabel = 'Cancel',
  variant = 'danger',
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel()
    }
    window.addEventListener('keydown', handleKeyDown)
    cancelRef.current?.focus()
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [onCancel])

  const confirmColors = {
    danger: 'bg-red-600 hover:bg-red-500 text-white',
    warning: 'bg-yellow-600 hover:bg-yellow-500 text-[var(--zf-ink)]',
    info: 'bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] text-white',
  }[variant]

  const iconBg = {
    danger: 'bg-red-500/10',
    warning: 'bg-yellow-500/10',
    info: 'bg-blue-500/10',
  }[variant]

  const iconColor = {
    danger: 'text-red-600',
    warning: 'text-amber-600',
    info: 'text-[var(--zf-link)]',
  }[variant]

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="confirm-title"
      aria-describedby="confirm-message"
      onClick={onCancel}
    >
      <div
        className="bg-[var(--zf-canvas)] rounded-xl shadow-2xl border border-[var(--zf-hairline)] w-full max-w-md p-6"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start gap-4 mb-5">
          <div className={`p-2 rounded-lg shrink-0 ${iconBg}`}>
            <AlertTriangle className={`w-5 h-5 ${iconColor}`} />
          </div>
          <div>
            <h3 id="confirm-title" className="text-base font-semibold text-[var(--zf-ink)]">
              {title}
            </h3>
            <p id="confirm-message" className="text-sm text-[var(--zf-muted)] mt-1 leading-relaxed">
              {message}
            </p>
          </div>
        </div>
        <div className="flex justify-end gap-2">
          <button
            ref={cancelRef}
            onClick={onCancel}
            className="px-4 py-2 bg-[var(--zf-surface)] hover:bg-black/[0.04] border border-[var(--zf-hairline)] rounded-lg transition-colors text-sm text-[var(--zf-ink)]"
          >
            {cancelLabel}
          </button>
          <button
            onClick={onConfirm}
            className={`px-4 py-2 rounded-lg transition-colors text-sm font-medium ${confirmColors}`}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  )
}
