// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  getToken,
  setToken,
  clearToken,
  apiFetch,
  apiGet,
  apiPost,
  apiPostVoid,
  apiPut,
  apiPutVoid,
  apiDelete,
} from '../client'

// Mock localStorage
const storage = new Map<string, string>()
const localStorageMock = {
  getItem: vi.fn((key: string) => storage.get(key) ?? null),
  setItem: vi.fn((key: string, value: string) => storage.set(key, value)),
  removeItem: vi.fn((key: string) => storage.delete(key)),
}
Object.defineProperty(globalThis, 'localStorage', { value: localStorageMock, writable: true })

// Mock window.location
const locationMock = { pathname: '/dashboard', href: '' }
Object.defineProperty(globalThis, 'window', {
  value: { ...globalThis.window, location: locationMock },
  writable: true,
})

// Mock fetch
const fetchMock = vi.fn()
globalThis.fetch = fetchMock

function okJson(data: unknown): Response {
  const body = JSON.stringify(data)
  return {
    ok: true,
    status: 200,
    statusText: 'OK',
    headers: new Headers({ 'content-type': 'application/json' }),
    text: () => Promise.resolve(body),
    json: () => Promise.resolve(data),
    blob: () => Promise.resolve(new Blob()),
  } as unknown as Response
}

function failResponse(status = 500): Response {
  return {
    ok: false,
    status,
    statusText: 'Internal Server Error',
    json: () => Promise.resolve({ error: 'fail' }),
    text: () => Promise.resolve('server error'),
  } as unknown as Response
}

beforeEach(() => {
  storage.clear()
  fetchMock.mockReset()
  locationMock.pathname = '/dashboard'
  locationMock.href = ''
})

// ─── Token management ─────────────────────────────────────────────────────────

describe('Token management', () => {
  it('getToken returns null when no token is stored', () => {
    expect(getToken()).toBeNull()
  })

  it('setToken stores and getToken retrieves the token', () => {
    setToken('abc123')
    expect(getToken()).toBe('abc123')
  })

  it('clearToken removes the token', () => {
    setToken('abc123')
    clearToken()
    expect(getToken()).toBeNull()
  })
})

// ─── apiFetch ─────────────────────────────────────────────────────────────────

describe('apiFetch', () => {
  it('injects Bearer token into Authorization header', async () => {
    setToken('mytoken')
    fetchMock.mockResolvedValue(okJson({}))

    await apiFetch('/api/test')

    const [, init] = fetchMock.mock.calls[0]
    const headers = init.headers as Headers
    expect(headers.get('Authorization')).toBe('Bearer mytoken')
  })

  it('does not set Authorization header when no token exists', async () => {
    fetchMock.mockResolvedValue(okJson({}))

    await apiFetch('/api/test')

    const [, init] = fetchMock.mock.calls[0]
    const headers = init.headers as Headers
    expect(headers.has('Authorization')).toBe(false)
  })

  it('passes through init options', async () => {
    fetchMock.mockResolvedValue(okJson({}))

    await apiFetch('/api/test', {
      method: 'POST',
      body: '{"a":1}',
    })

    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/test')
    expect(init.method).toBe('POST')
    expect(init.body).toBe('{"a":1}')
  })

  it('clears token and redirects on 401', async () => {
    setToken('oldtoken')
    fetchMock.mockResolvedValue(failResponse(401))

    await apiFetch('/api/test')

    expect(getToken()).toBeNull()
    expect(locationMock.href).toBe('/sign-in')
  })

  it('does not redirect to /login if already on /login', async () => {
    setToken('oldtoken')
    locationMock.pathname = '/login'
    fetchMock.mockResolvedValue(failResponse(401))

    await apiFetch('/api/test')

    expect(getToken()).toBeNull()
    expect(locationMock.href).toBe('')
  })

  it('returns the response object', async () => {
    const res = okJson({ x: 1 })
    fetchMock.mockResolvedValue(res)

    const result = await apiFetch('/api/test')
    expect(result).toBe(res)
  })

  it('merges custom headers with auth header', async () => {
    setToken('tok')
    fetchMock.mockResolvedValue(okJson({}))

    await apiFetch('/api/test', {
      headers: { 'Content-Type': 'application/json' },
    })

    const [, init] = fetchMock.mock.calls[0]
    const headers = init.headers as Headers
    expect(headers.get('Authorization')).toBe('Bearer tok')
    expect(headers.get('Content-Type')).toBe('application/json')
  })
})

// ─── apiGet ───────────────────────────────────────────────────────────────────

