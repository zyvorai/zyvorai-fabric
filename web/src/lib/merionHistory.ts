// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import type { ArtifactItem, ApprovalItem } from '../api/agents'

/** Order two picked artifacts as [older, newer] so the diff reads forward in time. */
export function orderPair(a: ArtifactItem, b: ArtifactItem): [ArtifactItem, ArtifactItem] {
  return Date.parse(a.created_at) <= Date.parse(b.created_at) ? [a, b] : [b, a]
}

/** Toggle an artifact in the selection; at most two are kept, the oldest pick drops first. */
export function toggleSelection(selected: string[], id: string): string[] {
  if (selected.includes(id)) return selected.filter((s) => s !== id)
  return [...selected, id].slice(-2)
}

/** Use-case ids present in the list, sorted, for the filter dropdown. */
export function demoIdsOf(items: ArtifactItem[]): string[] {
  return [...new Set(items.map((i) => i.metadata?.demo).filter((d): d is string => !!d))].sort()
}

export type ApprovalFilter = 'all' | 'pending' | 'decided'

export function filterApprovals(items: ApprovalItem[], filter: ApprovalFilter): ApprovalItem[] {
  if (filter === 'pending') return items.filter((a) => a.status === 'pending')
  if (filter === 'decided') return items.filter((a) => a.status !== 'pending')
  return items
}

/** One line the operator can trust at a glance. */
export function chainLabel(chain: { entries: number; chain_ok: boolean; broken_at?: number | null }): string {
  return chain.chain_ok
    ? `Hash chain intact · ${chain.entries} entries`
    : `Hash chain BROKEN at entry ${chain.broken_at ?? '?'} of ${chain.entries}`
}
