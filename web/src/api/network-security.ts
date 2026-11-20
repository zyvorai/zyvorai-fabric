// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiPut, apiPostVoid, apiDelete } from './client'

const API_BASE = '/api'

// ─── Network Policies ────────────────────────────────────────────────────────

export type PolicyDirection = 'ingress' | 'egress'
/** UI-only convenience shape for one rule being authored in the Add Rule
 * form -- converted to the backend's PolicyIngressRule/PolicyEgressRule
 * shape (from/to peer selectors + to_ports) on submit. The backend has no
 * per-rule action: every ingress/egress entry is an allow, and anything
 * not explicitly allowed is denied by default once a policy selects a VM. */
export interface PolicyRule {
  direction: PolicyDirection
  protocol?: string
  port?: number
  cidr?: string
}

export interface PolicyPortRule {
  protocol?: 'tcp' | 'udp' | 'any'
  port: number
  end_port?: number
}

export type PolicyPeerSelector =
  | { endpoint: { match_labels?: Record<string, string> } }
  | { cidr: string }

export interface PolicyIngressRule {
  from?: PolicyPeerSelector[]
  to_ports?: PolicyPortRule[]
}

export interface PolicyEgressRule {
  to?: PolicyPeerSelector[]
  to_ports?: PolicyPortRule[]
}

export interface NetworkPolicy {
  id: string
  name: string
  description?: string
  endpoint_selector: { match_labels?: Record<string, string> }
  ingress: PolicyIngressRule[]
  egress: PolicyEgressRule[]
  enabled: boolean
  managed?: boolean
  matched_vms?: number
  created: string
  updated: string
}

export interface CreateNetworkPolicyRequest {
  name: string
  description?: string
  endpoint_selector: { match_labels: Record<string, string> }
  ingress?: PolicyIngressRule[]
  egress?: PolicyEgressRule[]
  enabled?: boolean
}

export async function listNetworkPolicies(): Promise<NetworkPolicy[]> {
  return apiGet<NetworkPolicy[]>(`${API_BASE}/network-policies`)
}

export async function createNetworkPolicy(req: CreateNetworkPolicyRequest): Promise<NetworkPolicy> {
  return apiPost<NetworkPolicy>(`${API_BASE}/network-policies`, req)
}

export async function getNetworkPolicy(id: string): Promise<NetworkPolicy> {
  return apiGet<NetworkPolicy>(`${API_BASE}/network-policies/${id}`)
}

export async function updateNetworkPolicy(id: string, req: CreateNetworkPolicyRequest): Promise<NetworkPolicy> {
  return apiPut<NetworkPolicy>(`${API_BASE}/network-policies/${id}`, req)
}

