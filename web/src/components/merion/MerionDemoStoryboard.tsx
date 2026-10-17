// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef, useState } from 'react'

const CHIPS = ['Cell up', 'Extract', 'Brief ready'] as const

/**
 * Presentational PDF-brief walkthrough — the steps light up in order. No API.
 * The outbound-connection count is a constant 0: it never counts down, because
 * the demo claims the agent made none.
 */
export default function MerionDemoStoryboard() {
  const root = useRef<HTMLDivElement>(null)
  const [active, setActive] = useState(0)
  const [playing, setPlaying] = useState(false)

  useEffect(() => {
    const el = root.current
    if (!el) return
    const io = new IntersectionObserver(
      ([e]) => {
        if (e?.isIntersecting) setPlaying(true)
      },
      { threshold: 0.35 },
    )
    io.observe(el)
    return () => io.disconnect()
  }, [])

  useEffect(() => {
    if (!playing) return
    const reduce =
      typeof window !== 'undefined' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches
    if (reduce) {
      setActive(CHIPS.length)
      return
    }
    setActive(0)
    const timers: number[] = []
    timers.push(window.setTimeout(() => setActive(1), 500))
    timers.push(window.setTimeout(() => setActive(2), 1100))
    timers.push(window.setTimeout(() => setActive(3), 1700))
    return () => timers.forEach(clearTimeout)
  }, [playing])

  return (
    <div ref={root} className="merion-mkt-storyboard">
      <div className="merion-mkt-storyboard-inner">
        <div className="merion-mkt-storyboard-label">PDF brief</div>
        <div className="merion-mkt-chips">
          {CHIPS.map((label, i) => (
            <span
              key={label}
              className={`merion-mkt-chip${active > i ? ' is-on' : ''}${active === i + 1 ? ' is-pulse' : ''}`}
            >
              {label}
            </span>
          ))}
        </div>
        <div className="merion-mkt-connect is-zero">
          <span className="merion-mkt-connect-label">Outbound connections</span>
          <span className="merion-mkt-connect-num">0</span>
        </div>
        <p className="merion-mkt-storyboard-note">
          Counted from Merion’s own audit log and enforced on the host.
        </p>
      </div>
    </div>
  )
}
