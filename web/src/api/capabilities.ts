// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet } from './client'

export type SubsystemPhase = 'off' | 'unreachable' | 'live'

export interface SubsystemStatus {
  phase: SubsystemPhase
  detail?: string
}

export interface Capabilities {
  vm_driver: SubsystemStatus
  storage: SubsystemStatus
  network_security: SubsystemStatus
  /** FluxVM Network Fabric schema v4 (TC/eBPF) — not Fabric SDN network-policies */
  vm_dataplane: SubsystemStatus
  auth: SubsystemStatus
  events: SubsystemStatus
  /** External Hubble UI URL when network.hubble_ui_url is set */
  hubble_ui_url?: string
}

export function getCapabilities(): Promise<Capabilities> {
  return apiGet<Capabilities>('/api/capabilities')
}
