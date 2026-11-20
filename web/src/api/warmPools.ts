// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiDelete } from './client'
import type { VM } from './vm'

const API_BASE = '/api'

/** A named pool of VMs pre-booted from a template, then paused, ready to
    be handed out instantly by claimPool instead of a slow cold create. */
export interface WarmPool {
  name: string
  size: number
  image: string
  cpus: number
  memory: number
  ready_members: number
}

export async function listWarmPools(): Promise<WarmPool[]> {
  return apiGet<WarmPool[]>(`${API_BASE}/vm-pools`)
}

export async function getWarmPool(name: string): Promise<WarmPool> {
  return apiGet<WarmPool>(`${API_BASE}/vm-pools/${encodeURIComponent(name)}`)
}

export async function createWarmPool(req: {
  name: string
  size: number
  image: string
  cpus?: number
  memory?: number
}): Promise<WarmPool> {
  return apiPost<WarmPool>(`${API_BASE}/vm-pools`, req)
}

export async function deleteWarmPool(name: string): Promise<void> {
  return apiDelete(`${API_BASE}/vm-pools/${encodeURIComponent(name)}`)
}

/** Instantly resumes one ready (already-booted, paused) member as a real
    VM named `newName`. Fails if the pool has no ready member right now. */
export async function claimWarmPool(poolName: string, newName: string, ttlSeconds?: number): Promise<VM> {
  return apiPost<VM>(`${API_BASE}/vm-pools/${encodeURIComponent(poolName)}/claim`, {
    name: newName,
    ttl_seconds: ttlSeconds,
  })
}
