// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback, useState } from 'react'
import { useToastContext } from '../contexts/ToastContext'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'

/** Standard page load state with toast + formatted error for ErrorBanner. */
export function usePageLoader(toastLabel = 'Failed to load data') {
  const toast = useToastContext()
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)

  const run = useCallback(
    async (fn: () => Promise<void>, options?: { silent?: boolean }) => {
      if (!options?.silent) {
        setLoading(true)
      }
      setLoadError(null)
      try {
        await fn()
      } catch (err) {
        const msg = formatUserError(err)
        setLoadError(msg)
        if (!options?.silent) {
          toastFailure(toast, toastLabel, err)
        }
      } finally {
        if (!options?.silent) {
          setLoading(false)
        }
      }
    },
    [toast, toastLabel],
  )

  return { loading, setLoading, loadError, setLoadError, run }
}
