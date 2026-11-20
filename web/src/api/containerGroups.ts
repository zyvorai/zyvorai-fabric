// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiDelete } from './client'

export interface EnvVarSpec {
  name: string
  value: string
}

export interface VolumeMount {
  host: string
  guest: string
  type?: string
  readonly?: boolean
}

export type ProbeCheckSpec =
  | { type: 'http'; path: string; port: number }
  | { type: 'tcp'; port: number }
  | { type: 'exec'; command: string[] }

export type ProbeSpec = ProbeCheckSpec & {
  initial_delay_secs?: number
  period_secs?: number
  timeout_secs?: number
  success_threshold?: number
  failure_threshold?: number
}

export interface ResourceSpec {
  cpus: number
  memory: string // e.g. "512M", "2G"
  disk?: string
}

export interface ContainerSpec {
  name: string
  image: string
  command?: string[]
  args?: string[]
  env?: EnvVarSpec[]
  resources: ResourceSpec
  volume_mounts?: VolumeMount[]
  liveness_probe?: ProbeSpec
  readiness_probe?: ProbeSpec
}

export interface PlacementSpec {
  node_hint?: string
  auto_reschedule?: boolean
}

export interface NetworkPolicyRuleSpec {
  from_container_groups?: string[]
  from_cidrs?: string[]
  ports?: number[]
}

export interface NetworkPolicySpec {
  ingress?: NetworkPolicyRuleSpec[]
  egress?: NetworkPolicyRuleSpec[]
}

export interface ContainerGroupSpec {
  name: string
  containers: ContainerSpec[]
  replicas?: number
  placement?: PlacementSpec
  restart_policy?: string
  tags?: string[]
  tenant?: string
  image_pull_secrets?: string[]
  network_policy?: NetworkPolicySpec
}

export interface ContainerGroupApplyResult {
  name: string
  host_id: string
  host_name: string
  replicas_created: number
  warnings: string[]
}

export type ContainerGroupEventType =
  | 'created'
  | 'applied'
  | 'deleted'
  | 'placement_failed'
  | 'quota_exceeded'

export interface ContainerGroupEvent {
  id: string
  event_type: ContainerGroupEventType
  container_group_name: string
  tenant?: string
  actor: string
  detail?: string
  timestamp: string
}

export interface ContainerGroupBackup {
  id: string
  container_group_name: string
  tenant?: string
  volume_paths: string[]
  size_bytes: number
  status: 'completed' | 'failed'
  error?: string
  archive_path: string
  created: string
  retention_days: number
  expires_at: string
}

export interface RestoreResult {
  container_group_name: string
  restored_paths: string[]
  warnings: string[]
}

const API_BASE = '/api'

export async function listContainerGroups(): Promise<ContainerGroupSpec[]> {
  return apiGet<ContainerGroupSpec[]>(`${API_BASE}/container-groups`)
}

export async function getContainerGroup(name: string): Promise<ContainerGroupSpec> {
  return apiGet<ContainerGroupSpec>(`${API_BASE}/container-groups/${encodeURIComponent(name)}/spec`)
}

export async function applyContainerGroup(spec: ContainerGroupSpec): Promise<ContainerGroupApplyResult> {
  return apiPost<ContainerGroupApplyResult>(`${API_BASE}/container-groups/apply`, spec)
}

export async function deleteContainerGroup(name: string): Promise<void> {
  return apiDelete(`${API_BASE}/container-groups/${encodeURIComponent(name)}`)
}

export interface ContainerGroupLivePod {
  name: string
  phase: string | null
  node_name: string | null
  pod_ip: string | null
}

export interface ContainerGroupLiveStatus {
  name: string
  host_id: string | null
  host_name: string | null
  updated_at: string | null
  pods: ContainerGroupLivePod[]
}

export async function getContainerGroupStatus(name: string): Promise<ContainerGroupLiveStatus> {
  return apiGet<ContainerGroupLiveStatus>(
    `${API_BASE}/container-groups/${encodeURIComponent(name)}/status`,
  )
}

export async function listContainerGroupEvents(): Promise<ContainerGroupEvent[]> {
  return apiGet<ContainerGroupEvent[]>(`${API_BASE}/container-group-events`)
}

export async function listContainerGroupBackups(): Promise<ContainerGroupBackup[]> {
  return apiGet<ContainerGroupBackup[]>(`${API_BASE}/container-group-backups`)
}

export async function createContainerGroupBackup(
  containerGroupName: string,
  retentionDays = 30,
): Promise<ContainerGroupBackup> {
  return apiPost<ContainerGroupBackup>(`${API_BASE}/container-group-backups`, {
    container_group_name: containerGroupName,
    retention_days: retentionDays,
  })
}

export async function deleteContainerGroupBackup(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/container-group-backups/${encodeURIComponent(id)}`)
}

export async function restoreContainerGroupBackup(id: string): Promise<RestoreResult> {
  return apiPost<RestoreResult>(`${API_BASE}/container-group-backups/${encodeURIComponent(id)}/restore`)
}