export async function deleteNetworkPolicy(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/network-policies/${id}`)
}

export async function adoptNetworkPolicy(hostId: string): Promise<NetworkPolicy> {
  return apiPost<NetworkPolicy>(`${API_BASE}/network-policies/adopt`, { host_id: hostId })
}

export async function syncNetworkPolicies(): Promise<{ status: string; policies: number }> {
  return apiPost<{ status: string; policies: number }>(`${API_BASE}/network-policies/sync`)
}

export async function networkPolicyStatus(): Promise<{ enforced: number; pending: number }> {
  return apiGet<{ enforced: number; pending: number }>(`${API_BASE}/network-policies/status`)
}

export interface SecurityIdentity {
  id: number
  labels: Record<string, string>
  endpoints: string[]
  description?: string
  managed?: boolean
  created: string
  updated: string
}

export async function listIdentities(): Promise<SecurityIdentity[]> {
  return apiGet<SecurityIdentity[]>(`${API_BASE}/identities`)
}

export async function getIdentity(id: number): Promise<SecurityIdentity> {
  return apiGet<SecurityIdentity>(`${API_BASE}/identities/${id}`)
}

export async function adoptIdentity(hostId: string): Promise<SecurityIdentity> {
  return apiPost<SecurityIdentity>(`${API_BASE}/identities/adopt`, { host_id: hostId })
}

// ─── Firewall ────────────────────────────────────────────────────────────────

export type FirewallAction = 'accept' | 'drop' | 'reject' | 'log'

export interface FirewallRule {
  priority: number
  protocol?: string
  dest_port?: number
  source_cidr?: string
  action: FirewallAction
}

export interface FirewallProfile {
  id: string
  name: string
  description?: string
  default_action: FirewallAction
  rules: FirewallRule[]
  managed?: boolean
  created: string
  updated: string
}

export interface CreateFirewallProfileRequest {
  name: string
  description?: string
  default_action?: FirewallAction
  rules?: FirewallRule[]
  enabled?: boolean
}

export interface FirewallZone {
  id: string
  name: string
  description?: string
  default_profile_id?: string
  managed?: boolean
  created: string
  updated: string
}

export interface CreateFirewallZoneRequest {
  name: string
  description?: string
  default_profile_id?: string
}

export interface VMFirewallAssignment {
  vm_name: string
  profile_id: string
  zone_id?: string
}

export interface CreateVMFirewallAssignmentRequest {
  vm_name: string
  profile_id: string
  zone_id?: string
}

export async function listFirewallProfiles(): Promise<FirewallProfile[]> {
  return apiGet<FirewallProfile[]>(`${API_BASE}/firewall-profiles`)
}

export async function createFirewallProfile(req: CreateFirewallProfileRequest): Promise<FirewallProfile> {
  return apiPost<FirewallProfile>(`${API_BASE}/firewall-profiles`, req)
}

export async function getFirewallProfile(id: string): Promise<FirewallProfile> {
  return apiGet<FirewallProfile>(`${API_BASE}/firewall-profiles/${id}`)
}

export async function updateFirewallProfile(id: string, req: CreateFirewallProfileRequest): Promise<FirewallProfile> {
  return apiPut<FirewallProfile>(`${API_BASE}/firewall-profiles/${id}`, req)
}

export async function deleteFirewallProfile(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/firewall-profiles/${id}`)
}

export async function adoptFirewallProfile(hostId: string): Promise<FirewallProfile> {
  return apiPost<FirewallProfile>(`${API_BASE}/firewall-profiles/adopt`, { host_id: hostId })
}

export async function listFirewallZones(): Promise<FirewallZone[]> {
  return apiGet<FirewallZone[]>(`${API_BASE}/firewall-zones`)
}

export async function createFirewallZone(req: CreateFirewallZoneRequest): Promise<FirewallZone> {
  return apiPost<FirewallZone>(`${API_BASE}/firewall-zones`, req)
}

