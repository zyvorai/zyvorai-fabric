// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { StaticRouter } from 'react-router'
import { LiveFabric } from '../LiveFabric'
import { LoadBar } from '../LoadBar'
import { GettingStarted } from '../GettingStarted'
import { StatusBadge } from '../ui'
import { ConsoleMock } from '../ConsoleMock'
import { FleetBar } from '../FleetBar'
import { VMLoad } from '../VMLoad'
import { EmptyState } from '../ui'
import { PremiumLoginShell } from '../PremiumLoginShell'
import { SAMPLE_FLEET } from '../../lib/sampleFleet'
import { fleetCounts } from '../../lib/fleet'

const vm = (name: string, state: string) => ({ name, state })

describe('LiveFabric', () => {
  it('has one node per VM, in the colour of its state, and says what it shows in words', () => {
    const html = renderToStaticMarkup(<LiveFabric vms={[vm('a', 'running'), vm('b', 'paused'), vm('c', 'failed'), vm('d', 'stopped')]} />)
    expect(html).toContain('aria-label="4 virtual machines: 1 running, 1 paused, 1 stopped, 1 failed"')
    expect(html).toContain('role="img"')
    for (const n of ['a: running', 'b: paused', 'c: failed', 'd: stopped']) expect(html).toContain(`<title>${n}</title>`)
    expect(html).toContain('var(--zf-proof)')
    expect(html).toContain('var(--zf-danger)')
  })

  it('moves only the running VM\'s line', () => {
    const html = renderToStaticMarkup(<LiveFabric vms={[vm('a', 'running'), vm('b', 'stopped')]} />)
    expect((html.match(/class="zf-flow"/g) ?? []).length).toBe(1)
  })

  it('shows the empty core for no VMs, and says how many did not fit', () => {
    const empty = renderToStaticMarkup(<LiveFabric vms={[]} />)
    expect(empty).toContain('No virtual machines yet')
    expect(empty).not.toContain('<title>')
    const many = renderToStaticMarkup(<LiveFabric vms={Array.from({ length: 11 }, (_, i) => vm(`vm-${i}`, 'running'))} />)
    expect(many).toContain('+3 more')
    expect((many.match(/<title>/g) ?? []).length).toBe(8)
  })
})

describe('LoadBar', () => {
  it('is a meter with the reading in text, amber from 50 and red from 80', () => {
    const html = renderToStaticMarkup(<LoadBar label="web-01 CPU" value={83.4} />)
    expect(html).toContain('role="meter"')
    expect(html).toContain('aria-label="web-01 CPU 83%"')
    expect(html).toContain('aria-valuenow="83"')
    expect(html).toContain('83%')
    expect(html).toContain('var(--zf-danger)')
    expect(renderToStaticMarkup(<LoadBar label="x" value={55} />)).toContain('var(--zf-warning)')
    expect(renderToStaticMarkup(<LoadBar label="x" value={5} />)).toContain('var(--zf-proof)')
  })
  it('draws a dash and no bar for a missing reading, never a zero', () => {
    const html = renderToStaticMarkup(<LoadBar label="x" value={undefined} />)
    expect(html).toContain('–')
    expect(html).not.toContain('meter')
  })
})

describe('GettingStarted', () => {
  it('lists the four steps in order with the first as the one to take', () => {
    const html = renderToStaticMarkup(<StaticRouter location="/app"><GettingStarted /></StaticRouter>)
    expect((html.match(/<li>/g) ?? []).length).toBe(4)
    expect(html.indexOf('Create your first VM')).toBeLessThan(html.indexOf('Invite your team'))
    expect(html).toContain('data-tone="brand"')
    expect((html.match(/data-tone="brand"/g) ?? []).length).toBe(1)
    expect(html).toContain('href="/app/create"')
  })
})

describe('StatusBadge', () => {
  it('shows a stopped VM as a normal state, not an error', () => {
    const html = renderToStaticMarkup(<StatusBadge status="stopped" />)
    expect(html).not.toContain('red')
    expect(renderToStaticMarkup(<StatusBadge status="failed" />)).toContain('red')
  })
})

describe('decorative LiveFabric', () => {
  it('is hidden from assistive tech and carries no role, label or titles', () => {
    const html = renderToStaticMarkup(<LiveFabric vms={SAMPLE_FLEET} decorative />)
    expect(html).toContain('aria-hidden="true"')
    expect(html).not.toContain('role="img"')
    expect(html).not.toContain('aria-label')
    expect(html).not.toContain('<title>')
  })
})

