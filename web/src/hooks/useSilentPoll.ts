// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { useEffect } from 'react'

/** Poll on an interval; initial load is the caller's responsibility. */
export function useSilentPoll(
  callback: () => void | Promise<void>,
  intervalMs: number,
  enabled = true,
) {
  useEffect(() => {
    if (!enabled) return
    const id = setInterval(() => void callback(), intervalMs)
    return () => clearInterval(id)
  }, [callback, intervalMs, enabled])
}
