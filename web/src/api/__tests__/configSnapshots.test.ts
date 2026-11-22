// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('../client', () => ({
  apiGet: vi.fn().mockResolvedValue({}),
  apiPost: vi.fn().mockResolvedValue({}),
}))

import { apiGet, apiPost } from '../client'
import { createConfigSnapshot, getConfigDiff, listConfigSnapshots } from '../configSnapshots'

beforeEach(() => vi.clearAllMocks())

describe('config snapshots api', () => {
  it('lists, stores and diffs against the right routes', async () => {
    await listConfigSnapshots()
    await createConfigSnapshot()
    await getConfigDiff('snap-1', 'current')
    expect(apiGet).toHaveBeenNthCalledWith(1, '/api/config/snapshots')
    expect(apiPost).toHaveBeenCalledWith('/api/config/snapshots')
    expect(apiGet).toHaveBeenNthCalledWith(2, '/api/config/diff?from=snap-1&to=current')
  })
  it('encodes ids in the query string', async () => {
    await getConfigDiff('a&b', 'c d')
    expect(apiGet).toHaveBeenCalledWith('/api/config/diff?from=a%26b&to=c+d')
  })
})