describe('apiGet', () => {
  it('returns parsed JSON on success', async () => {
    fetchMock.mockResolvedValue(okJson({ items: [1, 2] }))
    const result = await apiGet('/api/items')
    expect(result).toEqual({ items: [1, 2] })
  })

  it('throws on non-ok response', async () => {
    fetchMock.mockResolvedValue(failResponse())
    await expect(apiGet('/api/fail')).rejects.toThrow(/server error/)
  })

  it('does not send a request body', async () => {
    fetchMock.mockResolvedValue(okJson({}))
    await apiGet('/api/x')
    const [, init] = fetchMock.mock.calls[0]
    expect(init.body).toBeUndefined()
    expect(init.method).toBeUndefined()
  })
})

// ─── apiPost ──────────────────────────────────────────────────────────────────

describe('apiPost', () => {
  it('sends JSON body with Content-Type and returns parsed JSON', async () => {
    fetchMock.mockResolvedValue(okJson({ id: '1' }))
    const result = await apiPost('/api/create', { name: 'test' })

    expect(result).toEqual({ id: '1' })
    const [, init] = fetchMock.mock.calls[0]
    expect(init.method).toBe('POST')
    expect(init.body).toBe('{"name":"test"}')
    const headers = init.headers as Headers
    expect(headers.get('Content-Type')).toBe('application/json')
  })

  it('sends no body or Content-Type when body is undefined', async () => {
    fetchMock.mockResolvedValue(okJson({ ok: true }))
    await apiPost('/api/action')

    const [, init] = fetchMock.mock.calls[0]
    expect(init.method).toBe('POST')
    expect(init.body).toBeUndefined()
    const headers = init.headers as Headers
    expect(headers.has('Content-Type')).toBe(false)
  })

  it('throws on non-ok response', async () => {
    fetchMock.mockResolvedValue(failResponse())
    await expect(apiPost('/api/fail', {})).rejects.toThrow(/server error/)
  })
})

// ─── apiPostVoid ──────────────────────────────────────────────────────────────

describe('apiPostVoid', () => {
  it('sends POST and returns void on success', async () => {
    fetchMock.mockResolvedValue(okJson(null))
    const result = await apiPostVoid('/api/action')
    expect(result).toBeUndefined()
  })

  it('sends JSON body when provided', async () => {
    fetchMock.mockResolvedValue(okJson(null))
    await apiPostVoid('/api/action', { key: 'val' })

    const [, init] = fetchMock.mock.calls[0]
    expect(init.method).toBe('POST')
    expect(init.body).toBe('{"key":"val"}')
    const headers = init.headers as Headers
    expect(headers.get('Content-Type')).toBe('application/json')
  })

  it('throws on non-ok response', async () => {
    fetchMock.mockResolvedValue(failResponse())
    await expect(apiPostVoid('/api/fail')).rejects.toThrow(/server error/)
  })
})

// ─── apiPut ───────────────────────────────────────────────────────────────────

describe('apiPut', () => {
  it('sends PUT with JSON body and returns parsed JSON', async () => {
    fetchMock.mockResolvedValue(okJson({ updated: true }))
    const result = await apiPut('/api/item/1', { name: 'updated' })

    expect(result).toEqual({ updated: true })
    const [, init] = fetchMock.mock.calls[0]
    expect(init.method).toBe('PUT')
    expect(init.body).toBe('{"name":"updated"}')
    const headers = init.headers as Headers
    expect(headers.get('Content-Type')).toBe('application/json')
  })

  it('throws on non-ok response', async () => {
    fetchMock.mockResolvedValue(failResponse())
    await expect(apiPut('/api/fail', {})).rejects.toThrow(/server error/)
  })
})

// ─── apiPutVoid ───────────────────────────────────────────────────────────────

describe('apiPutVoid', () => {
  it('sends PUT with JSON body and returns void', async () => {
    fetchMock.mockResolvedValue(okJson(null))
    const result = await apiPutVoid('/api/item/1', { name: 'x' })

    expect(result).toBeUndefined()
    const [, init] = fetchMock.mock.calls[0]
    expect(init.method).toBe('PUT')
    expect(init.body).toBe('{"name":"x"}')
  })

  it('throws on non-ok response', async () => {
    fetchMock.mockResolvedValue(failResponse())
    await expect(apiPutVoid('/api/fail', {})).rejects.toThrow(/server error/)
  })
})

// ─── apiDelete ────────────────────────────────────────────────────────────────

describe('apiDelete', () => {
  it('sends DELETE and returns void on success', async () => {
    fetchMock.mockResolvedValue(okJson(null))
    const result = await apiDelete('/api/item/1')

    expect(result).toBeUndefined()
    const [, init] = fetchMock.mock.calls[0]
    expect(init.method).toBe('DELETE')
  })

  it('throws on non-ok response', async () => {
    fetchMock.mockResolvedValue(failResponse())
    await expect(apiDelete('/api/fail')).rejects.toThrow(/server error/)
  })
})
