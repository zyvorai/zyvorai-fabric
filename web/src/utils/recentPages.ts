// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

const RECENT_KEY = 'zyvor-fabricd-recent-pages'
const MAX_RECENT = 8

/** VM detail routes use recentVMs instead. */
function isVmDetailPath(path: string): boolean {
  return /^\/vms\/[^/]+(\/console)?$/.test(path)
}

function loadRecent(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as unknown
    return Array.isArray(parsed) ? parsed.filter((p): p is string => typeof p === 'string') : []
  } catch {
    return []
  }
}

function saveRecent(paths: string[]) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(paths))
  } catch {
    /* ignore */
  }
}

export function recordRecentPage(path: string) {
  const normalized = path.split('?')[0] || '/'
  if (!normalized.startsWith('/') || normalized === '/login' || isVmDetailPath(normalized)) return

  const current = loadRecent().filter((p) => p !== normalized)
  current.unshift(normalized)
  saveRecent(current.slice(0, MAX_RECENT))
}

export function getRecentPages(): string[] {
  return loadRecent()
}
