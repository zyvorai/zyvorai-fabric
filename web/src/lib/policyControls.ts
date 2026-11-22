// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import type { VmNetworkPolicy } from '../api/dataplane'
import { emptyPolicy } from '../api/dataplane'

export type EnforcementMode = 'open' | 'audit' | 'guard'
export type ControlAction = 'open' | 'audit' | 'guard' | 'invert' | 'block' | 'allow'
export type DropReason =
  | 'NONE'
  | 'POLICY_DENIED'
  | 'DEFAULT_DENY'
  | 'PORT_DENIED'
  | 'RATE_LIMIT'
  | 'AUDIT_WOULD_DROP'
  | 'UNKNOWN'

export interface ControlRequest {
  action: ControlAction
  cidr?: string
  port?: string
  entity?: string
}

export interface ExplainResult {
  verdict: string
  reason: DropReason
  would_drop: boolean
  matched_deny?: string
  matched_allow?: string
  summary: string
}

export function modeFromPolicy(p: VmNetworkPolicy): EnforcementMode {
  if (p.audit_mode) return 'audit'
  if (p.default_allow) return 'open'
  return 'guard'
}

export function hostCidr(addr: string): string {
  const raw = addr.trim()
  const a = raw.split('/')[0]
  if (a.includes(':') && !raw.includes('/')) return `${a}/128`
  if (a.includes('.') && !raw.includes('/')) return `${a}/32`
  return raw
}

function parseV4(s: string): number | null {
  const p = s.split('.')
  if (p.length !== 4) return null
  let n = 0
  for (const x of p) {
    const o = Number(x)
    if (!Number.isInteger(o) || o < 0 || o > 255) return null
    n = (n << 8) + o
  }
  return n >>> 0
}

export function cidrContains(cidr: string, ip: string): boolean {
  const host = ip.split('/')[0]
  if (cidr.includes(':') || host.includes(':')) {
    const net = cidr.split('/')[0]
    return net === host || cidr === host
  }
  const [net, plenRaw] = cidr.split('/')
  const plen = plenRaw === undefined ? 32 : Math.min(32, Number(plenRaw))
  const n = parseV4(net)
  const a = parseV4(host)
  if (n === null || a === null) return cidr === host
  if (plen === 0) return true
  const mask = plen === 32 ? 0xffffffff : (~((1 << (32 - plen)) - 1)) >>> 0
  return (n & mask) === (a & mask)
}

function pushUnique(list: string[] | undefined, value: string): string[] {
  const next = [...(list ?? [])]
  if (value && !next.includes(value)) next.push(value)
  return next
}

export function applyMode(p: VmNetworkPolicy, mode: EnforcementMode): VmNetworkPolicy {
  const next = { ...emptyPolicy(), ...p }
  if (mode === 'open') {
    next.default_allow = true
    next.audit_mode = false
  } else if (mode === 'audit') {
    next.audit_mode = true
    if (!next.sample_rate) next.sample_rate = 1
  } else {
    next.default_allow = false
    next.audit_mode = false
    if (!next.sample_rate) next.sample_rate = 1
  }
  return next
}

export function invertPolicy(p: VmNetworkPolicy): VmNetworkPolicy {
  return {
    ...p,
    allow_cidrs: [...(p.deny_cidrs ?? [])],
    deny_cidrs: [...(p.allow_cidrs ?? [])],
    default_allow: !p.default_allow,
  }
}

export function blockCidr(p: VmNetworkPolicy, cidr: string): VmNetworkPolicy {
  const c = hostCidr(cidr)
  return {
    ...p,
    deny_cidrs: pushUnique((p.deny_cidrs ?? []).filter((x) => x !== c), c),
  }
}

export function allowCidr(p: VmNetworkPolicy, cidr: string): VmNetworkPolicy {
  const c = hostCidr(cidr)
  return {
    ...p,
    deny_cidrs: (p.deny_cidrs ?? []).filter((x) => x !== c),
    allow_cidrs: pushUnique(p.allow_cidrs, c),
  }
}

