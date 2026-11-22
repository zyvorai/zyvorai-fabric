// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { navGroups, routeLabels, flattenNavGroup } from './routes'

/** Human-readable label for an app route path. */
export function getPageLabel(path: string): string {
  const base = path.split('?')[0] || '/'
  if (routeLabels[base]) return routeLabels[base]

  for (const group of navGroups) {
    for (const item of flattenNavGroup(group)) {
      const itemPath = item.path.split('?')[0]
      if (itemPath === base) return item.label
    }
  }

  const segments = base.split('/').filter(Boolean)
  const slug = segments[segments.length - 1] || 'Page'
  return slug.replace(/-/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}
