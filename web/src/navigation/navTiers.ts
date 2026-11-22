// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import type { NavGroup, NavItem, NavSection } from './navConfig'

export type NavTier = 'core' | 'integrate' | 'labs'

const LABS_PATHS = new Set([
  '/cost-estimator',
  '/api-playground',
  '/debug',
  '/event-stream',
])

const INTEGRATE_PATHS = new Set([
  '/site-recovery',
  '/distributed-storage',
  '/replication',
  '/service-map',
  '/certificates',
  '/encryption',
])

export function navTierForPath(path: string): NavTier {
  if (LABS_PATHS.has(path)) return 'labs'
  if (INTEGRATE_PATHS.has(path)) return 'integrate'
  return 'core'
}

export function canSeeLabs(role: string | null | undefined): boolean {
  return role === 'admin'
}

export function canSeeIntegrate(role: string | null | undefined): boolean {
  return role === 'admin' || role === 'operator'
}

function filterItems(items: NavItem[], role: string | null | undefined): NavItem[] {
  return items.filter((item) => {
    const tier = navTierForPath(item.path)
    if (tier === 'labs' && !canSeeLabs(role)) return false
    if (tier === 'integrate' && !canSeeIntegrate(role)) return false
    return true
  })
}

function filterSections(sections: NavSection[], role: string | null | undefined): NavSection[] {
  return sections
    .map((s) => ({ ...s, items: filterItems(s.items, role) }))
    .filter((s) => s.items.length > 0)
}

/** Filter nav groups by role (admin sees all; viewer sees core only). */
export function filterNavGroups(groups: NavGroup[], role: string | null | undefined): NavGroup[] {
  return groups
    .map((g) => {
      if (g.items?.length) {
        return { ...g, items: filterItems(g.items, role) }
      }
      if (g.sections?.length) {
        return { ...g, sections: filterSections(g.sections, role) }
      }
      return g
    })
    .filter((g) => (g.items?.length ?? 0) > 0 || (g.sections?.some((s) => s.items.length > 0) ?? false))
}
