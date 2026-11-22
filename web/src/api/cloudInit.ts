// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiPostVoid } from './client'
import { API_BASE_URL } from './config'

export interface CloudInitConfig {
  instance_id: string
  hostname: string
  user_data?: string | null
  meta_data?: Record<string, unknown> | null
  network_config?: Record<string, unknown> | null
}

export async function configureCloudInit(vmName: string, config: CloudInitConfig): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/cloud-init`, config)
}