export async function deleteFirewallZone(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/firewall-zones/${id}`)
}

export async function adoptFirewallZone(hostId: string): Promise<FirewallZone> {
  return apiPost<FirewallZone>(`${API_BASE}/firewall-zones/adopt`, { host_id: hostId })
}

export async function listVMFirewallAssignments(): Promise<VMFirewallAssignment[]> {
  return apiGet<VMFirewallAssignment[]>(`${API_BASE}/firewall-assignments`)
}

export async function createVMFirewallAssignment(req: CreateVMFirewallAssignmentRequest): Promise<VMFirewallAssignment> {
  return apiPut<VMFirewallAssignment>(`${API_BASE}/vms/${req.vm_name}/firewall`, {
    profile_id: req.profile_id,
    zone_id: req.zone_id,
  })
}

export async function deleteVMFirewallAssignment(vmName: string): Promise<void> {
  return apiDelete(`${API_BASE}/vms/${vmName}/firewall`)
}

export async function syncFirewall(): Promise<{ status: string; profiles: number }> {
  return apiPost<{ status: string; profiles: number }>(`${API_BASE}/firewall/sync`)
}

export async function firewallStatus(): Promise<{ active_profiles: number; assigned_vms: number }> {
  return apiGet<{ active_profiles: number; assigned_vms: number }>(`${API_BASE}/firewall/status`)
}

// ─── Service Mesh ────────────────────────────────────────────────────────────

export type LoadBalancerAlgorithm = 'round_robin' | 'random' | 'ip_hash'

export interface ServiceBackend {
  address: string
  port: number
  weight?: number
}

export interface ServicePort {
  port: number
  target_port?: number
  protocol?: 'tcp' | 'udp'
}

export interface Service {
  id: string
  name: string
  description?: string
  virtual_ip: string
  ports: ServicePort[]
  algorithm: LoadBalancerAlgorithm
  selector?: { match_labels?: Record<string, string> }
  enabled: boolean
  managed?: boolean
  created: string
  updated: string
}

export interface CreateServiceRequest {
  name: string
  description?: string
  virtual_ip: string
  ports: ServicePort[]
  algorithm?: LoadBalancerAlgorithm
  selector?: { match_labels?: Record<string, string> }
  enabled?: boolean
}

export async function listServices(): Promise<Service[]> {
  return apiGet<Service[]>(`${API_BASE}/services`)
}

export async function createService(req: CreateServiceRequest): Promise<Service> {
  return apiPost<Service>(`${API_BASE}/services`, req)
}

export async function getService(id: string): Promise<Service> {
  return apiGet<Service>(`${API_BASE}/services/${id}`)
}

export async function updateService(id: string, req: CreateServiceRequest): Promise<Service> {
  return apiPut<Service>(`${API_BASE}/services/${id}`, req)
}

export async function deleteService(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/services/${id}`)
}

export async function adoptService(hostId: string): Promise<Service> {
  return apiPost<Service>(`${API_BASE}/services/adopt`, { host_id: hostId })
}

export async function syncServices(): Promise<{ status: string; services: number }> {
  return apiPost<{ status: string; services: number }>(`${API_BASE}/services/sync`)
}

export async function serviceMeshStatus(): Promise<{ active_services: number; total_backends: number }> {
  return apiGet<{ active_services: number; total_backends: number }>(`${API_BASE}/services/status`)
}

// ─── QoS / Traffic Shaping ───────────────────────────────────────────────────

export interface BandwidthRate {
  value: number
  unit: 'kbit' | 'mbit' | 'gbit'
}

export interface TrafficClass {
  name: string
  guaranteed_rate: BandwidthRate
  max_rate: BandwidthRate
  burst?: string
  priority: number
}

export interface QoSPolicy {
  id: string
  name: string
  description?: string
  interface: string
  selector?: { match_labels?: Record<string, string> }
  traffic_class: TrafficClass
  enabled: boolean
  managed?: boolean
  created: string
  updated: string
}

export interface CreateQoSPolicyRequest {
  name: string
  description?: string
  interface: string
  selector?: { match_labels?: Record<string, string> }
  traffic_class: TrafficClass
  enabled?: boolean
}

export async function listQosPolicies(): Promise<QoSPolicy[]> {
  return apiGet<QoSPolicy[]>(`${API_BASE}/qos-policies`)
}

export async function createQosPolicy(req: CreateQoSPolicyRequest): Promise<QoSPolicy> {
  return apiPost<QoSPolicy>(`${API_BASE}/qos-policies`, req)
}

export async function getQosPolicy(id: string): Promise<QoSPolicy> {
  return apiGet<QoSPolicy>(`${API_BASE}/qos-policies/${id}`)
}

export async function updateQosPolicy(id: string, req: CreateQoSPolicyRequest): Promise<QoSPolicy> {
  return apiPut<QoSPolicy>(`${API_BASE}/qos-policies/${id}`, req)
}

