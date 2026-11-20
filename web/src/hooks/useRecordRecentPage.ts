// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { useEffect } from 'react'
import { useLocation } from 'react-router'
import { recordRecentPage } from '../utils/recentPages'

/** Records the current route for command palette "Recent pages". */
export function useRecordRecentPage() {
  const location = useLocation()

  useEffect(() => {
    recordRecentPage(location.pathname + location.search)
  }, [location.pathname, location.search])
}
