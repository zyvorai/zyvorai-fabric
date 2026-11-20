// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet } from './client'

const API_BASE = '/api'

export interface PluginInfo {
  name: string
  version: string
  description: string
  plugin_type: 'storage_backend' | 'vm_driver' | 'scheduler' | 'event_hook'
  enabled: boolean
  loaded: boolean
}

export async function listPlugins(): Promise<PluginInfo[]> {
  return apiGet<PluginInfo[]>(`${API_BASE}/plugins`)
}
