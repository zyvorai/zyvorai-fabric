// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useState, useCallback, useRef } from 'react'

export interface ConfirmState {
  isOpen: boolean
  title: string
  message: string
  onConfirm: () => void
  confirmLabel?: string
  variant?: 'danger' | 'warning' | 'info'
}

export function useConfirm() {
  const [state, setState] = useState<ConfirmState | null>(null)
  const resolveRef = useRef<((value: boolean) => void) | null>(null)

  const confirm = useCallback(
    (
      title: string,
      message: string,
      options?: { confirmLabel?: string; variant?: 'danger' | 'warning' | 'info' },
    ): Promise<boolean> => {
      return new Promise((resolve) => {
        resolveRef.current = resolve
        setState({
          isOpen: true,
          title,
          message,
          confirmLabel: options?.confirmLabel,
          variant: options?.variant,
          onConfirm: () => {
            setState(null)
            resolveRef.current = null
            resolve(true)
          },
        })
      })
    },
    [],
  )

  const cancel = useCallback(() => {
    if (resolveRef.current) {
      resolveRef.current(false)
      resolveRef.current = null
    }
    setState(null)
  }, [])

  return { confirmState: state, confirm, cancel }
}
