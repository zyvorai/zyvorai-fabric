// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet } from './client'

export interface LoginRequest {
  username: string
  password: string
}

export interface LoginResponse {
  token: string
  user_id: string
  role: string
  username: string
  tenant?: string | null
}

export interface UserInfo {
  id: string
  username: string
  role: string
  tenant?: string | null
}

const API_BASE = '/api'

export async function login(req: LoginRequest): Promise<LoginResponse> {
  const res = await fetch(`${API_BASE}/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  })
  if (!res.ok) {
    if (res.status === 401) throw new Error('Invalid username or password')
    throw new Error('Login failed')
  }
  return res.json()
}

export async function getMe(): Promise<UserInfo> {
  return apiGet<UserInfo>(`${API_BASE}/auth/me`)
}
