// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

/** Hubble-style packet path for Fabric's Edge Dataplane console. */

export type FlowTheme = 'color' | 'normal'

export type TrafficDirection = 'INGRESS' | 'EGRESS'

export interface PacketHop {
  index: number
  name: string
  role: string
  detail: string
}

export interface HubbleEndpoint {
  identity: number
  labels: string[]
  pod_name?: string | null
}

export interface HubbleFlow {
  time?: string
  verdict: string
  drop_reason_desc?: string | null
  IP?: { source?: string; destination?: string; ipVersion?: string }
  l4?: { protocol?: string; source_port?: number; destination_port?: number }
  source?: HubbleEndpoint
  destination?: HubbleEndpoint
  Type?: string
  traffic_direction?: string
  summary?: string
  packets?: number
  bytes?: number
  hops?: PacketHop[]
}

export interface RawFlowRecord {
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

export interface PacketFlowView {
  time: string
  verdict: 'FORWARDED' | 'DROPPED' | 'AUDIT'
  dropReason?: string
  direction: TrafficDirection
  protocol: string
  family: number
  sourceIp: string
  destinationIp: string
  sourcePort: number
  destinationPort: number
  source: HubbleEndpoint
  destination: HubbleEndpoint
  packets: number
  bytes: number
  hops: PacketHop[]
  summary: string
  vmName: string
}

export function protoName(protocol: number): string {
  switch (protocol) {
    case 1:
      return 'icmp'
    case 6:
      return 'tcp'
    case 17:
      return 'udp'
    case 58:
      return 'icmpv6'
    default:
      return 'any'
  }
}

export function normalizeVerdict(raw: string): PacketFlowView['verdict'] {
  const l = raw.toLowerCase()
  if (l.includes('drop') || l.includes('deny')) return 'DROPPED'
  if (l.includes('audit')) return 'AUDIT'
  return 'FORWARDED'
}

export function buildHops(
  direction: TrafficDirection,
  verdict: string,
  dataplaneMode: string,
  vmName: string,
  proto: string,
  dport: number,
): PacketHop[] {
  const policy = `FluxVM Network Fabric eBPF  proto=${proto} dport=${dport} verdict=${verdict}`
  const cilium = dataplaneMode.toLowerCase() === 'cilium'
  const hops: PacketHop[] = []
  const push = (name: string, role: string, detail: string) => {
    hops.push({ index: hops.length, name, role, detail })
  }
  if (direction === 'EGRESS') {
    push(vmName, 'guest', 'virtio-net in guest namespace')
    push('tap', 'l2', 'host TAP/veth pair')
    push('tc-clsact', 'dataplane', policy)
    if (cilium) {
      push('cilium', 'coexist', 'mode=cilium — FluxVM does not write Cilium-private maps')
    }
    push('uplink', 'host', 'bridge / underlay toward peer')
    push('peer', 'identity', 'reserved:world or remote identity')
  } else {
    push('peer', 'identity', 'reserved:world or remote identity')
    push('uplink', 'host', 'bridge / underlay from peer')
    if (cilium) {
      push('cilium', 'coexist', 'mode=cilium — FluxVM does not write Cilium-private maps')
    }
    push('tc-clsact', 'dataplane', policy)
    push('tap', 'l2', 'host TAP/veth pair')
    push(vmName, 'guest', 'virtio-net in guest namespace')
  }
  return hops
}

export function fromRawFlow(
  rec: RawFlowRecord,
  opts?: { vmName?: string; guestIp?: string; dataplaneMode?: string; labels?: string[] },
): PacketFlowView {
  const vmName = opts?.vmName ?? 'vm'
  const verdict = normalizeVerdict(rec.verdict)
  const protocol = protoName(rec.protocol)
  let direction: TrafficDirection = 'EGRESS'
  const guest = opts?.guestIp?.split('/')[0]
  if (guest && rec.destination === guest) direction = 'INGRESS'
  if (guest && rec.source === guest) direction = 'EGRESS'
  const hops = buildHops(direction, verdict, opts?.dataplaneMode ?? 'ebpf', vmName, protocol, rec.destination_port)
  const source: HubbleEndpoint = {
    identity: rec.identity,
    labels: opts?.labels ?? [],
    pod_name: vmName,
  }
  const destination: HubbleEndpoint = {
    identity: 2,
    labels: ['reserved:world'],
  }
  const summary = `${rec.last_seen_ns} ${verdict} ${direction} ${protocol}/${rec.destination_port} ${rec.source}:${rec.source_port} → ${rec.destination}:${rec.destination_port}`
  return {
    time: String(rec.last_seen_ns),
    verdict,
    dropReason: verdict === 'DROPPED' ? 'POLICY_DENIED' : undefined,
    direction,
    protocol,
    family: rec.family,
    sourceIp: rec.source,
    destinationIp: rec.destination,
    sourcePort: rec.source_port,
    destinationPort: rec.destination_port,
    source,
    destination,
    packets: rec.packets,
    bytes: rec.bytes,
    hops,
    summary,
    vmName,
  }
}

export function fromHubbleFlow(flow: HubbleFlow, dataplaneMode = 'ebpf'): PacketFlowView {
  const protocol = String(flow.l4?.protocol ?? 'any')
  const sourceIp = String(flow.IP?.source ?? '')
  const destinationIp = String(flow.IP?.destination ?? '')
  const sourcePort = Number(flow.l4?.source_port ?? 0)
  const destinationPort = Number(flow.l4?.destination_port ?? 0)
  const vmName = flow.source?.pod_name || 'vm'
  const verdict = normalizeVerdict(flow.verdict || 'FORWARDED')
  const direction: TrafficDirection =
    String(flow.traffic_direction || 'EGRESS').toUpperCase() === 'INGRESS' ? 'INGRESS' : 'EGRESS'
  const hops =
    flow.hops && flow.hops.length > 0
      ? flow.hops
      : buildHops(direction, verdict, dataplaneMode, vmName, protocol, destinationPort)
  return {
    time: String(flow.time ?? ''),
    verdict,
    dropReason: flow.drop_reason_desc ?? undefined,
    direction,
    protocol,
    family: flow.IP?.ipVersion === 'IPv6' ? 6 : 4,
    sourceIp,
    destinationIp,
    sourcePort,
    destinationPort,
    source: flow.source ?? { identity: 0, labels: [] },
    destination: flow.destination ?? { identity: 2, labels: ['reserved:world'] },
    packets: flow.packets ?? 0,
    bytes: flow.bytes ?? 0,
    hops,
    summary:
      flow.summary ??
      `${verdict} ${direction} ${protocol}/${destinationPort} ${sourceIp}:${sourcePort} → ${destinationIp}:${destinationPort}`,
    vmName,
  }
}

export function filterViews(
  views: PacketFlowView[],
  verdict: string,
  protocol: string,
): PacketFlowView[] {
  const vWant = verdict.toUpperCase()
  const pWant = protocol.toLowerCase()
  return views.filter((v) => {
    if (vWant !== 'ALL' && v.verdict !== vWant) return false
    if (pWant !== 'all' && v.protocol !== pWant) return false
    return true
  })
}

export function verdictClass(verdict: PacketFlowView['verdict'], theme: FlowTheme): string {
  if (theme === 'normal') {
    if (verdict === 'DROPPED') return 'text-[#b42318] font-semibold'
    if (verdict === 'AUDIT') return 'text-[#92640a] font-semibold'
    return 'text-[#0d7a4a] font-semibold'
  }
  if (verdict === 'DROPPED') return 'text-[#f87171] font-semibold'
  if (verdict === 'AUDIT') return 'text-[#fbbf24] font-semibold'
  return 'text-[#34d399] font-semibold'
}
