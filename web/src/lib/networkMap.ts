// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/** The graph behind the Network page's map: which interface belongs to which bond or bridge, which VLAN, macvtap or VXLAN sits on
    which parent, and which tap is plugged into which bridge. Built only from the fields the API already returns (a bond's members,
    a network file's `bridge` and `bond`, a parent interface, a tap's bridge); nothing is inferred. Pure, so it is tested without a DOM. */

export type NodeKind = 'interface' | 'bond' | 'vlan' | 'vxlan' | 'macvtap' | 'bridge' | 'tap'
export type Column = 'interfaces' | 'virtual' | 'bridges' | 'taps'

export interface NetNode {
  id: string
  kind: NodeKind
  name: string
  column: Column
  /** networkd's operational state when the API reported one. */
  state?: string
  /** Referenced by something but not configured or listed. Drawn, and labelled as such, never hidden. */
  missing?: boolean
}

export interface NetEdge {
  from: string
  to: string
}

export interface NetworkMapInput {
  links?: Array<{ name: string; operational_state?: string }>
  bonds?: Array<{ name: string; slave_interfaces?: string[]; operational_state?: string }>
  vlans?: Array<{ name: string; parent_interface?: string; operational_state?: string }>
  vxlans?: Array<{ name: string; parent_interface?: string; operational_state?: string }>
  macvtaps?: Array<{ name: string; parent_interface?: string; operational_state?: string }>
  bridges?: Array<{ name: string; operational_state?: string }>
  taps?: Array<{ name: string; bridge?: string; operational_state?: string }>
  netfiles?: Array<{ match_name: string; bridge?: string; bond?: string }>
}

export interface NetworkMap {
  nodes: NetNode[]
  edges: NetEdge[]
}

const COLUMN: Record<NodeKind, Column> = {
  interface: 'interfaces',
  bond: 'virtual',
  vlan: 'virtual',
  vxlan: 'virtual',
  macvtap: 'virtual',
  bridge: 'bridges',
  tap: 'taps',
}

export function buildNetworkMap(input: NetworkMapInput): NetworkMap {
  const nodes = new Map<string, NetNode>()
  const byName = new Map<string, NetNode>()
  const edges = new Map<string, NetEdge>()
  const add = (kind: NodeKind, name: string, state?: string, missing = false): NetNode => {
    const id = `${kind}:${name}`
    let n = nodes.get(id)
    if (!n) {
      n = { id, kind, name, column: COLUMN[kind], state, missing: missing || undefined }
      nodes.set(id, n)
      // A configured virtual device wins over a plain interface of the same name.
      if (!byName.has(name) || byName.get(name)!.kind === 'interface') byName.set(name, n)
    }
    return n
  }
  const link = (a: NetNode, b: NetNode) => {
    if (a.id === b.id) return
    const key = `${a.id}>${b.id}`
    if (!edges.has(key)) edges.set(key, { from: a.id, to: b.id })
  }
  /** A name from a parent, member or `bridge` field: a configured device if there is one, otherwise a plain interface. */
  const resolve = (name: string, fallback: NodeKind = 'interface'): NetNode => {
    const found = byName.get(name)
    if (found) return found
    return add(fallback, name, input.links?.find((l) => l.name === name)?.operational_state, fallback !== 'interface' || undefined)
  }

  // Configured devices first, so a later reference resolves to them and not to a placeholder interface.
  for (const b of input.bonds ?? []) add('bond', b.name, b.operational_state)
  for (const v of input.vlans ?? []) add('vlan', v.name, v.operational_state)
  for (const v of input.vxlans ?? []) add('vxlan', v.name, v.operational_state)
  for (const m of input.macvtaps ?? []) add('macvtap', m.name, m.operational_state)
  for (const b of input.bridges ?? []) add('bridge', b.name, b.operational_state)
  for (const t of input.taps ?? []) add('tap', t.name, t.operational_state)

  for (const b of input.bonds ?? []) for (const s of b.slave_interfaces ?? []) link(resolve(s), resolve(b.name))
  for (const v of input.vlans ?? []) if (v.parent_interface) link(resolve(v.parent_interface), resolve(v.name))
  for (const v of input.vxlans ?? []) if (v.parent_interface) link(resolve(v.parent_interface), resolve(v.name))
  for (const m of input.macvtaps ?? []) if (m.parent_interface) link(resolve(m.parent_interface), resolve(m.name))
  for (const f of input.netfiles ?? []) {
    if (f.bond) link(resolve(f.match_name), resolve(f.bond, 'bond'))
    if (f.bridge) link(resolve(f.match_name), resolve(f.bridge, 'bridge'))
  }
  for (const t of input.taps ?? []) if (t.bridge) link(resolve(t.bridge, 'bridge'), resolve(t.name))

  const order: Column[] = ['interfaces', 'virtual', 'bridges', 'taps']
  const sorted = [...nodes.values()].sort(
    (a, b) => order.indexOf(a.column) - order.indexOf(b.column) || a.kind.localeCompare(b.kind) || a.name.localeCompare(b.name),
  )
  return { nodes: sorted, edges: [...edges.values()] }
}

export type NetTone = 'proof' | 'warn' | 'neutral'

/** The tone of a networkd operational state. Only a clearly working link is green; `degraded` is amber; everything else, including
    a link with no carrier and one whose state was not reported, is neutral. No state is red: a down link is not an error. */
export function netTone(state: string | undefined): NetTone {
  switch (state) {
    case 'routable':
    case 'carrier':
    case 'enslaved':
      return 'proof'
    case 'degraded':
      return 'warn'
    default:
      return 'neutral'
  }
}

/** One sentence per node that has connections, for a screen reader: "bond0 connects eth0, eth1". */
export function describeConnections(map: NetworkMap): string[] {
  const name = new Map(map.nodes.map((n) => [n.id, n.name]))
  const links = new Map<string, Set<string>>()
  for (const e of map.edges) {
    for (const [a, b] of [[e.from, e.to], [e.to, e.from]] as const) {
      if (!links.has(a)) links.set(a, new Set())
      links.get(a)!.add(name.get(b) ?? b)
    }
  }
  return map.nodes
    .filter((n) => links.has(n.id))
    .map((n) => `${n.name} connects ${[...links.get(n.id)!].sort().join(', ')}`)
}
