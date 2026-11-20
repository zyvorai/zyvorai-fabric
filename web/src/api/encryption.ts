// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiPostVoid, apiDelete } from './client'

export interface KeyProvider {
  id: string
  name: string
  provider_type: string
  endpoint?: string
  status: string
  created: string
  updated?: string
}

export interface EncryptionPolicy {
  id: string
  name: string
  description?: string
  key_provider_id: string
  algorithm: string
  encrypt_vmotion: boolean
  auto_rotate_days?: number
  created: string
  updated?: string
}

export interface VmEncryptionStatus {
  vm_name: string
  encrypted: boolean
  policy_id?: string
  algorithm?: string
  key_id?: string
  vmotion_encrypted: boolean
  last_key_rotation?: string
}

const API_BASE = '/api'

// Key providers

export async function listProviders(): Promise<KeyProvider[]> {
  return apiGet<KeyProvider[]>(`${API_BASE}/encryption/providers`)
}

export async function registerProvider(req: {
  name: string
  provider_type: string
  endpoint: string
}): Promise<KeyProvider> {
  return apiPost<KeyProvider>(`${API_BASE}/encryption/providers`, { ...req, status: 'connected' })
}

export async function removeProvider(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/encryption/providers/${id}`)
}

// Encryption policies

export async function listEncryptionPolicies(): Promise<EncryptionPolicy[]> {
  return apiGet<EncryptionPolicy[]>(`${API_BASE}/encryption/policies`)
}

export async function createEncryptionPolicy(req: {
  name: string
  description?: string
  key_provider_id: string
  algorithm: string
  encrypt_vmotion?: boolean
  auto_rotate_days?: number
}): Promise<EncryptionPolicy> {
  return apiPost<EncryptionPolicy>(`${API_BASE}/encryption/policies`, { encrypt_vmotion: false, ...req })
}

// VM encryption operations

export async function encryptVm(vmName: string, policyId: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/encryption/vms/${vmName}/encrypt`, { vm_name: vmName, policy_id: policyId })
}

export async function decryptVm(vmName: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/encryption/vms/${vmName}/decrypt`, { vm_name: vmName })
}

export async function getVmEncryptionStatus(vmName: string): Promise<VmEncryptionStatus> {
  return apiGet<VmEncryptionStatus>(`${API_BASE}/encryption/vms/${vmName}/status`)
}

export async function listEncryptedVms(): Promise<VmEncryptionStatus[]> {
  return apiGet<VmEncryptionStatus[]>(`${API_BASE}/encryption/vms`)
}

export async function rotateVmKey(vmName: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/encryption/vms/${vmName}/rotate-key`)
}
