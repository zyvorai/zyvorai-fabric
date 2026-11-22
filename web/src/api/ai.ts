// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiDelete, apiGet, apiPost } from './client'

export interface ModelArtifact {
  name: string
  source: string
  revision?: string
  checksum?: string
  format: string
  size_bytes?: number
  tenant?: string
  local_path?: string
  residency?: string
  license?: string
  created: string
  updated: string
}

export interface InferenceProfile {
  name: string
  runtime: string
  gpu: { vendor: string; count: number; minimum_vram_gib: number }
  cpu: number
  memory_gib: number
  allowed_hosts?: string[]
  max_egress_mbps?: number
  tenant?: string
  created: string
  updated: string
}

export interface InferenceReplica {
  replica_id?: string
  ordinal?: number
  vm_name: string
  bdf: string
  ready: boolean
  address?: string
  maglev_weight?: number
  site?: string
  draining?: boolean
}

export interface AutoscalingPolicy {
  enabled?: boolean
  min_replicas?: number
  max_replicas?: number
  scale_to_zero?: boolean
}

export interface InferenceDeployment {
  name: string
  model: string
  profile: string
  replicas: number
  gpus_per_replica?: number
  tenant?: string
  autoscaling?: AutoscalingPolicy
  status: { phase: string; replicas: InferenceReplica[]; message?: string }
  created: string
  updated: string
}

export interface InferenceEndpoint {
  name: string
  deployment: string
  protocol: string
  port: number
  service_id?: number
  vip?: string
  routing_strategy?: string
  preferred_site?: string
  tenant?: string
  created: string
  updated: string
}

export interface InferenceApiKey {
  id: string
  name: string
  endpoint: string
  model?: string
  prefix: string
  request_quota?: number
  requests_used: number
  created: string
}

export interface FabricGpuView {
  bdf: string
  vendor: string
  vendor_id: number
  device_id: number
  driver?: string
  iommu_group?: number
  group_bound_to_vfio: boolean
  group_held: boolean
  numa_node?: number
  vram_gib?: number
  allocated_to?: { deployment: string; tenant?: string; vm_name: string }
}

export interface NodeGpu {
  bdf: string
  vendor?: string
  vram_gib?: number
  model?: string
  healthy?: boolean
  mig_profile?: string
  parent_bdf?: string
  temperature_c?: number
  power_watts?: number
  ecc_errors?: number
}

export interface InferenceNode {
  id: string
  site?: string
  failure_domain?: string
  state?: string
  heartbeat_unix?: number
  gpus?: NodeGpu[]
  taints?: string[]
  cached_models?: string[]
}

export const listModels = () => apiGet<ModelArtifact[]>('/api/ai/models')
export const listProfiles = () => apiGet<InferenceProfile[]>('/api/ai/profiles')
export const listDeployments = () => apiGet<InferenceDeployment[]>('/api/ai/deployments')
export const listEndpoints = () => apiGet<InferenceEndpoint[]>('/api/ai/endpoints')
export const listKeys = () => apiGet<InferenceApiKey[]>('/api/ai/keys')
export const listGpus = () => apiGet<{ items: FabricGpuView[] } | FabricGpuView[]>('/api/ai/gpus')
export const listNodes = () => apiGet<InferenceNode[] | { items?: InferenceNode[] }>('/api/ai/nodes')
export const listCapacity = () => apiGet<Record<string, unknown>>('/api/ai/capacity')

export const createModel = (body: Partial<ModelArtifact>) =>
  apiPost<ModelArtifact>('/api/ai/models', body)
export const createProfile = (body: Partial<InferenceProfile>) =>
  apiPost<InferenceProfile>('/api/ai/profiles', body)
export const createDeployment = (body: {
  name: string
  model: string
  profile: string
  replicas?: number
}) => apiPost<InferenceDeployment>('/api/ai/deployments', body)
export const scaleDeployment = (name: string, replicas: number) =>
  apiPost<InferenceDeployment>(`/api/ai/deployments/${encodeURIComponent(name)}/scale`, {
    replicas,
  })
export const deleteDeployment = (name: string) =>
  apiDelete(`/api/ai/deployments/${encodeURIComponent(name)}`)
export const createEndpoint = (body: Partial<InferenceEndpoint>) =>
  apiPost<InferenceEndpoint>('/api/ai/endpoints', body)