export function applyControl(p: VmNetworkPolicy, req: ControlRequest): VmNetworkPolicy {
  switch (req.action) {
    case 'open':
      return applyMode(p, 'open')
    case 'audit':
      return applyMode(p, 'audit')
    case 'guard':
      return applyMode(p, 'guard')
    case 'invert':
      return invertPolicy(p)
    case 'block':
      if (!req.cidr) throw new Error('block requires cidr')
      return blockCidr(p, req.cidr)
    case 'allow': {
      let next = p
      if (req.cidr) next = allowCidr(next, req.cidr)
      if (req.port) next = { ...next, allow_ports: pushUnique(next.allow_ports, req.port) }
      if (req.entity) next = { ...next, entities: pushUnique(next.entities, req.entity) }
      if (!req.cidr && !req.port && !req.entity) throw new Error('allow requires cidr, port, or entity')
      return next
    }
    default:
      return p
  }
}

export function dropFlowTarget(destIp: string, destPort?: number, proto?: string): ControlRequest {
  const port =
    destPort && proto && proto !== 'any' ? `${proto.toLowerCase()}/${destPort}` : undefined
  return { action: 'block', cidr: destIp, port }
}

export function explain(
  policy: VmNetworkPolicy,
  destIp: string,
  destPort: number,
  proto: string,
): ExplainResult {
  const protoL = proto.toLowerCase()
  const token = `${protoL}/${destPort}`
  const matched_deny = (policy.deny_cidrs ?? []).find((c) => cidrContains(c, destIp))
  const matched_allow = (policy.allow_cidrs ?? []).find((c) => cidrContains(c, destIp))
  const portOk =
    !policy.allow_ports.length ||
    protoL === 'icmp' ||
    protoL === 'any' ||
    policy.allow_ports.some((p) => p.toLowerCase() === token)

  let verdict = 'FORWARDED'
  let reason: DropReason = 'NONE'
  let would_drop = false
  if (matched_deny) {
    verdict = 'DROPPED'
    reason = 'POLICY_DENIED'
    would_drop = true
  } else if (policy.allow_cidrs.length && !matched_allow && !policy.default_allow) {
    verdict = 'DROPPED'
    reason = 'DEFAULT_DENY'
    would_drop = true
  } else if (!portOk && !policy.default_allow) {
    verdict = 'DROPPED'
    reason = 'PORT_DENIED'
    would_drop = true
  } else if (!policy.default_allow && !policy.allow_cidrs.length && !policy.allow_ports.length) {
    verdict = 'DROPPED'
    reason = 'DEFAULT_DENY'
    would_drop = true
  }
  if (policy.audit_mode && would_drop) {
    verdict = 'AUDIT'
    reason = 'AUDIT_WOULD_DROP'
  }
  return {
    verdict,
    reason,
    would_drop,
    matched_deny,
    matched_allow,
    summary: `${verdict} ${reason} dest=${destIp}:${destPort}/${protoL}`,
  }
}

export const TEMPLATES: { id: string; label: string }[] = [
  { id: 'open', label: 'Open' },
  { id: 'guard', label: 'Guard / deny-all' },
  { id: 'web', label: 'Web egress' },
  { id: 'dns-only', label: 'DNS only' },
  { id: 'no-world', label: 'No world' },
]

export function templatePolicy(id: string): VmNetworkPolicy | null {
  const base = emptyPolicy()
  switch (id) {
    case 'open':
      return { ...base, default_allow: true, sample_rate: 1 }
    case 'guard':
    case 'deny':
      return { ...base, default_allow: false, sample_rate: 1 }
    case 'web':
      return {
        ...base,
        default_allow: false,
        allow_cidrs: ['0.0.0.0/0', '::/0'],
        allow_ports: ['tcp/80', 'tcp/443', 'udp/53'],
        max_egress_mbps: 100,
        max_egress_pps: 10000,
        sample_rate: 1,
      }
    case 'dns-only':
      return {
        ...base,
        default_allow: false,
        allow_cidrs: ['0.0.0.0/0'],
        allow_ports: ['udp/53', 'tcp/53'],
        sample_rate: 1,
      }
    case 'no-world':
      return {
        ...base,
        default_allow: false,
        deny_cidrs: ['0.0.0.0/0'],
        entities: ['host'],
        sample_rate: 1,
      }
    default:
      return null
  }
}
