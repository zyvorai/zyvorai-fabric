// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiPost } from './client'
import type { PreflightResult } from '../lib/preflight'

export type { PreflightFinding, PreflightResult, PreflightSeverity } from '../lib/preflight'

/** Read-only "can this VM start here?" check. Never starts or changes the VM. */
export function runPreflight(vmName: string): Promise<PreflightResult> {
  return apiPost<PreflightResult>(`/api/vms/${encodeURIComponent(vmName)}/preflight`)
}