describe('SAMPLE_FLEET', () => {
  it('is an invented fleet of eight with six running, so the marketing copy and the picture agree', () => {
    expect(fleetCounts(SAMPLE_FLEET)).toMatchObject({ total: 8, running: 6, paused: 1, stopped: 1, failed: 0 })
  })
})

describe('ConsoleMock', () => {
  const html = renderToStaticMarkup(<ConsoleMock />)
  it('is one described picture, labelled as example data', () => {
    expect(html).toContain('role="img"')
    expect(html).toContain('Example of the Zyvor Fabric console')
    expect(html).toContain('six running')
    expect(html).toContain('Example data.')
  })
  it('is built from the real pieces: the fleet map, the donut and load bars', () => {
    expect(html).toContain('zf-flow') // a running VM's line in the fleet map
    expect(html).toContain('role="meter"')
    expect(html).toContain('<caption>VMs</caption>')
  })
})

describe('PremiumLoginShell backdrop', () => {
  const shell = (props: Record<string, unknown>) =>
    renderToStaticMarkup(
      <PremiumLoginShell productName="Zyvor Fabric" {...props}>
        <form />
      </PremiumLoginShell>,
    )
  it('puts the art behind the credentials card, hidden from assistive tech', () => {
    const html = shell({ backdrop: <i data-art="x" /> })
    expect(html).toContain('<div class="login-backdrop" aria-hidden="true"><i data-art="x"></i></div>')
    expect(html.indexOf('login-backdrop')).toBeGreaterThan(html.indexOf('login-chapter-sign-in'))
  })
  it('draws nothing extra without a backdrop', () => {
    expect(shell({})).not.toContain('login-backdrop')
  })
})

describe('FleetBar', () => {
  const counts = fleetCounts([vm('a', 'running'), vm('b', 'running'), vm('c', 'paused'), vm('d', 'stopped'), vm('e', 'failed')])
  it('prints every state with VMs as text, failed included, and draws the bar from the same numbers', () => {
    const html = renderToStaticMarkup(<FleetBar counts={counts} />)
    expect(html).toContain('5 VMs')
    for (const t of ['2 running', '1 paused', '1 stopped', '1 failed']) expect(html).toContain(t)
    expect(html).toContain('width:40%')
  })
  it('draws only a failed VM in red; stopped is grey', () => {
    const html = renderToStaticMarkup(<FleetBar counts={counts} />)
    expect(html).toContain('var(--zf-danger)')
    expect(renderToStaticMarkup(<FleetBar counts={fleetCounts([vm('a', 'stopped')])} />)).not.toContain('var(--zf-danger)')
  })
  it('says how many a search leaves, and uses the singular for one', () => {
    expect(renderToStaticMarkup(<FleetBar counts={counts} shown={2} />)).toContain('2 of 5 VMs')
    expect(renderToStaticMarkup(<FleetBar counts={fleetCounts([vm('a', 'running')])} />)).toContain('1 VM<')
  })
})

describe('VMLoad', () => {
  it('shows CPU and memory bars with their names for a running VM that has a reading', () => {
    const html = renderToStaticMarkup(<VMLoad name="web-01" state="running" load={{ cpu: 61, mem: 32 }} />)
    expect(html).toContain('aria-label="web-01 CPU 61%"')
    expect(html).toContain('aria-label="web-01 memory 32%"')
  })
  it('shows nothing for a stopped VM or one with no reading yet, so it never claims 0%', () => {
    expect(renderToStaticMarkup(<VMLoad name="x" state="stopped" load={{ cpu: 1, mem: 1 }} />)).toBe('')
    expect(renderToStaticMarkup(<VMLoad name="x" state="running" load={undefined} />)).toBe('')
  })
})

describe('EmptyState illustration', () => {
  it('replaces the icon tile with the picture when one is given, and keeps the words', () => {
    const html = renderToStaticMarkup(<EmptyState illustration={<i data-art="pic" />} title="No virtual machines" description="Create one." />)
    expect(html).toContain('data-art="pic"')
    expect(html).not.toContain('icon-tile')
    expect(html).toContain('No virtual machines')
  })
  it('still draws the icon tile as before', () => {
    expect(renderToStaticMarkup(<EmptyState icon={<i data-icon="x" />} title="T" />)).toContain('icon-tile')
  })
})
