// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { apiGet, apiPost, apiPut, apiDelete } from './client'

const API_BASE = '/api/federation'

export interface FederationPeer {
  id: string
  name: string
  url: string
  token_set: boolean
  token_available: boolean
  created_at: string
}

export type PeerStatus = 'ok' | 'unreachable' | 'unauthorized' | 'token_missing' | 'error'

export interface PeerHost {
  id: string
  hostname: string
  status: string
  cpus: number
  memory_mb: number
  vm_count: number
  cpu_usage_pct: number
  memory_usage_pct: number
}

export interface PeerOverview {
  id: string
  name: string
  url: string
  status: PeerStatus
  latency_ms: number
  message?: string
  vm_total?: number | null
  totals?: { hosts: number; hosts_in_maintenance: number; cpus: number; memory_mb: number }
  hosts?: PeerHost[]
}

export interface FederationOverview {
  generated_at: string
  totals: { peers: number; reachable: number; hosts: number; hosts_in_maintenance: number; vms: number }
  peers: PeerOverview[]
}

export function getFederationOverview(): Promise<FederationOverview> {
  return apiGet<FederationOverview>(`${API_BASE}/overview`)
}

export function listFederationPeers(): Promise<FederationPeer[]> {
  return apiGet<FederationPeer[]>(`${API_BASE}/peers`)
}

export function registerFederationPeer(req: { name: string; url: string; token?: string }): Promise<FederationPeer> {
  return apiPost<FederationPeer>(`${API_BASE}/peers`, req)
}

export function setFederationPeerToken(id: string, token: string): Promise<FederationPeer> {
  return apiPut<FederationPeer>(`${API_BASE}/peers/${encodeURIComponent(id)}/token`, { token })
}

export function deleteFederationPeer(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/peers/${encodeURIComponent(id)}`)
}

/** Forwarded to the peer with your own token; the peer decides whether you may. */
export function setPeerHostMaintenance(peerId: string, hostId: string, enter: boolean): Promise<{ peer_status: number }> {
  return apiPost<{ peer_status: number }>(
    `${API_BASE}/peers/${encodeURIComponent(peerId)}/hosts/${encodeURIComponent(hostId)}/maintenance/${enter ? 'enter' : 'exit'}`,
  )
}
