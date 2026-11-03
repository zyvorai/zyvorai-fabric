// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest'
import { buildNetworkMap, describeConnections, netTone } from './networkMap'

const ids = (m: ReturnType<typeof buildNetworkMap>) => m.nodes.map((n) => n.id)
const edge = (m: ReturnType<typeof buildNetworkMap>, a: string, b: string) => m.edges.some((e) => e.from === a && e.to === b)

describe('buildNetworkMap', () => {
  const input = {
    links: [{ name: 'eth0', operational_state: 'carrier' }, { name: 'eth1', operational_state: 'carrier' }, { name: 'eth2', operational_state: 'no-carrier' }],
    bonds: [{ name: 'bond0', slave_interfaces: ['eth0', 'eth1'], operational_state: 'routable' }],
    vlans: [{ name: 'vlan100', parent_interface: 'bond0' }],
    macvtaps: [{ name: 'mv0', parent_interface: 'eth2' }],
    bridges: [{ name: 'br0', operational_state: 'routable' }],
    taps: [{ name: 'tap-web', bridge: 'br0' }],
    netfiles: [{ match_name: 'vlan100', bridge: 'br0' }],
  }
  const m = buildNetworkMap(input)

  it('links a bond to its members, and a VLAN or macvtap to its parent', () => {
    expect(edge(m, 'interface:eth0', 'bond:bond0')).toBe(true)
    expect(edge(m, 'interface:eth1', 'bond:bond0')).toBe(true)
    expect(edge(m, 'bond:bond0', 'vlan:vlan100')).toBe(true)
    expect(edge(m, 'interface:eth2', 'macvtap:mv0')).toBe(true)
  })

  it('resolves a network file to the configured device, not a second plain interface of the same name', () => {
    expect(edge(m, 'vlan:vlan100', 'bridge:br0')).toBe(true)
    expect(ids(m)).not.toContain('interface:vlan100')
  })

  it('links a bridge to the taps plugged into it', () => {
    expect(edge(m, 'bridge:br0', 'tap:tap-web')).toBe(true)
  })

  it('puts nodes in columns: interfaces, virtual devices, bridges, taps', () => {
    const col = (id: string) => m.nodes.find((n) => n.id === id)!.column
    expect([col('interface:eth0'), col('bond:bond0'), col('vlan:vlan100'), col('bridge:br0'), col('tap:tap-web')]).toEqual(['interfaces', 'virtual', 'virtual', 'bridges', 'taps'])
  })

  it('carries each interface\'s reported state, and has no duplicate edges', () => {
    expect(m.nodes.find((n) => n.id === 'interface:eth2')?.state).toBe('no-carrier')
    const keys = m.edges.map((e) => `${e.from}>${e.to}`)
    expect(new Set(keys).size).toBe(keys.length)
  })

  it('draws a reference to something not listed, labelled missing, and never drops the edge', () => {
    const g = buildNetworkMap({ taps: [{ name: 'tap-x', bridge: 'virbr9' }] })
    const b = g.nodes.find((n) => n.id === 'bridge:virbr9')
    expect(b?.missing).toBe(true)
    expect(edge(g, 'bridge:virbr9', 'tap:tap-x')).toBe(true)
  })

  it('handles nothing at all, and an interface that belongs to itself', () => {
    expect(buildNetworkMap({})).toEqual({ nodes: [], edges: [] })
    expect(buildNetworkMap({ bonds: [{ name: 'b', slave_interfaces: ['b'] }] }).edges).toEqual([])
  })
})

describe('interfaces to always draw', () => {
  it('draws a physical NIC that nothing refers to, so a host with no bonds, VLANs or taps still shows what it has', () => {
    const m = buildNetworkMap({ interfaces: [{ name: 'eno1', operational_state: 'routable' }, { name: 'eno2' }] })
    expect(ids(m)).toEqual(['interface:eno1', 'interface:eno2'])
    expect(m.edges).toEqual([])
    expect(m.nodes[0].state).toBe('routable')
  })
  it('does not draw a name twice when a bond or bridge has it', () => {
    const m = buildNetworkMap({ bonds: [{ name: 'bond0', slave_interfaces: ['eno1'] }], interfaces: [{ name: 'eno1' }, { name: 'bond0' }] })
    expect(ids(m).filter((i) => i.endsWith(':bond0'))).toEqual(['bond:bond0'])
    expect(ids(m).filter((i) => i.endsWith(':eno1'))).toEqual(['interface:eno1'])
  })
})

describe('a host with nothing connected (the shape seen on the lab host)', () => {
  const m = buildNetworkMap({
    bridges: [{ name: 'mfork0', operational_state: 'down' }, { name: 'virbr0', operational_state: 'down' }],
    vxlans: [{ name: 'cilium_vxlan' }],
    interfaces: [
      { name: 'cilium_host', operational_state: 'up' },
      { name: 'cilium_net', operational_state: 'up' },
      { name: 'cilium_vxlan', operational_state: 'unknown' },
      { name: 'eno8303', operational_state: 'up' },
      { name: 'eno8403', operational_state: 'down' },
    ],
  })
  it('still draws what the host has: its NICs, bridges and VXLAN, with no lines', () => {
    expect(ids(m)).toEqual([
      'interface:cilium_host', 'interface:cilium_net', 'interface:eno8303', 'interface:eno8403',
      'vxlan:cilium_vxlan', 'bridge:mfork0', 'bridge:virbr0',
    ])
    expect(m.edges).toEqual([])
  })
  it('draws a VXLAN once even though it is also listed as an interface, and tones the reported states', () => {
    expect(ids(m).filter((i) => i.endsWith(':cilium_vxlan'))).toEqual(['vxlan:cilium_vxlan'])
    const tone = (id: string) => netTone(m.nodes.find((n) => n.id === id)?.state)
    expect(tone('interface:eno8303')).toBe('proof') // "up" is a working link, not "not reported"
    expect(tone('interface:eno8403')).toBe('neutral')
    expect(tone('bridge:virbr0')).toBe('neutral')
  })
})

describe('netTone', () => {
  it('is green only for a clearly working link, amber for degraded, and never red', () => {
    expect(['routable', 'carrier', 'enslaved'].map(netTone)).toEqual(['proof', 'proof', 'proof'])
    // the plain vocabulary a real host's bridges, VXLANs and network files report
    expect(netTone('up')).toBe('proof')
    expect(['down', 'unknown'].map(netTone)).toEqual(['neutral', 'neutral'])
    expect(netTone('degraded')).toBe('warn')
    expect(['no-carrier', 'off', 'dormant', undefined, 'whatever'].map(netTone)).toEqual(['neutral', 'neutral', 'neutral', 'neutral', 'neutral'])
  })
})

describe('describeConnections', () => {
  it('says what each connected node connects, in words', () => {
    const lines = describeConnections(buildNetworkMap({ bonds: [{ name: 'bond0', slave_interfaces: ['eth1', 'eth0'] }] }))
    expect(lines).toContain('bond0 connects eth0, eth1')
    expect(lines).toContain('eth0 connects bond0')
  })
  it('says nothing for a node with no connections', () => {
    expect(describeConnections(buildNetworkMap({ bridges: [{ name: 'br0' }] }))).toEqual([])
  })
})
