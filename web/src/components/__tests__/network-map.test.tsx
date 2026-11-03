// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { NetworkMap } from '../NetworkMap'

const input = {
  links: [{ name: 'eth0', operational_state: 'carrier' }, { name: 'eth1', operational_state: 'carrier' }, { name: 'eth2', operational_state: 'no-carrier' }],
  bonds: [{ name: 'bond0', slave_interfaces: ['eth0', 'eth1'], operational_state: 'routable' }],
  vlans: [{ name: 'vlan100', parent_interface: 'bond0', operational_state: 'routable' }],
  macvtaps: [{ name: 'mv0', parent_interface: 'eth2' }],
  bridges: [{ name: 'br0', operational_state: 'routable' }],
  taps: [{ name: 'tap-web', bridge: 'br0', operational_state: 'carrier' }],
}

describe('NetworkMap', () => {
  const html = renderToStaticMarkup(<NetworkMap {...input} />)

  it('draws every device with its kind, and says which are up', () => {
    for (const n of ['eth0', 'bond0', 'vlan100', 'mv0', 'br0', 'tap-web']) expect(html).toContain(`>${n}<`)
    expect(html).toContain('<title>bond0: bond, routable</title>')
    expect(html).toContain('<title>eth2: interface, no-carrier</title>')
  })

  it('moves a line only when both ends are up', () => {
    // eth0 -> bond0, eth1 -> bond0, bond0 -> vlan100 and br0 -> tap-web are up; eth2 -> mv0 is not
    expect((html.match(/class="zf-flow"/g) ?? []).length).toBe(4)
    expect(html).toContain('stroke-dasharray="4 5"')
  })

  it('is decorative to assistive tech and lists the same connections in words', () => {
    expect(html).toContain('<svg viewBox')
    expect(html).toContain('aria-hidden="true"')
    expect(html).toContain('bond0 connects eth0, eth1, vlan100')
    expect(html).toContain('br0 connects tap-web')
  })

  it('scrolls inside its own box on a narrow screen, and that box is reachable by keyboard', () => {
    expect(html).toContain('tabindex="0"')
    expect(html).toContain('role="region"')
    expect(html).toContain('overflow-x-auto')
  })

  it('uses theme tokens only', () => {
    expect(html).not.toMatch(/#[0-9a-fA-F]{3,6}/)
  })

  it('draws physical NICs and says how many more were not drawn', () => {
    const h = renderToStaticMarkup(<NetworkMap bridges={[{ name: 'br0' }]} interfaces={[{ name: 'eno1', operational_state: 'routable' }]} hiddenInterfaces={3} />)
    expect(h).toContain('>eno1<')
    expect(h).toContain('3 more physical interfaces not drawn')
    expect(renderToStaticMarkup(<NetworkMap bridges={[{ name: 'br0' }]} hiddenInterfaces={1} />)).toContain('1 more physical interface not drawn')
  })

  it('draws nothing when nothing is configured', () => {
    expect(renderToStaticMarkup(<NetworkMap />)).toBe('')
  })

  it('marks something a device refers to but the API did not list', () => {
    const h = renderToStaticMarkup(<NetworkMap taps={[{ name: 'tap-x', bridge: 'virbr9' }]} />)
    expect(h).toContain('bridge (not listed)')
  })
})
