// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { useCallback, useEffect, useRef, useState } from 'react'

export interface PendingUndo {
  label: string
  secondsLeft: number
  totalSeconds: number
}

/**
 * Defers a destructive action behind a short grace window instead of firing it
 * immediately: `run` starts the countdown, `undo` cancels it, and if neither
 * `undo` nor a new `run` happens before time is up, `commit` fires for real.
 * Only one action is pending at a time — starting a new one immediately
 * commits whatever was already pending, so nothing silently gets lost.
 */
export function useUndoableAction(seconds = 5) {
  const [pending, setPending] = useState<PendingUndo | null>(null)
  const commitRef = useRef<(() => void | Promise<void>) | null>(null)
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const clearTimers = useCallback(() => {
    if (intervalRef.current) clearInterval(intervalRef.current)
    intervalRef.current = null
  }, [])

  const flush = useCallback(() => {
    clearTimers()
    const commit = commitRef.current
    commitRef.current = null
    setPending(null)
    if (commit) void commit()
  }, [clearTimers])

  const run = useCallback(
    (label: string, commit: () => void | Promise<void>) => {
      flush() // commit any already-pending action first rather than dropping it
      commitRef.current = commit
      setPending({ label, secondsLeft: seconds, totalSeconds: seconds })
      intervalRef.current = setInterval(() => {
        setPending((p) => {
          if (!p) return p
          if (p.secondsLeft <= 1) {
            flush()
            return null
          }
          return { ...p, secondsLeft: p.secondsLeft - 1 }
        })
      }, 1000)
    },
    [flush, seconds]
  )

  const undo = useCallback(() => {
    clearTimers()
    commitRef.current = null
    setPending(null)
  }, [clearTimers])

  useEffect(() => clearTimers, [clearTimers])

  return { pending, run, undo }
}
