// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

/**
 * Format bytes into a human-readable string (e.g., "1.5 GB")
 */
export function formatBytes(bytes: number, decimals = 1): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / Math.pow(k, i)).toFixed(decimals)} ${sizes[i]}`
}

/**
 * Format a date string or Date object into a localized date-time string
 */
export function formatDateTime(date: string | Date): string {
  const d = typeof date === 'string' ? new Date(date) : date
  return d.toLocaleString()
}

/**
 * Format a date string or Date object into a relative time string. Handles both
 * the past ("5m ago") and the future ("in 5m") — callers pass expiry/next-run
 * dates as often as they pass creation dates, and those are ahead of `now`.
 */
export function formatRelativeTime(date: string | Date): string {
  const d = typeof date === 'string' ? new Date(date) : date
  const now = new Date()
  const diffMs = now.getTime() - d.getTime()
  const future = diffMs < 0
  const absSec = Math.floor(Math.abs(diffMs) / 1000)
  const absMin = Math.floor(absSec / 60)
  const absHour = Math.floor(absMin / 60)
  const absDay = Math.floor(absHour / 24)

  if (absSec < 60) return 'just now'
  if (absDay >= 30) return formatDateTime(d)

  const value = absDay >= 1 ? `${absDay}d` : absHour >= 1 ? `${absHour}h` : `${absMin}m`
  return future ? `in ${value}` : `${value} ago`
}
