// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react'

export type AppTheme = 'light' | 'dark'

const STORAGE_KEY = 'zyvor-fabricd-theme'

interface ThemeContextType {
  theme: AppTheme
  toggleTheme: () => void
  setTheme: (t: AppTheme) => void
}

const ThemeContext = createContext<ThemeContextType>({
  theme: 'light',
  toggleTheme: () => {},
  setTheme: () => {},
})

function initialTheme(): AppTheme {
  if (typeof document !== 'undefined' && document.documentElement.getAttribute('data-theme') === 'dark') {
    return 'dark'
  }
  return 'light'
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<AppTheme>(initialTheme)

  useEffect(() => {
    const root = document.documentElement
    if (theme === 'dark') {
      root.setAttribute('data-theme', 'dark')
    } else {
      root.removeAttribute('data-theme')
    }
    try {
      localStorage.setItem(STORAGE_KEY, theme)
    } catch {
      // localStorage unavailable (private mode, etc.) — theme just won't persist
    }
  }, [theme])

  const setTheme = useCallback((t: AppTheme) => setThemeState(t), [])
  const toggleTheme = useCallback(() => setThemeState((t) => (t === 'dark' ? 'light' : 'dark')), [])

  return <ThemeContext.Provider value={{ theme, toggleTheme, setTheme }}>{children}</ThemeContext.Provider>
}

export function useTheme() {
  return useContext(ThemeContext)
}
