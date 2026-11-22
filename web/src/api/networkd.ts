// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiGet, apiPost, apiPut, apiPostVoid, apiDelete } from './client'

const API_BASE = '/api'

// ─── Types ────────────────────────────────────────────────────────────────────

export type DhcpMode = 'yes' | 'no' | 'ipv4' | 'ipv6'
export type MacvtapMode = 'private' | 'vepa' | 'bridge' | 'passthru' | 'source'
export type BondMode = 'balance-rr' | 'active-backup' | 'balance-xor' | 'broadcast' | '802.3ad' | 'balance-tlb' | 'balance-alb'
export type LacpRate = 'slow' | 'fast'
export type TransmitHashPolicy = 'layer2' | 'layer3+4' | 'layer2+3' | 'encap2+3' | 'encap3+4'

export interface RouteEntry {
  destination: string
  gateway?: string
  metric?: number
  scope?: string
}

export interface BridgeConfig {
  id: string
  name: string
  stp?: boolean
  forward_delay_sec?: number
  hello_time_sec?: number
  max_age_sec?: number
  vlan_filtering?: boolean
  mtu?: number
  mac_address?: string
  addresses: string[]
  gateway?: string
  dns: string[]
  dhcp: DhcpMode
  created: string
  updated: string
  /** False for host bridges (e.g. virbr0) discovered via ip link. */
  managed?: boolean
  operational_state?: string
}

export interface CreateBridgeRequest {
  name: string
  stp?: boolean
  forward_delay_sec?: number
  hello_time_sec?: number
  max_age_sec?: number
  vlan_filtering?: boolean
  mtu?: number
  mac_address?: string
  addresses?: string[]
  gateway?: string
  dns?: string[]
  dhcp?: DhcpMode
}

export interface VlanConfig {
  id: string
  name: string
  vlan_id: number
  parent_interface: string
  mtu?: number
  addresses: string[]
  gateway?: string
  dns: string[]
  dhcp: DhcpMode
  created: string
  updated: string
  managed?: boolean
  operational_state?: string
}

export interface CreateVlanRequest {
  name: string
  vlan_id: number
  parent_interface: string
  mtu?: number
  addresses?: string[]
  gateway?: string
  dns?: string[]
  dhcp?: DhcpMode
}

export interface MacvtapConfig {
  id: string
  name: string
  parent_interface: string
  mode: MacvtapMode
  mtu?: number
  mac_address?: string
  created: string
  updated: string
  managed?: boolean
  operational_state?: string
}

export interface CreateMacvtapRequest {
  name: string
  parent_interface: string
  mode?: MacvtapMode
  mtu?: number
  mac_address?: string
}

export interface TapConfig {
  id: string
  name: string
  user?: string
  group?: string
  multi_queue?: boolean
  vnet_hdr?: boolean
  bridge?: string
  mtu?: number
  mac_address?: string
  created: string
  updated: string
  managed?: boolean
  operational_state?: string
}

export interface CreateTapRequest {
  name: string
  user?: string
  group?: string
  multi_queue?: boolean
  vnet_hdr?: boolean
  bridge?: string
  mtu?: number
  mac_address?: string
}

export interface BondConfig {
  id: string
  name: string
  mode: BondMode
  mii_monitor_sec?: number
  up_delay_sec?: number
  down_delay_sec?: number
  lacp_rate?: LacpRate
  transmit_hash_policy?: TransmitHashPolicy
  min_links?: number
  primary_slave?: string
  slave_interfaces: string[]
  mtu?: number
  mac_address?: string
  addresses: string[]
  gateway?: string
  dns: string[]
  dhcp: DhcpMode
  routes: RouteEntry[]
  created: string
  updated: string
  managed?: boolean
  operational_state?: string
}

export interface CreateBondRequest {
  name: string
  mode?: BondMode
  mii_monitor_sec?: number
  up_delay_sec?: number
  down_delay_sec?: number
  lacp_rate?: LacpRate
  transmit_hash_policy?: TransmitHashPolicy
  min_links?: number
  primary_slave?: string
  slave_interfaces?: string[]
  mtu?: number
  mac_address?: string
  addresses?: string[]
  gateway?: string
  dns?: string[]
  dhcp?: DhcpMode
  routes?: RouteEntry[]
}

