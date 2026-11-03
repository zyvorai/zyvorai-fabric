// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('../client', () => ({
  apiGet: vi.fn().mockResolvedValue({}),
  apiPost: vi.fn().mockResolvedValue({}),
  apiPut: vi.fn().mockResolvedValue({}),
  apiDelete: vi.fn().mockResolvedValue(undefined),
}))

import { apiGet, apiPost, apiPut, apiDelete } from '../client'
import {
  getFederationOverview,
  listFederationPeers,
  registerFederationPeer,
  setFederationPeerToken,
  deleteFederationPeer,
  setPeerHostMaintenance,
} from '../federation'
import { PEER_STATUS_LABEL } from '../../pages/Federation'

beforeEach(() => vi.clearAllMocks())

describe('federation api', () => {
  it('reads the overview and the peer list', async () => {
    await getFederationOverview()
    await listFederationPeers()
    expect(apiGet).toHaveBeenNthCalledWith(1, '/api/federation/overview')
    expect(apiGet).toHaveBeenNthCalledWith(2, '/api/federation/peers')
  })

  it('registers a peer, sets its token and removes it', async () => {
    await registerFederationPeer({ name: 'lab', url: 'https://b.example:9095', token: 't' })
    expect(apiPost).toHaveBeenCalledWith('/api/federation/peers', { name: 'lab', url: 'https://b.example:9095', token: 't' })
    await setFederationPeerToken('p 1', 'new')
    expect(apiPut).toHaveBeenCalledWith('/api/federation/peers/p%201/token', { token: 'new' })
    await deleteFederationPeer('p1')
    expect(apiDelete).toHaveBeenCalledWith('/api/federation/peers/p1')
  })

  it('forwards maintenance as enter or exit and escapes ids', async () => {
    await setPeerHostMaintenance('p1', 'h/../x', true)
    await setPeerHostMaintenance('p1', 'h1', false)
    expect(apiPost).toHaveBeenNthCalledWith(1, '/api/federation/peers/p1/hosts/h%2F..%2Fx/maintenance/enter')
    expect(apiPost).toHaveBeenNthCalledWith(2, '/api/federation/peers/p1/hosts/h1/maintenance/exit')
  })

  it('has a label for every status the backend can return', () => {
    for (const s of ['ok', 'unreachable', 'unauthorized', 'token_missing', 'error']) {
      expect(PEER_STATUS_LABEL[s]).toBeTruthy()
    }
  })
})
