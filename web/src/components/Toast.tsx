// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react'
import { CheckCircle, XCircle, AlertCircle, Info, X } from 'lucide-react'
import { formatUserError } from '../utils/apiError'

export type ToastType = 'success' | 'error' | 'warning' | 'info'

export interface Toast {
  id: string
  type: ToastType
  message: string
  duration?: number
}

interface ToastProps {
  toast: Toast
  onClose: (id: string) => void
}

function displayMessage(type: ToastType, message: string): string {
  if (type !== 'error') return message
  const sanitized = formatUserError(new Error(message))
  return sanitized.length > 320 ? `${sanitized.slice(0, 317)}…` : sanitized
}

export function ToastItem({ toast, onClose }: ToastProps) {
  useEffect(() => {
    const duration = toast.duration || 5000
    const timer = setTimeout(() => onClose(toast.id), duration)
    return () => clearTimeout(timer)
  }, [toast, onClose])

  const display = displayMessage(toast.type, toast.message)

  const config = {
    success: { icon: CheckCircle, bg: 'bg-emerald-50', border: 'border-emerald-200', text: 'text-emerald-700' },
    error: { icon: XCircle, bg: 'bg-red-50', border: 'border-red-200', text: 'text-red-700' },
    warning: { icon: AlertCircle, bg: 'bg-amber-50', border: 'border-amber-200', text: 'text-amber-800' },
    info: { icon: Info, bg: 'bg-sky-50', border: 'border-sky-200', text: 'text-sky-800' },
  }[toast.type]

  const Icon = config.icon

  return (
    <div
      className={`flex items-center gap-3 px-4 py-3 rounded-xl border shadow-lg animate-slide-in bg-[var(--zf-surface)] ${config.bg} ${config.border}`}
    >
      <Icon className={`w-4 h-4 shrink-0 ${config.text}`} />
      <span className="flex-1 text-sm text-[var(--zf-ink)] whitespace-pre-wrap break-words">{display}</span>
      <button
        onClick={() => onClose(toast.id)}
        className="shrink-0 p-0.5 rounded-md text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition-colors" aria-label="Close" title="Close">
        <X className="w-3.5 h-3.5" />
      </button>
    </div>
  )
}

interface ToastContainerProps {
  toasts: Toast[]
  onClose: (id: string) => void
}

export function ToastContainer({ toasts, onClose }: ToastContainerProps) {
  return (
    <div className="fixed top-4 right-4 z-50 space-y-2 max-w-sm">
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} onClose={onClose} />
      ))}
    </div>
  )
}