export async function deleteQosPolicy(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/qos-policies/${id}`)
}

export async function adoptQosPolicy(hostId: string): Promise<QoSPolicy> {
  return apiPost<QoSPolicy>(`${API_BASE}/qos-policies/adopt`, { host_id: hostId })
}

export async function syncQos(): Promise<{ status: string; policies: number }> {
  return apiPost<{ status: string; policies: number }>(`${API_BASE}/qos-policies/sync`)
}

export async function qosStatus(): Promise<{ active_policies: number; shaped_vms: number }> {
  return apiGet<{ active_policies: number; shaped_vms: number }>(`${API_BASE}/qos-policies/status`)
}

// ─── DNS ─────────────────────────────────────────────────────────────────────

export type DnsRecordType = 'A' | 'AAAA' | 'CNAME' | 'MX' | 'TXT' | 'SRV' | 'PTR'

export interface DnsRecord {
  name: string
  record_type: DnsRecordType
  value: string
  ttl: number
}

export interface DnsZone {
  id: string
  name: string
  description?: string
  domain?: string
  records?: DnsRecord[]
  enabled?: boolean
  managed?: boolean
  created: string
  updated: string
}

export interface CreateDnsZoneRequest {
  name: string
  description?: string
  domain: string
  records?: DnsRecord[]
  enabled?: boolean
}

export interface DnsPolicy {
  id: string
  name: string
  description?: string
  zone_id?: string
  selector?: { match_labels?: Record<string, string> }
  record_template?: string
  record_type?: string
  labels?: Record<string, string>
  upstream_servers?: string[]
  blocked_domains?: string[]
  enabled: boolean
  managed?: boolean
  created: string
  updated: string
}

export interface CreateDnsPolicyRequest {
  name: string
  zone_id: string
  selector?: { match_labels?: Record<string, string> }
  record_template: string
  record_type?: DnsRecordType
  enabled?: boolean
}

export async function listDnsZones(): Promise<DnsZone[]> {
  return apiGet<DnsZone[]>(`${API_BASE}/dns-zones`)
}

export async function createDnsZone(req: CreateDnsZoneRequest): Promise<DnsZone> {
  return apiPost<DnsZone>(`${API_BASE}/dns-zones`, req)
}

export async function getDnsZone(id: string): Promise<DnsZone> {
  return apiGet<DnsZone>(`${API_BASE}/dns-zones/${id}`)
}

export async function updateDnsZone(id: string, req: CreateDnsZoneRequest): Promise<DnsZone> {
  return apiPut<DnsZone>(`${API_BASE}/dns-zones/${id}`, req)
}

export async function deleteDnsZone(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/dns-zones/${id}`)
}

export async function adoptDnsZone(hostId: string): Promise<DnsZone> {
  return apiPost<DnsZone>(`${API_BASE}/dns-zones/adopt`, { host_id: hostId })
}

export async function listDnsPolicies(): Promise<DnsPolicy[]> {
  return apiGet<DnsPolicy[]>(`${API_BASE}/dns-policies`)
}

export async function createDnsPolicy(req: CreateDnsPolicyRequest): Promise<DnsPolicy> {
  return apiPost<DnsPolicy>(`${API_BASE}/dns-policies`, req)
}

export async function getDnsPolicy(id: string): Promise<DnsPolicy> {
  return apiGet<DnsPolicy>(`${API_BASE}/dns-policies/${id}`)
}

export async function updateDnsPolicy(id: string, req: CreateDnsPolicyRequest): Promise<DnsPolicy> {
  return apiPut<DnsPolicy>(`${API_BASE}/dns-policies/${id}`, req)
}

