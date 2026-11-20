// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPostVoid, apiDelete } from './client'
import { API_BASE_URL } from './config'

export interface FirmwareStatus {
  firmware_type: string
  code_path: string
  vars_path: string
  secure_boot_enabled: boolean
  tpm_enabled: boolean
  tpm_version: string | null
}

export interface EnableUefiRequest {
  secure_boot: boolean
  tpm_version?: 'V1_2' | 'V2_0'
}

export interface FirmwareCapabilities {
  ovmf_available: boolean
  secureboot_available: boolean
  tpm_available: boolean
}

// Get firmware status for a VM
export async function getFirmwareStatus(vmName: string): Promise<FirmwareStatus> {
  return apiGet<FirmwareStatus>(`${API_BASE_URL}/vms/${vmName}/firmware/status`)
}

// Enable UEFI firmware for a VM
export async function enableUefi(vmName: string, request: EnableUefiRequest): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/firmware/uefi`, request)
}

// Enable Secure Boot for a VM
export async function enableSecureBoot(vmName: string): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/firmware/secureboot`)
}

// Disable Secure Boot for a VM
export async function disableSecureBoot(vmName: string): Promise<void> {
  return apiDelete(`${API_BASE_URL}/vms/${vmName}/firmware/secureboot`)
}

// Reset NVRAM to defaults
export async function resetNvram(vmName: string): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/firmware/reset`)
}

// Get system firmware capabilities
export async function getFirmwareCapabilities(): Promise<FirmwareCapabilities> {
  return apiGet<FirmwareCapabilities>(`${API_BASE_URL}/system/firmware/capabilities`)
}
