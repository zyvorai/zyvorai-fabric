// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost, apiDelete } from './client'
import { API_BASE_URL } from './config'

export interface Volume {
  id: string
  pool_name: string
  name: string
  size: string
  vm_attached: string | null
  created: string
  updated: string
}

export interface CreateVolumeRequest {
  name: string
  size: string
}

export interface ResizeVolumeRequest {
  size: string
}

export interface AttachVolumeRequest {
  vm_name: string
}

export async function listVolumes(poolName: string): Promise<Volume[]> {
  return apiGet<Volume[]>(`${API_BASE_URL}/storage/pools/${poolName}/volumes`)
}

export async function createVolume(poolName: string, req: CreateVolumeRequest): Promise<Volume> {
  return apiPost<Volume>(`${API_BASE_URL}/storage/pools/${poolName}/volumes`, req)
}

export async function getVolume(poolName: string, id: string): Promise<Volume> {
  return apiGet<Volume>(`${API_BASE_URL}/storage/pools/${poolName}/volumes/${id}`)
}

export async function deleteVolume(poolName: string, id: string): Promise<void> {
  return apiDelete(`${API_BASE_URL}/storage/pools/${poolName}/volumes/${id}`)
}

export async function resizeVolume(poolName: string, id: string, req: ResizeVolumeRequest): Promise<Volume> {
  return apiPost<Volume>(`${API_BASE_URL}/storage/pools/${poolName}/volumes/${id}/resize`, req)
}

export async function attachVolume(poolName: string, id: string, req: AttachVolumeRequest): Promise<Volume> {
  return apiPost<Volume>(`${API_BASE_URL}/storage/pools/${poolName}/volumes/${id}/attach`, req)
}

export async function detachVolume(poolName: string, id: string): Promise<Volume> {
  return apiPost<Volume>(`${API_BASE_URL}/storage/pools/${poolName}/volumes/${id}/detach`)
}