export async function deleteDnsPolicy(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/dns-policies/${id}`)
}

export async function adoptDnsPolicy(hostId: string): Promise<DnsPolicy> {
  return apiPost<DnsPolicy>(`${API_BASE}/dns-policies/adopt`, { host_id: hostId })
}

export async function syncDns(): Promise<{ status: string; zones: number }> {
  return apiPost<{ status: string; zones: number }>(`${API_BASE}/dns-policies/sync`)
}

// ─── VPN Mesh ────────────────────────────────────────────────────────────────

export type VpnTopology = 'full-mesh' | 'hub-spoke' | 'point-to-point'

export interface VpnPeer {
  public_key: string
  endpoint?: string
  allowed_ips: string[]
}

export interface VpnTunnel {
  id: string
  name: string
  description?: string
  interface_name: string
  listen_port: number
  private_key_set?: boolean
  private_key_ref?: string
  address?: string
  peers: VpnPeer[]
  enabled: boolean
  managed?: boolean
  created: string
  updated: string
}

export interface CreateVpnTunnelRequest {
  name: string
  description?: string
  interface_name: string
  listen_port?: number
  address: string
  private_key_ref: string
  peers?: VpnPeer[]
  enabled?: boolean
}

export interface VpnNetwork {
  id: string
  name: string
  description?: string
  topology: VpnTopology
  address_range: string
  labels: Record<string, string>
  tunnel_ids: string[]
  created: string
  updated: string
}

export interface CreateVpnNetworkRequest {
  name: string
  description?: string
  topology?: VpnTopology
  address_range: string
  labels?: Record<string, string>
  tunnel_ids?: string[]
}

export async function listVpnTunnels(): Promise<VpnTunnel[]> {
  return apiGet<VpnTunnel[]>(`${API_BASE}/vpn-tunnels`)
}

export async function createVpnTunnel(req: CreateVpnTunnelRequest): Promise<VpnTunnel> {
  return apiPost<VpnTunnel>(`${API_BASE}/vpn-tunnels`, req)
}

export async function getVpnTunnel(id: string): Promise<VpnTunnel> {
  return apiGet<VpnTunnel>(`${API_BASE}/vpn-tunnels/${id}`)
}

export async function updateVpnTunnel(id: string, req: CreateVpnTunnelRequest): Promise<VpnTunnel> {
  return apiPut<VpnTunnel>(`${API_BASE}/vpn-tunnels/${id}`, req)
}

export async function deleteVpnTunnel(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/vpn-tunnels/${id}`)
}

export async function adoptVpnTunnel(hostId: string): Promise<VpnTunnel> {
  return apiPost<VpnTunnel>(`${API_BASE}/vpn-tunnels/adopt`, { host_id: hostId })
}

export async function listVpnNetworks(): Promise<VpnNetwork[]> {
  return apiGet<VpnNetwork[]>(`${API_BASE}/vpn-networks`)
}

export async function createVpnNetwork(req: CreateVpnNetworkRequest): Promise<VpnNetwork> {
  return apiPost<VpnNetwork>(`${API_BASE}/vpn-networks`, req)
}

export async function deleteVpnNetwork(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/vpn-networks/${id}`)
}

export async function syncVpn(): Promise<{ status: string; tunnels: number }> {
  return apiPost<{ status: string; tunnels: number }>(`${API_BASE}/vpn-tunnels/sync`)
}

export async function vpnStatus(): Promise<{ active_tunnels: number; networks: number }> {
  return apiGet<{ active_tunnels: number; networks: number }>(`${API_BASE}/vpn-tunnels/status`)
}

// ─── Packet Mirror ───────────────────────────────────────────────────────────

export type MirrorDirection = 'ingress' | 'egress' | 'both'

export interface MirrorFilter {
  protocol?: string
  src_cidr?: string
  dst_cidr?: string
  dst_port?: number
}

export interface MirrorSession {
  id: string
  name: string
  description?: string
  selector: { match_labels?: Record<string, string> }
  collector_type?: string
  collector_target: string
  direction: MirrorDirection
  filter?: MirrorFilter
  enabled: boolean
  managed?: boolean
  created: string
  updated: string
}

export interface CreateMirrorSessionRequest {
  name: string
  description?: string
  selector: { match_labels: Record<string, string> }
  direction?: MirrorDirection
  collector_target: string
  filter?: MirrorFilter
  enabled?: boolean
}

export async function listMirrorSessions(): Promise<MirrorSession[]> {
  return apiGet<MirrorSession[]>(`${API_BASE}/mirror-sessions`)
}

export async function createMirrorSession(req: CreateMirrorSessionRequest): Promise<MirrorSession> {
  return apiPost<MirrorSession>(`${API_BASE}/mirror-sessions`, req)
}

export async function getMirrorSession(id: string): Promise<MirrorSession> {
  return apiGet<MirrorSession>(`${API_BASE}/mirror-sessions/${id}`)
}

export async function updateMirrorSession(id: string, req: CreateMirrorSessionRequest): Promise<MirrorSession> {
  return apiPut<MirrorSession>(`${API_BASE}/mirror-sessions/${id}`, req)
}

export async function deleteMirrorSession(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/mirror-sessions/${id}`)
}

