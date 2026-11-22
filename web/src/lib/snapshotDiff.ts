// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

// Pure helpers for presenting a configuration snapshot diff.

export type ChangeKind = 'added' | 'removed' | 'changed'

export interface SnapshotChange {
  section: string
  key: string
  kind: ChangeKind
  field?: string
  old?: unknown
  new?: unknown
}

export interface ChangeGroup {
  section: string
  key: string
  /** Net status of the entity: added/removed as a whole, otherwise changed. */
  kind: ChangeKind
  changes: SnapshotChange[]
}

export interface ChangeSummary {
  added: number
  removed: number
  changed: number
  total: number
}

export const SECTION_LABELS: Record<string, string> = {
  vms: 'Virtual machines',
  network_policies: 'Network policies',
  storage_pools: 'Storage pools',
}

const SECTION_ORDER = ['vms', 'network_policies', 'storage_pools']

export const REDACTED = '***REDACTED***'

export function sectionLabel(section: string): string {
  return SECTION_LABELS[section] ?? section
}

/** Count entities added / removed and individual field changes. */
export function summarizeChanges(changes: SnapshotChange[]): ChangeSummary {
  const s: ChangeSummary = { added: 0, removed: 0, changed: 0, total: changes.length }
  for (const c of changes) s[c.kind] += 1
  return s
}

/** Group changes per entity, ordered by section (known sections first) then key. */
export function groupChanges(changes: SnapshotChange[]): ChangeGroup[] {
  const map = new Map<string, ChangeGroup>()
  for (const c of changes) {
    const id = `${c.section}\u0000${c.key}`
    let g = map.get(id)
    if (!g) {
      g = { section: c.section, key: c.key, kind: c.kind, changes: [] }
      map.set(id, g)
    }
    g.changes.push(c)
  }
  const rank = (s: string) => {
    const i = SECTION_ORDER.indexOf(s)
    return i === -1 ? SECTION_ORDER.length : i
  }
  return [...map.values()].sort(
    (a, b) =>
      rank(a.section) - rank(b.section) ||
      a.section.localeCompare(b.section) ||
      a.key.localeCompare(b.key),
  )
}

/** Compact one-line rendering of a JSON value; long values are truncated. */
export function formatValue(v: unknown, max = 80): string {
  if (v === undefined) return '(none)'
  let s: string
  if (typeof v === 'string') s = v
  else {
    try {
      s = JSON.stringify(v)
    } catch {
      s = String(v)
    }
  }
  return s.length > max ? `${s.slice(0, max - 1)}…` : s
}

export function isRedacted(v: unknown): boolean {
  return v === REDACTED
}

/** One-line description of a single change. */
export function describeChange(c: SnapshotChange): string {
  if (c.kind === 'added') return `added ${c.key}`
  if (c.kind === 'removed') return `removed ${c.key}`
  return `${c.field ?? c.key}: ${formatValue(c.old)} → ${formatValue(c.new)}`
}
