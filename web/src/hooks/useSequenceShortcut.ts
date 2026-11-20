// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { useEffect, useRef } from 'react'
import { isInputFocused } from './useKeyboardShortcut'

interface SequenceShortcut {
  sequence: [string, string]
  handler: () => void
}

export function useSequenceShortcuts(shortcuts: SequenceShortcut[], enabled = true) {
  const firstKeyRef = useRef<string | null>(null)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    if (!enabled) return

    const onKeyDown = (e: KeyboardEvent) => {
      if (isInputFocused()) return
      if (e.ctrlKey || e.metaKey || e.altKey) return

      const key = e.key.toLowerCase()

      if (firstKeyRef.current) {
        const first = firstKeyRef.current
        firstKeyRef.current = null
        if (timerRef.current) { clearTimeout(timerRef.current); timerRef.current = null }

        for (const s of shortcuts) {
          if (s.sequence[0] === first && s.sequence[1] === key) {
            e.preventDefault()
            s.handler()
            return
          }
        }
        return
      }

      // Check if this key is the first key of any sequence
      const isFirstKey = shortcuts.some(s => s.sequence[0] === key)
      if (isFirstKey) {
        firstKeyRef.current = key
        timerRef.current = setTimeout(() => { firstKeyRef.current = null }, 500)
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => {
      window.removeEventListener('keydown', onKeyDown)
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [shortcuts, enabled])
}
