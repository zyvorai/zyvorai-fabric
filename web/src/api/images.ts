// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost } from './client'

const API_BASE = '/api'

export interface ImageInfo {
  name: string
  path: string
  format: string
  size_bytes: number
}

export async function listImages(): Promise<ImageInfo[]> {
  return apiGet<ImageInfo[]>(`${API_BASE}/images`)
}

export interface ConvertJobStatus {
  id: string
  status: 'pending' | 'running' | 'completed' | 'failed'
  progress: number
  error?: string
  output_path: string
  /** Offline boot-readiness score (0-100) from GuestKit's `doctor` analysis.
      Only set for golden-image jobs (createImageFromVm), once completed. */
  boot_score?: number
}

/** Materializes a VM's current disk as a new, independent catalog image
    (not a live CoW fork — the source VM can change or be deleted afterward
    without affecting the resulting image). Runs as an async job; poll with
    getConvertJob until status is 'completed' or 'failed'. */
export async function createImageFromVm(vmName: string, name: string): Promise<{ job_id: string }> {
  return apiPost<{ job_id: string }>(`${API_BASE}/images/from-vm/${encodeURIComponent(vmName)}`, { name })
}

export async function getConvertJob(id: string): Promise<ConvertJobStatus> {
  return apiGet<ConvertJobStatus>(`${API_BASE}/images/convert/${encodeURIComponent(id)}`)
}

export interface CloudImage {
  name: string
  distro: string
  version: string
  url: string
  format: string
  arch: string
}

export interface DownloadStatus {
  id: string
  name: string
  state: 'pending' | 'building' | 'completed' | 'failed'
  output_path?: string
  error?: string
  started: string
  completed?: string
}

export async function listCloudImages(): Promise<CloudImage[]> {
  return apiGet<CloudImage[]>(`${API_BASE}/images/cloud`)
}

export async function downloadCloudImage(name: string): Promise<DownloadStatus> {
  return apiPost<DownloadStatus>(`${API_BASE}/images/cloud/download`, { name })
}

export async function listDownloads(): Promise<DownloadStatus[]> {
  return apiGet<DownloadStatus[]>(`${API_BASE}/images/downloads`)
}
