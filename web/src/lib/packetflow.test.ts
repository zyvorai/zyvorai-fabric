// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, expect, it } from 'vitest'
import {
  buildHops,
  filterViews,
  fromHubbleFlow,
  fromRawFlow,
  normalizeVerdict,
  protoName,
} from './packetflow'

const rec = {
  identity: 1042,
  family: 4,
  source: '10.0.0.2',
  destination: '1.1.1.1',
  source_port: 54321,
  destination_port: 443,
  protocol: 6,
  verdict: 'allow',
  packets: 12,
  bytes: 980,
  last_seen_ns: 1700000000,
}

describe('packetflow', () => {
  it('maps proto and verdict', () => {
    expect(protoName(6)).toBe('tcp')
    expect(normalizeVerdict('drop')).toBe('DROPPED')
    expect(normalizeVerdict('allow')).toBe('FORWARDED')
  })

  it('builds egress path without cilium hop', () => {
    const names = buildHops('EGRESS', 'FORWARDED', 'ebpf', 'web-1', 'tcp', 443).map((h) => h.name)
    expect(names).toEqual(['web-1', 'tap', 'tc-clsact', 'uplink', 'peer'])
  })

  it('builds ingress path with cilium coexistence hop', () => {
    const names = buildHops('INGRESS', 'FORWARDED', 'cilium', 'web-1', 'tcp', 80).map((h) => h.name)
    expect(names).toEqual(['peer', 'uplink', 'cilium', 'tc-clsact', 'tap', 'web-1'])
  })

  it('treats destination=guest as ingress', () => {
    const view = fromRawFlow(
      { ...rec, source: '9.9.9.9', destination: '10.0.0.2', destination_port: 8080 },
      { vmName: 'web-1', guestIp: '10.0.0.2' },
    )
    expect(view.direction).toBe('INGRESS')
    expect(view.verdict).toBe('FORWARDED')
  })

  it('uses Hubble hops when present', () => {
    const view = fromHubbleFlow({
      verdict: 'DROPPED',
      traffic_direction: 'EGRESS',
      IP: { source: '10.0.0.2', destination: '1.1.1.1' },
      l4: { protocol: 'tcp', source_port: 1, destination_port: 443 },
      source: { identity: 9, labels: ['app=web'], pod_name: 'web-1' },
      hops: [{ index: 0, name: 'custom', role: 'x', detail: 'y' }],
      packets: 3,
      bytes: 9,
    })
    expect(view.verdict).toBe('DROPPED')
    expect(view.hops).toHaveLength(1)
    expect(view.hops[0].name).toBe('custom')
  })

  it('filters verdict and protocol', () => {
    const a = fromRawFlow(rec, { vmName: 'a' })
    const b = fromRawFlow({ ...rec, verdict: 'drop', protocol: 17, destination_port: 53 }, { vmName: 'b' })
    expect(filterViews([a, b], 'DROPPED', 'all')).toHaveLength(1)
    expect(filterViews([a, b], 'all', 'tcp')).toHaveLength(1)
  })
})
