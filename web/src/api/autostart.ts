// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost } from './client'

export type AutostartRunState = 'running' | 'completed' | 'failed'
export type AutostartVmStatus =
  | 'pending'
  | 'checking'
  | 'waiting'
  | 'starting'
  | 'started'
  | 'skipped'
  | 'failed'

export interface AutostartRunSummary {
  id: string
  trigger: 'startup' | 'manual'
  state: AutostartRunState
  started_at: string
  finished_at?: string
  candidates: number
  started: number
  skipped: number
  failed: number
}

export interface AutostartVmProgress {
  vm: string
  order: number
  status: AutostartVmStatus
  code?: string
  reason?: string
}

export interface AutostartRun {
  id: string
  trigger: 'startup' | 'manual'
  state: AutostartRunState
  started_at: string
  finished_at?: string
  vms: AutostartVmProgress[]
  report?: {
    summary: { candidates: number; started: number; skipped: number; failed: number }
  }
  error?: string
}

export function listAutostartRuns(): Promise<{ runs: AutostartRunSummary[] }> {
  return apiGet('/api/vms/autostart/reconcile')
}

export function getAutostartRun(id: string): Promise<AutostartRun> {
  return apiGet(`/api/vms/autostart/reconcile/${encodeURIComponent(id)}`)
}

/** Start a live run; resolves with the accepted job (HTTP 202). */
export function startAutostartRun(): Promise<{ id: string; state: AutostartRunState }> {
  return apiPost('/api/vms/autostart/reconcile')
}
