// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useEffect, useRef, useState } from 'react'

/** Animates a displayed number toward `target` whenever it changes -- used on
    dashboard stat tiles so live updates read as a change, not a silent swap. */
export function useCountUp(target: number, durationMs = 600): number {
  const [value, setValue] = useState(target)
  const frameRef = useRef<number | undefined>(undefined)
  const fromRef = useRef(target)

  useEffect(() => {
    if (target === fromRef.current) return
    const from = fromRef.current
    const start = performance.now()
    if (frameRef.current) cancelAnimationFrame(frameRef.current)

    const tick = (now: number) => {
      const t = Math.min(1, (now - start) / durationMs)
      const eased = 1 - Math.pow(1 - t, 3)
      setValue(Math.round(from + (target - from) * eased))
      if (t < 1) {
        frameRef.current = requestAnimationFrame(tick)
      } else {
        fromRef.current = target
      }
    }
    frameRef.current = requestAnimationFrame(tick)
    return () => { if (frameRef.current) cancelAnimationFrame(frameRef.current) }
  }, [target, durationMs])

  return value
}
