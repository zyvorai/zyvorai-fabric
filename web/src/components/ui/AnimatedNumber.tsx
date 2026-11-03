// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useCountUp } from '../../hooks/useCountUp'
import { usePrefersReducedMotion } from '../../hooks/usePrefersReducedMotion'

interface AnimatedNumberProps {
  value: number
  durationMs?: number
  /** Shown after the number, such as "%" or " ms". */
  suffix?: string
  className?: string
}

/** A number that counts toward its new value, so a live update reads as a change. With reduced motion it just shows the
    value. Digits are tabular so the width does not jitter. */
export function AnimatedNumber({ value, durationMs = 600, suffix = '', className = '' }: AnimatedNumberProps) {
  const reduced = usePrefersReducedMotion()
  const counted = useCountUp(value, durationMs)
  return (
    <span className={`tabular-nums ${className}`}>
      {reduced ? value : counted}
      {suffix}
    </span>
  )
}
