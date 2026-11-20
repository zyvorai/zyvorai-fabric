// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiDelete } from './client'

const API_BASE = '/api'

export interface VMProfile {
  name: string
  description: string
  cpus: number
  memory: number
  disk: number
  category: 'general' | 'compute' | 'memory' | 'storage' | 'gpu'
  network_bandwidth?: string
  builtin: boolean
}

export async function listProfiles(): Promise<VMProfile[]> {
  return apiGet<VMProfile[]>(`${API_BASE}/profiles`)
}

export async function createProfile(req: Omit<VMProfile, 'builtin'>): Promise<VMProfile> {
  return apiPost<VMProfile>(`${API_BASE}/profiles`, req)
}

export async function deleteProfile(name: string): Promise<void> {
  return apiDelete(`${API_BASE}/profiles/${name}`)
}