export async function adoptMirrorSession(hostId: string): Promise<MirrorSession> {
  return apiPost<MirrorSession>(`${API_BASE}/mirror-sessions/adopt`, { host_id: hostId })
}

export async function syncMirror(): Promise<{ status: string; sessions: number }> {
  return apiPost<{ status: string; sessions: number }>(`${API_BASE}/mirror-sessions/sync`)
}

export async function mirrorStatus(): Promise<{ active_sessions: number; mirrored_vms: number }> {
  return apiGet<{ active_sessions: number; mirrored_vms: number }>(`${API_BASE}/mirror-sessions/status`)
}

// ─── NAT Gateway ─────────────────────────────────────────────────────────────

export type NatRuleType = 'masquerade' | 'snat' | 'dnat' | 'hairpin'

export interface NatRule {
  id: string
  name: string
  description?: string
  rule_type: NatRuleType
  source_cidr?: string
  dest_cidr?: string
  protocol?: 'tcp' | 'udp' | 'any'
  dest_port?: number
  translate_to?: string
  translate_port?: number
  outbound_interface?: string
  enabled: boolean
  managed?: boolean
  created: string
  updated: string
}

export interface CreateNatRuleRequest {
  name: string
  description?: string
  rule_type: NatRuleType
  source_cidr?: string
  dest_cidr?: string
  protocol?: 'tcp' | 'udp' | 'any'
  dest_port?: number
  translate_to?: string
  translate_port?: number
  outbound_interface?: string
  enabled?: boolean
}

export interface NatPool {
  id: string
  name: string
  ip_ranges: string[]
  port_range?: string
  created: string
  updated: string
}

export interface CreateNatPoolRequest {
  name: string
  ip_ranges: string[]
  port_range?: string
}

export interface NatGatewayConfig {
  id: string
  name: string
  description?: string
  subnet: string
  outbound_interface: string
  enabled: boolean
  created: string
  updated: string
}

export interface CreateNatGatewayRequest {
  name: string
  description?: string
  subnet: string
  outbound_interface: string
  enabled?: boolean
}

export async function listNatRules(): Promise<NatRule[]> {
  return apiGet<NatRule[]>(`${API_BASE}/nat-rules`)
}

export async function createNatRule(req: CreateNatRuleRequest): Promise<NatRule> {
  return apiPost<NatRule>(`${API_BASE}/nat-rules`, req)
}

export async function getNatRule(id: string): Promise<NatRule> {
  return apiGet<NatRule>(`${API_BASE}/nat-rules/${id}`)
}

export async function updateNatRule(id: string, req: CreateNatRuleRequest): Promise<NatRule> {
  return apiPut<NatRule>(`${API_BASE}/nat-rules/${id}`, req)
}

export async function deleteNatRule(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/nat-rules/${id}`)
}

export async function adoptNatRule(hostId: string): Promise<NatRule> {
  return apiPost<NatRule>(`${API_BASE}/nat-rules/adopt`, { host_id: hostId })
}

export async function listNatPools(): Promise<NatPool[]> {
  return apiGet<NatPool[]>(`${API_BASE}/nat-pools`)
}

export async function createNatPool(req: CreateNatPoolRequest): Promise<NatPool> {
  return apiPost<NatPool>(`${API_BASE}/nat-pools`, req)
}

export async function deleteNatPool(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/nat-pools/${id}`)
}

export async function listNatGateways(): Promise<NatGatewayConfig[]> {
  return apiGet<NatGatewayConfig[]>(`${API_BASE}/nat-gateways`)
}

