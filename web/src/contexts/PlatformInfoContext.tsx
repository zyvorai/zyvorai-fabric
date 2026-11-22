// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'
import { getCapabilities, type Capabilities } from '../api/capabilities'

export interface PlatformInfoContextValue {
  capabilities: Capabilities | null
  loading: boolean
  refresh: () => Promise<void>
}

const DEFAULT: PlatformInfoContextValue = {
  capabilities: null,
  loading: true,
  refresh: async () => {},
}

const PlatformInfoContext = createContext<PlatformInfoContextValue>(DEFAULT)

export function PlatformInfoProvider({ children }: { children: ReactNode }) {
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null)
  const [loading, setLoading] = useState(true)

  const refresh = useCallback(async () => {
    try {
      setCapabilities(await getCapabilities())
    } catch {
      setCapabilities(null)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    refresh()
    const interval = setInterval(refresh, 30_000)
    return () => clearInterval(interval)
  }, [refresh])

  const value = useMemo(
    () => ({ capabilities, loading, refresh }),
    [capabilities, loading, refresh],
  )

  return <PlatformInfoContext.Provider value={value}>{children}</PlatformInfoContext.Provider>
}

export function usePlatformInfo(): PlatformInfoContextValue {
  return useContext(PlatformInfoContext)
}
