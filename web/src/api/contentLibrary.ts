// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost, apiPostVoid, apiDelete } from './client'

export interface Library {
  id: string
  name: string
  description?: string
  library_type: 'local' | 'subscribed'
  storage_path: string
  publish_url?: string
  subscribe_url?: string
  item_count: number
  total_size_bytes: number
  auto_sync: boolean
  last_sync?: string
  status: string
  created: string
  updated?: string
}

export interface LibraryItem {
  id: string
  library_id: string
  name: string
  description?: string
  item_type: 'template' | 'iso' | 'ovf' | 'script' | 'file'
  version: string
  size_bytes: number
  content_hash?: string
  tags?: string[]
  metadata?: Record<string, string>
  created: string
  updated?: string
}

export interface GuestCustomizationSpec {
  id: string
  name: string
  description?: string
  os_type: 'linux' | 'windows'
  hostname_prefix?: string
  domain?: string
  dns_servers?: string[]
  ntp_servers?: string[]
  timezone?: string
  network_config?: {
    dhcp: boolean
    static_ip?: string
    subnet_mask?: string
    gateway?: string
  }
  ssh_keys?: string[]
  run_once_commands?: string[]
  admin_password_hash?: string
  created: string
  updated?: string
}

export interface HostProfile {
  id: string
  name: string
  description?: string
  reference_host_id?: string
  settings: Record<string, unknown>
  compliant_hosts: number
  non_compliant_hosts: number
  status: string
  created: string
  updated?: string
}

const API_BASE = '/api'

// Libraries

export async function listLibraries(): Promise<Library[]> {
  return apiGet<Library[]>(`${API_BASE}/content-library/libraries`)
}

export async function createLibrary(req: {
  name: string
  description?: string
  library_type: 'local' | 'subscribed'
  storage_path: string
  subscribe_url?: string
  auto_sync?: boolean
}): Promise<Library> {
  return apiPost<Library>(`${API_BASE}/content-library/libraries`, req)
}

export async function deleteLibrary(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/content-library/libraries/${id}`)
}

export async function syncLibrary(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/content-library/libraries/${id}/sync`)
}

// Library items

export async function listLibraryItems(libraryId: string): Promise<LibraryItem[]> {
  return apiGet<LibraryItem[]>(`${API_BASE}/content-library/libraries/${libraryId}/items`)
}

export async function addLibraryItem(libraryId: string, req: {
  name: string
  description?: string
  item_type: 'template' | 'iso' | 'ovf' | 'script' | 'file'
  tags?: string[]
  metadata?: Record<string, string>
}): Promise<LibraryItem> {
  return apiPost<LibraryItem>(`${API_BASE}/content-library/libraries/${libraryId}/items`, req)
}

export async function deleteLibraryItem(_libraryId: string, itemId: string): Promise<void> {
  return apiDelete(`${API_BASE}/content-library/items/${itemId}`)
}

export async function searchItems(query: string, libraryId?: string): Promise<LibraryItem[]> {
  const params = new URLSearchParams({ q: query })
  if (libraryId) params.append('library_id', libraryId)
  return apiGet<LibraryItem[]>(`${API_BASE}/content-library/items/search?${params}`)
}

// Guest customization specs

export async function listCustomizationSpecs(): Promise<GuestCustomizationSpec[]> {
  return apiGet<GuestCustomizationSpec[]>(`${API_BASE}/content-library/customization-specs`)
}

export async function createCustomizationSpec(req: {
  name: string
  description?: string
  os_type: 'linux' | 'windows'
  hostname_prefix?: string
  domain?: string
  dns_servers?: string[]
  ntp_servers?: string[]
  timezone?: string
  network_config?: {
    dhcp: boolean
    static_ip?: string
    subnet_mask?: string
    gateway?: string
  }
  ssh_keys?: string[]
  run_once_commands?: string[]
}): Promise<GuestCustomizationSpec> {
  return apiPost<GuestCustomizationSpec>(`${API_BASE}/content-library/customization-specs`, req)
}

export async function deleteCustomizationSpec(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/content-library/customization-specs/${id}`)
}

// Host profiles

export async function listHostProfiles(): Promise<HostProfile[]> {
  return apiGet<HostProfile[]>(`${API_BASE}/content-library/host-profiles`)
}

export async function createHostProfile(req: {
  name: string
  description?: string
  reference_host_id?: string
  settings: Record<string, unknown>
}): Promise<HostProfile> {
  return apiPost<HostProfile>(`${API_BASE}/content-library/host-profiles`, req)
}

export async function deleteHostProfile(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/content-library/host-profiles/${id}`)
}

export async function checkHostCompliance(profileId: string, hostId: string, currentConfig: Record<string, unknown>): Promise<{
  compliant: boolean
  deviations: Array<{
    setting: string
    expected: string
    actual: string
  }>
  checked_at: string
}> {
  return apiPost<{
    compliant: boolean
    deviations: Array<{
      setting: string
      expected: string
      actual: string
    }>
    checked_at: string
  }>(`${API_BASE}/content-library/host-profiles/${profileId}/compliance`, { host_id: hostId, current_config: currentConfig })
}
