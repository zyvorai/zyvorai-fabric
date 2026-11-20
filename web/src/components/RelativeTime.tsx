// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { useEffect, useState } from 'react'
import { formatDateTime, formatRelativeTime } from '../utils/format'

interface RelativeTimeProps {
  date: string | Date
  className?: string
}

/** Live "3m ago" label; hover for the exact timestamp. Ticks forward on its own so it doesn't go stale on a long-open page. */
export default function RelativeTime({ date, className = '' }: RelativeTimeProps) {
  const [, forceTick] = useState(0)

  useEffect(() => {
    const i = setInterval(() => forceTick((n) => n + 1), 30_000)
    return () => clearInterval(i)
  }, [])

  if (!date) return <span className={className}>-</span>

  return (
    <span className={className} title={formatDateTime(date)}>
      {formatRelativeTime(date)}
    </span>
  )
}
