// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { createContext, useContext, ReactNode } from 'react'

const ReadOnlyContext = createContext(false)

export function ReadOnlyProvider({ readOnly, children }: { readOnly: boolean; children: ReactNode }) {
  return (
    <ReadOnlyContext.Provider value={readOnly}>
      {children}
    </ReadOnlyContext.Provider>
  )
}

export function useReadOnly(): boolean {
  return useContext(ReadOnlyContext)
}
