// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

interface StatusBadgeProps {
  status: string
  variant?: 'dot' | 'pill'
  title?: string
}

const statusStyles: Record<string, string> = {
  running: 'text-emerald-700 bg-emerald-50 border-emerald-200',
  active: 'text-emerald-700 bg-emerald-50 border-emerald-200',
  enabled: 'text-emerald-700 bg-emerald-50 border-emerald-200',
  healthy: 'text-emerald-700 bg-emerald-50 border-emerald-200',
  completed: 'text-emerald-700 bg-emerald-50 border-emerald-200',
  success: 'text-emerald-700 bg-emerald-50 border-emerald-200',
  stopped: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
  failed: 'text-red-700 bg-red-50 border-red-200',
  error: 'text-red-700 bg-red-50 border-red-200',
  disabled: 'text-red-700 bg-red-50 border-red-200',
  paused: 'text-amber-800 bg-amber-50 border-amber-200',
  warning: 'text-amber-800 bg-amber-50 border-amber-200',
  pending: 'text-amber-800 bg-amber-50 border-amber-200',
  unknown: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
}

const dotColors: Record<string, string> = {
  running: 'bg-emerald-500',
  active: 'bg-emerald-500',
  enabled: 'bg-emerald-500',
  healthy: 'bg-emerald-500',
  completed: 'bg-emerald-500',
  success: 'bg-emerald-500',
  stopped: 'bg-[var(--zf-muted)]',
  failed: 'bg-red-500',
  error: 'bg-red-500',
  disabled: 'bg-red-500',
  paused: 'bg-amber-500',
  warning: 'bg-amber-500',
  pending: 'bg-amber-500',
  unknown: 'bg-[var(--zf-muted)]',
}

const isRunning = (s: string) =>
  ['running', 'active', 'enabled', 'healthy'].includes(s.toLowerCase())

export function StatusBadge({ status, variant = 'pill', title }: StatusBadgeProps) {
  const key = status.toLowerCase()
  const style = statusStyles[key] || statusStyles.unknown
  const pulse = isRunning(key)

  if (variant === 'dot') {
    const dot = dotColors[key] || dotColors.unknown
    return (
      <span className="flex items-center gap-2" title={title}>
        <span className="relative flex h-2 w-2">
          {pulse && <span className={`absolute inset-0 rounded-full ${dot} opacity-40 animate-ping`} />}
          <span className={`relative w-2 h-2 rounded-full ${dot}`} />
        </span>
        <span className="capitalize text-[var(--zf-ink)]">{status}</span>
      </span>
    )
  }

  return (
    <span
      title={title}
      className={`inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium capitalize border ${style}`}
    >
      <span className="relative flex h-1.5 w-1.5">
        {pulse && (
          <span
            className={`absolute inset-0 rounded-full ${dotColors[key] || dotColors.unknown} opacity-40 animate-ping`}
          />
        )}
        <span className={`relative w-1.5 h-1.5 rounded-full ${dotColors[key] || dotColors.unknown}`} />
      </span>
      {status}
    </span>
  )
}
