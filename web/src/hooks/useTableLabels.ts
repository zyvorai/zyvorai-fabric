// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, type RefObject } from 'react'

/**
 * Copies each column header's text onto the cells below it as `data-label`, so the phone
 * layout (tables become stacked cards, see main.css) can show "Name: web-1" instead of a bare
 * value. Runs on every DOM change under `root` because pages render their tables themselves.
 * A table can opt out with `data-no-cards`, a cell with its own `data-label`.
 */
export function useTableLabels(root: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const el = root.current
    if (!el) return
    let frame = 0
    const label = () => {
      frame = 0
      el.querySelectorAll('table:not([data-no-cards])').forEach((table) => {
        const heads = Array.from(table.querySelectorAll('thead tr:last-child > th')).map(
          (th) => th.textContent?.trim() ?? '',
        )
        if (heads.length === 0) return
        table.querySelectorAll('tbody tr').forEach((row) => {
          Array.from(row.children).forEach((cell, i) => {
            if (cell.tagName !== 'TD' || cell.hasAttribute('data-label')) return
            if ((cell as HTMLTableCellElement).colSpan > 1 || !heads[i]) return
            cell.setAttribute('data-label', heads[i])
          })
        })
      })
    }
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(label)
    }
    const observer = new MutationObserver(schedule)
    observer.observe(el, { childList: true, subtree: true })
    schedule()
    return () => {
      observer.disconnect()
      if (frame) cancelAnimationFrame(frame)
    }
  }, [root])
}