export async function createNatGateway(req: CreateNatGatewayRequest): Promise<NatGatewayConfig> {
  return apiPost<NatGatewayConfig>(`${API_BASE}/nat-gateways`, req)
}

export async function deleteNatGateway(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/nat-gateways/${id}`)
}

export async function syncNat(): Promise<{ status: string; rules: number }> {
  return apiPost<{ status: string; rules: number }>(`${API_BASE}/nat-rules/sync`)
}

export async function natStatus(): Promise<{ active_rules: number; gateways: number }> {
  return apiGet<{ active_rules: number; gateways: number }>(`${API_BASE}/nat-rules/status`)
}

// ─── Network Monitor ─────────────────────────────────────────────────────────

export type AlertSeverity = 'info' | 'warning' | 'critical'
export type MetricDirection = 'inbound' | 'outbound' | 'both'

export interface MonitorThreshold {
  metric: string
  value: number
  unit: string
  direction: MetricDirection
  severity: AlertSeverity
}

export interface MonitorPolicy {
  id: string
  name: string
  description?: string
  labels?: Record<string, string>
  selector?: { match_labels?: Record<string, string> }
  thresholds: MonitorThreshold[]
  interval_seconds?: number
  sample_interval_secs?: number
  enabled: boolean
  managed?: boolean
  created: string
  updated: string
}

export interface CreateMonitorPolicyRequest {
  name: string
  description?: string
  selector: { match_labels: Record<string, string> }
  thresholds: MonitorThreshold[]
  sample_interval_secs?: number
  enabled?: boolean
}

export interface NetworkMetrics {
  vm_name: string
  interface_name: string
  rx_bytes: number
  tx_bytes: number
  rx_packets: number
  tx_packets: number
  rx_errors: number
  tx_errors: number
  timestamp: string
}

export interface BandwidthAlert {
  id: string
  policy_id: string
  vm_name: string
  metric: string
  value: number
  threshold: number
  severity: AlertSeverity
  acknowledged: boolean
  created: string
}

export async function listMonitorPolicies(): Promise<MonitorPolicy[]> {
  return apiGet<MonitorPolicy[]>(`${API_BASE}/monitor-policies`)
}

export async function createMonitorPolicy(req: CreateMonitorPolicyRequest): Promise<MonitorPolicy> {
  return apiPost<MonitorPolicy>(`${API_BASE}/monitor-policies`, req)
}

export async function getMonitorPolicy(id: string): Promise<MonitorPolicy> {
  return apiGet<MonitorPolicy>(`${API_BASE}/monitor-policies/${id}`)
}

export async function updateMonitorPolicy(id: string, req: CreateMonitorPolicyRequest): Promise<MonitorPolicy> {
  return apiPut<MonitorPolicy>(`${API_BASE}/monitor-policies/${id}`, req)
}

export async function deleteMonitorPolicy(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/monitor-policies/${id}`)
}

export async function adoptMonitorPolicy(hostId: string): Promise<MonitorPolicy> {
  return apiPost<MonitorPolicy>(`${API_BASE}/monitor-policies/adopt`, { host_id: hostId })
}

export async function syncMonitor(): Promise<{ status: string; policies: number }> {
  return apiPost<{ status: string; policies: number }>(`${API_BASE}/monitor-policies/sync`)
}

export async function monitorStatus(): Promise<{ active_policies: number; monitored_vms: number }> {
  return apiGet<{ active_policies: number; monitored_vms: number }>(`${API_BASE}/monitor-policies/status`)
}

export async function listNetworkMetrics(): Promise<NetworkMetrics[]> {
  return apiGet<NetworkMetrics[]>(`${API_BASE}/network-metrics`)
}

export async function listBandwidthAlerts(): Promise<BandwidthAlert[]> {
  return apiGet<BandwidthAlert[]>(`${API_BASE}/bandwidth-alerts`)
}

export async function acknowledgeBandwidthAlert(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/bandwidth-alerts/${id}/acknowledge`)
}
