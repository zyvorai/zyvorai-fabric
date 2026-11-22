// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useMemo, useState } from 'react'

/**
 * Free-text filter over a list, matching against whatever fields `toSearchable`
 * pulls out of each item. Extracted from VMList's original inline search so
 * every table gets the same behavior (case-insensitive substring match across
 * multiple fields) instead of each page reinventing it.
 */
export function useTableFilter<T>(items: T[], toSearchable: (item: T) => Array<string | undefined | null>) {
  const [query, setQuery] = useState('')

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return items
    return items.filter((item) =>
      toSearchable(item).some((field) => (field ?? '').toLowerCase().includes(q))
    )
  }, [items, query, toSearchable])

  return { query, setQuery, filtered }
}
