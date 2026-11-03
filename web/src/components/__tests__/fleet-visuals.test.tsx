// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { StaticRouter } from 'react-router'
import { LiveFabric } from '../LiveFabric'
import { LoadBar } from '../LoadBar'
import { GettingStarted } from '../GettingStarted'
import { StatusBadge } from '../ui'

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