export interface NetworkFileConfig {
  id: string
  match_name: string
  match_mac?: string
  addresses: string[]
  gateway?: string
  dns: string[]
  dhcp: DhcpMode
  bridge?: string
  bond?: string
  mtu?: number
  routes: RouteEntry[]
  description?: string
  created: string
  updated: string
  managed?: boolean
  operational_state?: string
}

export interface CreateNetworkFileRequest {
  match_name: string
  match_mac?: string
  addresses?: string[]
  gateway?: string
  dns?: string[]
  dhcp?: DhcpMode
  bridge?: string
  bond?: string
  mtu?: number
  routes?: RouteEntry[]
  description?: string
}

export interface LinkFileConfig {
  id: string
  match_mac?: string
  match_path?: string
  match_driver?: string
  match_original_name?: string
  name?: string
  mtu?: number
  mac_address?: string
  wake_on_lan?: string
  description?: string
  created: string
  updated: string
  managed?: boolean
  source_file?: string
}

export interface CreateLinkFileRequest {
  match_mac?: string
  match_path?: string
  match_driver?: string
  match_original_name?: string
  name?: string
  mtu?: number
  mac_address?: string
  wake_on_lan?: string
  description?: string
}

export interface ParsedConfigFile {
  filename: string
  file_type: string
  sections: { name: string; entries: [string, string][] }[]
}

export interface LinkInfo {
  index: number
  name: string
  kind: string
  operational_state: string
  setup_state: string
}

// ─── Bridges ──────────────────────────────────────────────────────────────────

export async function listBridges(): Promise<BridgeConfig[]> {
  return apiGet<BridgeConfig[]>(`${API_BASE}/networkd/bridges`)
}

export async function createBridge(req: CreateBridgeRequest): Promise<BridgeConfig> {
  return apiPost<BridgeConfig>(`${API_BASE}/networkd/bridges`, req)
}

export async function getBridge(id: string): Promise<BridgeConfig> {
  return apiGet<BridgeConfig>(`${API_BASE}/networkd/bridges/${id}`)
}

export async function updateBridge(id: string, req: CreateBridgeRequest): Promise<BridgeConfig> {
  return apiPut<BridgeConfig>(`${API_BASE}/networkd/bridges/${id}`, req)
}

export async function deleteBridge(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/networkd/bridges/${id}`)
}

export async function adoptBridge(hostId: string): Promise<BridgeConfig> {
  return apiPost<BridgeConfig>(`${API_BASE}/networkd/bridges/adopt`, { host_id: hostId })
}

// ─── VLANs ────────────────────────────────────────────────────────────────────

export async function listVlans(): Promise<VlanConfig[]> {
  return apiGet<VlanConfig[]>(`${API_BASE}/networkd/vlans`)
}

export async function createVlan(req: CreateVlanRequest): Promise<VlanConfig> {
  return apiPost<VlanConfig>(`${API_BASE}/networkd/vlans`, req)
}

export async function getVlan(id: string): Promise<VlanConfig> {
  return apiGet<VlanConfig>(`${API_BASE}/networkd/vlans/${id}`)
}

export async function updateVlan(id: string, req: CreateVlanRequest): Promise<VlanConfig> {
  return apiPut<VlanConfig>(`${API_BASE}/networkd/vlans/${id}`, req)
}

export async function deleteVlan(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/networkd/vlans/${id}`)
}

export async function adoptVlan(hostId: string): Promise<VlanConfig> {
  return apiPost<VlanConfig>(`${API_BASE}/networkd/vlans/adopt`, { host_id: hostId })
}

// ─── Macvtap ──────────────────────────────────────────────────────────────────

export async function listMacvtaps(): Promise<MacvtapConfig[]> {
  return apiGet<MacvtapConfig[]>(`${API_BASE}/networkd/macvtaps`)
}

export async function createMacvtap(req: CreateMacvtapRequest): Promise<MacvtapConfig> {
  return apiPost<MacvtapConfig>(`${API_BASE}/networkd/macvtaps`, req)
}

export async function getMacvtap(id: string): Promise<MacvtapConfig> {
  return apiGet<MacvtapConfig>(`${API_BASE}/networkd/macvtaps/${id}`)
}

