// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiDelete, apiGet, apiPost } from './client'

export interface VmNetworkPolicy {
  default_allow: boolean
  allow_cidrs: string[]
  allow_ports: string[]
  max_egress_mbps: number | null
  max_egress_pps: number | null
  sample_rate: number
  deny_cidrs?: string[]
  allow_icmp?: boolean
  groups?: string[]
  labels?: string[]
  allow_fqdns?: string[]
  entities?: string[]
  audit_mode?: boolean
  /** Drop UDP/SCTP (except DHCP) at the host — blocks QUIC, WebRTC and STUN. */
  deny_udp?: boolean
}

export interface DataplaneStatus {
  mode: string
  required: boolean
  attached: boolean
  interface: string | null
  identity: number
  pin_dir: string | null
  schema_version: number | null
  schema_compatible: boolean
  policy_synced: boolean
  pod_ingress_required?: boolean
  pod_ingress_attached?: boolean
  policy: VmNetworkPolicy
}

export interface PodDirectionCounters {
  allowed: number
  dropped: number
  audited: number
}

export interface PodPolicyStats {
  egress: PodDirectionCounters
  ingress: PodDirectionCounters
  directional: boolean
}

export interface DataplaneStats {
  allowed_packets: number
  allowed_bytes: number
  dropped_packets: number
  dropped_bytes: number
  pod_policy?: PodPolicyStats | null
}

export interface FlowRecord {
  identity: number
  family: number
  source: string
  destination: string
  source_port: number
  destination_port: number
  protocol: number
  verdict: string
  packets: number
  bytes: number
  last_seen_ns: number
}

export interface FlowList {
  items: FlowRecord[]
}

export interface SecurityGroup {
  name: string
  labels: string[]
  policy: VmNetworkPolicy
  identity: number
  priority: number
  description: string
}

export interface DataplaneHealth {
  mode: string
  required: boolean
  default_allow: boolean
  bpf_object_present: boolean
  pin_root_present: boolean
  bpffs_present: boolean
  cilium_socket_present: boolean
  groups: number
  policies: number
  ipcache_entries: number
  ok: boolean
  notes: string[]
}

export interface IpcacheEntry {
  ip: string
  identity: number
  vm_id: string
}

export interface IdentityInfo {
  id: number
  name: string
  labels: string[]
  reserved: boolean
}

export interface CiliumEndpointView {
  id: number
  uuid: string
  identity: number
  identity_source?: string | null
  'identity-labels'?: string[]
  identity_labels?: string[]
  networking?: {
    addressing?: Array<{ ipv4?: string | null; ipv6?: string | null }>
  }
  state: string
  policy?: { ingress?: string; egress?: string; audit?: boolean }
}

export function emptyPolicy(): VmNetworkPolicy {
  return {
    default_allow: true,
    allow_cidrs: [],
    allow_ports: [],
    max_egress_mbps: null,
    max_egress_pps: null,
    sample_rate: 0,
    deny_cidrs: [],
    allow_icmp: false,
    groups: [],
    labels: [],
    allow_fqdns: [],
    entities: [],
    audit_mode: false,
    deny_udp: false,
  }
}

export function getDataplaneStatus(name: string): Promise<DataplaneStatus> {
  return apiGet(`/api/vms/${encodeURIComponent(name)}/dataplane/status`)
}

export function getDataplanePolicy(name: string): Promise<VmNetworkPolicy> {
  return apiGet(`/api/vms/${encodeURIComponent(name)}/dataplane/policy`)
}

export function setDataplanePolicy(name: string, policy: VmNetworkPolicy): Promise<VmNetworkPolicy> {
  return apiPost(`/api/vms/${encodeURIComponent(name)}/dataplane/policy`, policy)
}

export function applyDataplaneControl(
  name: string,
  req: import('../lib/policyControls').ControlRequest,
): Promise<VmNetworkPolicy> {
  return apiPost(`/api/vms/${encodeURIComponent(name)}/dataplane/policy/control`, req)
}

export function explainDataplane(
  name: string,
  dest: string,
  port = 0,
  proto = 'any',
): Promise<{ verdict: string; reason: string; would_drop: boolean; summary: string }> {
  const q = new URLSearchParams({ dest, port: String(port), proto })
  return apiGet(`/api/vms/${encodeURIComponent(name)}/dataplane/explain?${q}`)
}

export function dryRunDataplane(
  name: string,
  limit = 100,
): Promise<{
  would_drop: number
  examined: number
  hits: Array<{
    destination: string
    reason: string
    dry_verdict: string
    current_verdict: string
  }>
}> {
  return apiGet(`/api/vms/${encodeURIComponent(name)}/dataplane/dry-run?limit=${limit}`)
}

export function getDataplaneStats(name: string): Promise<DataplaneStats> {
  return apiGet(`/api/vms/${encodeURIComponent(name)}/dataplane/stats`)
}

export function getDataplaneFlows(name: string, limit = 100): Promise<FlowList> {
  return apiGet(`/api/vms/${encodeURIComponent(name)}/dataplane/flows?limit=${limit}`)
}

export interface DropReasonRecord {
  identity: number
  family: number
  source: string
  destination: string
  source_port: number
  destination_port: number
  protocol: number
  reason_code: number
  reason: string
  action: string
  packets: number
  bytes: number
  last_seen_ns: number
}

export function getDataplaneDropReasons(
  name: string,
  limit = 100,
): Promise<{ items: DropReasonRecord[] }> {
  return apiGet(
    `/api/vms/${encodeURIComponent(name)}/dataplane/drop-reasons?limit=${limit}`,
  )
}

