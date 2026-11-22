// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost } from './client'

const API_BASE = '/api'

export type RescueRequest =
  | { operation: 'inject-ssh-key'; user: string; key: string }
  | { operation: 'enable-ssh' }
  | { operation: 'set-hostname'; hostname: string }
  | { operation: 'reset-password'; user: string; password: string }
  | { operation: 'install-packages'; packages: string[]; network?: boolean }

/** Offline guest configuration via GuestKit -- mounts the VM's disk
    directly, no in-guest agent or network needed. The VM must be stopped
    first (GuestKit needs exclusive access; a running VM already holds the
    disk lock). Note: GuestKit's static-IP support is Windows-only -- there
    is no equivalent Linux operation, so this never claims to set one. */
export async function rescueVM(vmName: string, req: RescueRequest): Promise<{ status: string }> {
  return apiPost<{ status: string }>(`${API_BASE}/vms/${encodeURIComponent(vmName)}/rescue`, req)
}

export async function inspectVM(vmName: string): Promise<Record<string, unknown>> {
  return apiGet<Record<string, unknown>>(`${API_BASE}/vms/${encodeURIComponent(vmName)}/inspect`)
}