export async function deleteMacvtap(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/networkd/macvtaps/${id}`)
}

export async function adoptMacvtap(hostId: string): Promise<MacvtapConfig> {
  return apiPost<MacvtapConfig>(`${API_BASE}/networkd/macvtaps/adopt`, { host_id: hostId })
}

// ─── Tap ──────────────────────────────────────────────────────────────────────

export async function listTaps(): Promise<TapConfig[]> {
  return apiGet<TapConfig[]>(`${API_BASE}/networkd/taps`)
}

export async function createTap(req: CreateTapRequest): Promise<TapConfig> {
  return apiPost<TapConfig>(`${API_BASE}/networkd/taps`, req)
}

export async function getTap(id: string): Promise<TapConfig> {
  return apiGet<TapConfig>(`${API_BASE}/networkd/taps/${id}`)
}

export async function deleteTap(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/networkd/taps/${id}`)
}

export async function adoptTap(hostId: string): Promise<TapConfig> {
  return apiPost<TapConfig>(`${API_BASE}/networkd/taps/adopt`, { host_id: hostId })
}

// ─── Bonds ────────────────────────────────────────────────────────────────────

export async function listBonds(): Promise<BondConfig[]> {
  return apiGet<BondConfig[]>(`${API_BASE}/networkd/bonds`)
}

export async function createBond(req: CreateBondRequest): Promise<BondConfig> {
  return apiPost<BondConfig>(`${API_BASE}/networkd/bonds`, req)
}

export async function getBond(id: string): Promise<BondConfig> {
  return apiGet<BondConfig>(`${API_BASE}/networkd/bonds/${id}`)
}

export async function updateBond(id: string, req: CreateBondRequest): Promise<BondConfig> {
  return apiPut<BondConfig>(`${API_BASE}/networkd/bonds/${id}`, req)
}

export async function deleteBond(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/networkd/bonds/${id}`)
}

export async function adoptBond(hostId: string): Promise<BondConfig> {
  return apiPost<BondConfig>(`${API_BASE}/networkd/bonds/adopt`, { host_id: hostId })
}

// ─── Network Files ────────────────────────────────────────────────────────────

export async function listNetworkFiles(): Promise<NetworkFileConfig[]> {
  return apiGet<NetworkFileConfig[]>(`${API_BASE}/networkd/network-files`)
}

export async function createNetworkFile(req: CreateNetworkFileRequest): Promise<NetworkFileConfig> {
  return apiPost<NetworkFileConfig>(`${API_BASE}/networkd/network-files`, req)
}

export async function getNetworkFile(id: string): Promise<NetworkFileConfig> {
  return apiGet<NetworkFileConfig>(`${API_BASE}/networkd/network-files/${id}`)
}

export async function deleteNetworkFile(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/networkd/network-files/${id}`)
}

export async function adoptNetworkFile(hostId: string): Promise<NetworkFileConfig> {
  return apiPost<NetworkFileConfig>(`${API_BASE}/networkd/network-files/adopt`, { host_id: hostId })
}

// ─── Link Files ───────────────────────────────────────────────────────────────

export async function listLinkFiles(): Promise<LinkFileConfig[]> {
  return apiGet<LinkFileConfig[]>(`${API_BASE}/networkd/link-files`)
}

export async function createLinkFile(req: CreateLinkFileRequest): Promise<LinkFileConfig> {
  return apiPost<LinkFileConfig>(`${API_BASE}/networkd/link-files`, req)
}

export async function deleteLinkFile(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/networkd/link-files/${id}`)
}

// ─── VXLAN ────────────────────────────────────────────────────────────────────

export interface VxlanConfig {
  id: string
  name: string
  vni: number
  remote?: string
  local?: string
  port?: number
  parent_interface?: string
  mtu?: number
  addresses: string[]
  gateway?: string
  dns: string[]
  dhcp: DhcpMode
  created: string
  updated: string
  managed?: boolean
}

export interface CreateVxlanRequest {
  name: string
  vni: number
  remote?: string
  local?: string
  port?: number
  parent_interface?: string
  mtu?: number
  addresses?: string[]
  gateway?: string
  dns?: string[]
  dhcp?: DhcpMode
}

export async function listVxlans(): Promise<VxlanConfig[]> {
  return apiGet<VxlanConfig[]>(`${API_BASE}/networkd/vxlans`)
}

export async function createVxlan(req: CreateVxlanRequest): Promise<VxlanConfig> {
  return apiPost<VxlanConfig>(`${API_BASE}/networkd/vxlans`, req)
}

export async function deleteVxlan(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/networkd/vxlans/${id}`)
}

