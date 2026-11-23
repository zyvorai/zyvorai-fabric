// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost } from './client'

export type RiskLevel = 'high' | 'medium' | 'low'

export interface FieldChange {
  path: string
  label: string
  kind: 'added' | 'removed' | 'changed'
  from?: unknown
  to?: unknown
  summary: string
  risk?: { level: RiskLevel; reason: string } | null
  hidden: boolean
}

export interface GoodMark {
  source: 'auto' | 'operator'
  at: string
  by?: string
  note?: string
  healthy_minutes?: number
}

export interface RevisionSummary {
  rev: number
  timestamp: string
  actor: string
  cause: string
  content_hash: string
  rev_hash: string
  truncated: boolean
  audit: { id: string; seq?: number; hash?: string; link: string }
  good: GoodMark | null
  changed_fields: number
  high_risk_changes: number
}

export interface VmHistory {
  vm: string
  exists: boolean
  current_rev: number | null
  last_good_rev: number | null
  retention: { max_revisions: number; kept: number; evicted_total: number }
  good_after_minutes: number
  good_signal: string
  redaction: string
  chain: { status: 'ok' | 'broken' | 'empty'; revisions: number; first_break?: { rev: number; reason: string } }
  truncated_list: boolean
  revisions: RevisionSummary[]
}

export interface RevisionDetail extends RevisionSummary {
  vm: string
  prev_hash: string
  config: Record<string, unknown>
}

export interface Who {
  rev: number
  actor: string
  at: string
  cause: string
}

export interface VmDiff {
  vm: string
  from: Who
  to: Who
  identical: boolean
  identical_content_hash: boolean
  counts: { total: number; high: number; medium: number; low: number }
  summary: string
  changes: FieldChange[]
}

export interface Culprit extends FieldChange {
  rank: number
  changed_in: Who[]
}

export type SinceGood =
  | { vm: string; status: 'no_known_good'; current_rev: number | null; message: string; good_after_minutes: number }
  | { vm: string; status: 'unchanged'; good_rev: number; current_rev: number; good: GoodMark | null; summary: string }
  | {
      vm: string
      status: 'changed'
      good_rev: number
      current_rev: number
      good: GoodMark | null
      summary: string
      counts: { total: number; high: number; medium: number; low: number }
      culprits: Culprit[]
      revisions_since: Who[]
      note: string
    }

const base = (vm: string) => `/api/vms/${encodeURIComponent(vm)}/history`

export function getVmHistory(vm: string, limit?: number): Promise<VmHistory> {
  return apiGet(limit ? `${base(vm)}?limit=${limit}` : base(vm))
}

export function getVmRevision(vm: string, rev: number): Promise<RevisionDetail> {
  return apiGet(`${base(vm)}/${rev}`)
}

export function diffVmRevisions(vm: string, from: number, to?: number): Promise<VmDiff> {
  const q = to === undefined ? `from=${from}` : `from=${from}&to=${to}`
  return apiGet(`${base(vm)}/diff?${q}`)
}

export function getSinceGood(vm: string): Promise<SinceGood> {
  return apiGet(`${base(vm)}/since-good`)
}

export function markRevisionGood(vm: string, rev: number, note?: string): Promise<RevisionSummary> {
  return apiPost(`${base(vm)}/${rev}/mark-good`, note ? { note } : {})
}

export function goodLabel(g: GoodMark | null | undefined): string {
  if (!g) return ''
  if (g.source === 'operator') return `Marked good by ${g.by ?? 'an operator'}`
  return `Good automatically (${g.healthy_minutes ?? 0} min running)`
}

export function riskText(level: RiskLevel): string {
  return level === 'high' ? 'High risk' : level === 'medium' ? 'Medium risk' : 'Low risk'
}
