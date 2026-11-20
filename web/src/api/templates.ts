// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiPut, apiPostVoid, apiDelete } from './client'

const API_BASE = '/api'

export interface VMTemplate {
  id: string
  name: string
  description?: string
  cpus: number
  memory: number
  disk: number
  image: string
  cloud_init?: unknown
  tags: string[]
  created: string
  updated: string
}

export interface CreateTemplateRequest {
  name: string
  description?: string
  cpus: number
  memory: number
  disk: number
  image: string
  cloud_init?: unknown
  tags?: string[]
  from_vm?: string
}

export interface DeployTemplateRequest {
  vm_name: string
}

export async function listTemplates(): Promise<VMTemplate[]> {
  return apiGet<VMTemplate[]>(`${API_BASE}/templates`)
}

export async function getTemplate(id: string): Promise<VMTemplate> {
  return apiGet<VMTemplate>(`${API_BASE}/templates/${id}`)
}

export async function createTemplate(req: CreateTemplateRequest): Promise<VMTemplate> {
  return apiPost<VMTemplate>(`${API_BASE}/templates`, req)
}

export async function updateTemplate(id: string, req: Partial<CreateTemplateRequest>): Promise<VMTemplate> {
  return apiPut<VMTemplate>(`${API_BASE}/templates/${id}`, req)
}

export async function deleteTemplate(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/templates/${id}`)
}

export async function deployTemplate(id: string, vmName: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/templates/${id}/deploy`, { vm_name: vmName })
}