export function getDataplaneEffective(name: string): Promise<Record<string, unknown>> {
  return apiGet(`/api/vms/${encodeURIComponent(name)}/dataplane/effective`)
}

export function listDataplaneGroups(): Promise<{ items: SecurityGroup[] }> {
  return apiGet('/api/dataplane/groups')
}

export function upsertDataplaneGroup(group: SecurityGroup): Promise<SecurityGroup> {
  return apiPost('/api/dataplane/groups', group)
}

export function deleteDataplaneGroup(name: string): Promise<void> {
  return apiDelete(`/api/dataplane/groups/${encodeURIComponent(name)}`)
}

export type NetworkServiceProtocol = 'tcp' | 'udp'
export type NetworkServiceMode = 'nat' | 'dsr'

export type NetworkServiceExposure = 'east-west' | 'north-south' | 'both'
export type NetworkBackendState = 'ready' | 'draining' | 'unhealthy'

export interface NetworkServiceBackend {
  address: string
  port: number
  weight?: number
  enabled?: boolean
  state?: NetworkBackendState
  drain_until_unix_ms?: number | null
}

export interface NetworkServiceHealthCheck {
  kind?: 'tcp'
  timeout_ms?: number
  unhealthy_threshold?: number
  healthy_threshold?: number
}

export interface NetworkServiceSpec {
  name: string
  vip: string
  port: number
  protocol: NetworkServiceProtocol
  algorithm?: 'maglev'
  mode?: NetworkServiceMode
  exposure?: NetworkServiceExposure
  backends: NetworkServiceBackend[]
  maglev_table_size?: number | null
  snat_address?: string | null
  health_check?: NetworkServiceHealthCheck | null
  advertise?: boolean
  max_egress_mbps?: number | null
  flow_sample_rate?: number
  host_routing?: boolean
}

export interface NetworkServiceStatus {
  schema_version: number
  service_id: number
  name: string
  active_backends: number
  maglev_table_size: number
  family?: string
  mode?: NetworkServiceMode
  exposure?: NetworkServiceExposure
  snat_address?: string | null
}

export interface HostServiceInterfaceStatus {
  interface: string
  tc_program_pinned: boolean
  xdp_requested: boolean
  xdp_program_pinned: boolean
  pin_dir: string
}

export interface HostServiceStatus {
  schema_version: number
  north_south_interfaces: string[]
  xdp_acceleration: boolean
  interfaces: HostServiceInterfaceStatus[]
}

export function listDataplaneServices(): Promise<{ items: NetworkServiceSpec[] }> {
  return apiGet('/api/dataplane/services')
}

export function upsertDataplaneService(
  service: NetworkServiceSpec,
): Promise<NetworkServiceStatus> {
  return apiPost('/api/dataplane/services', service)
}

export function deleteDataplaneService(name: string): Promise<void> {
  return apiDelete(`/api/dataplane/services/${encodeURIComponent(name)}`)
}

export function getDataplaneServicesStatus(): Promise<HostServiceStatus> {
  return apiGet('/api/dataplane/services/status')
}

export function getDataplaneServicesStats(): Promise<{ interfaces?: unknown }> {
  return apiGet('/api/dataplane/services/stats')
}

export function getDataplaneServicesHealth(): Promise<unknown> {
  return apiGet('/api/dataplane/services/health')
}

export function reconcileDataplaneServicesHealth(): Promise<unknown> {
  return apiPost('/api/dataplane/services/health/reconcile', {})
}

export function gcDataplaneServicesConntrack(): Promise<unknown> {
  return apiPost('/api/dataplane/services/conntrack/gc', {})
}

export function getDataplaneServicesAdvertisements(): Promise<unknown> {
  return apiGet('/api/dataplane/services/advertisements')
}

export function getDataplaneServicesFlows(limit = 256): Promise<{ items?: unknown[] }> {
  return apiGet(`/api/dataplane/services/flows?limit=${limit}`)
}

export function exportDataplaneServicesTelemetry(limit = 1024): Promise<unknown> {
  return apiPost(`/api/dataplane/services/telemetry/export?limit=${limit}`, {})
}

export function listDataplaneCnp(): Promise<{ items: unknown[] }> {
  return apiGet('/api/dataplane/cnp')
}

export function applyDataplaneCnp(doc: unknown): Promise<unknown> {
  return apiPost('/api/dataplane/cnp', doc)
}

export function deleteDataplaneCnp(name: string): Promise<void> {
  return apiDelete(`/api/dataplane/cnp/${encodeURIComponent(name)}`)
}

export function listDataplaneIdentities(): Promise<{ items: IdentityInfo[] }> {
  return apiGet('/api/dataplane/identities')
}

export function listDataplaneEndpoints(): Promise<{ items: CiliumEndpointView[] }> {
  return apiGet('/api/dataplane/endpoints')
}

export function getDataplaneObserve(): Promise<Record<string, unknown>> {
  return apiGet('/api/dataplane/observe')
}

export interface HubbleFlowList {
  items: import('../lib/packetflow').HubbleFlow[]
}

export function getDataplaneHubbleFlows(limit = 64): Promise<HubbleFlowList> {
  return apiGet(`/api/dataplane/hubble/flows?limit=${limit}`)
}

export function getDataplaneHealth(): Promise<DataplaneHealth> {
  return apiGet('/api/dataplane/health')
}

export function listDataplaneIpcache(): Promise<{ items: IpcacheEntry[] }> {
  return apiGet('/api/dataplane/ipcache')
}

export function refreshDataplaneDns(): Promise<{ refreshed: number }> {
  return apiPost('/api/dataplane/refresh-dns', {})
}