export async function adoptVxlan(hostId: string): Promise<VxlanConfig> {
  return apiPost<VxlanConfig>(`${API_BASE}/networkd/vxlans/adopt`, { host_id: hostId })
}

// ─── SR-IOV ───────────────────────────────────────────────────────────────────

export interface VfConfig {
  vf_index: number
  mac_address?: string
  vlan?: number
  qos?: number
  spoofchk?: boolean
  trust?: boolean
}

export interface SriovConfig {
  id: string
  pf_name: string
  num_vfs: number
  vf_configs: VfConfig[]
  created: string
  updated: string
  managed?: boolean
}

export interface CreateSriovRequest {
  pf_name: string
  num_vfs: number
  vf_configs?: VfConfig[]
}

export async function listSriov(): Promise<SriovConfig[]> {
  return apiGet<SriovConfig[]>(`${API_BASE}/networkd/sriov`)
}

export async function createSriov(req: CreateSriovRequest): Promise<SriovConfig> {
  return apiPost<SriovConfig>(`${API_BASE}/networkd/sriov`, req)
}

export async function deleteSriov(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/networkd/sriov/${id}`)
}

export async function adoptSriov(hostId: string): Promise<SriovConfig> {
  return apiPost<SriovConfig>(`${API_BASE}/networkd/sriov/adopt`, { host_id: hostId })
}

// ─── Port Forwards ───────────────────────────────────────────────────────────

export type Protocol = 'tcp' | 'udp' | 'both'

export interface PortForwardConfig {
  id: string
  name: string
  protocol: Protocol
  host_port: number
  guest_ip: string
  guest_port: number
  interface?: string
  enabled: boolean
  description?: string
  created: string
  updated: string
  managed?: boolean
}

export interface CreatePortForwardRequest {
  name: string
  protocol?: Protocol
  host_port: number
  guest_ip: string
  guest_port: number
  interface?: string
  enabled?: boolean
  description?: string
}

export async function listPortForwards(): Promise<PortForwardConfig[]> {
  return apiGet<PortForwardConfig[]>(`${API_BASE}/networkd/port-forwards`)
}

export async function createPortForward(req: CreatePortForwardRequest): Promise<PortForwardConfig> {
  return apiPost<PortForwardConfig>(`${API_BASE}/networkd/port-forwards`, req)
}

export async function getPortForward(id: string): Promise<PortForwardConfig> {
  return apiGet<PortForwardConfig>(`${API_BASE}/networkd/port-forwards/${id}`)
}

export async function deletePortForward(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/networkd/port-forwards/${id}`)
}

export async function syncPortForwards(): Promise<{ status: string; rules: number }> {
  return apiPost<{ status: string; rules: number }>(`${API_BASE}/networkd/port-forwards/sync`)
}

export async function adoptPortForward(hostId: string): Promise<PortForwardConfig> {
  return apiPost<PortForwardConfig>(`${API_BASE}/networkd/port-forwards/adopt`, { host_id: hostId })
}

// ─── Scan existing configs ────────────────────────────────────────────────────

export async function scanConfigs(): Promise<ParsedConfigFile[]> {
  return apiGet<ParsedConfigFile[]>(`${API_BASE}/networkd/scan`)
}

// ─── Status & Control ─────────────────────────────────────────────────────────

export async function listLinks(): Promise<LinkInfo[]> {
  return apiGet<LinkInfo[]>(`${API_BASE}/networkd/links`)
}

export async function getDeviceStatus(name: string): Promise<{ name: string; status: string }> {
  return apiGet<{ name: string; status: string }>(`${API_BASE}/networkd/links/${name}/status`)
}

export async function reloadNetworkd(): Promise<void> {
  return apiPostVoid(`${API_BASE}/networkd/reload`)
}

export async function listManagedFiles(): Promise<string[]> {
  return apiGet<string[]>(`${API_BASE}/networkd/files`)
}
