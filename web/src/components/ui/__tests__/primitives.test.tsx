// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { Donut, GlowCard, StatusPulse, FlowLine, Illustration } from '..'
import type { IllustrationName } from '..'

const circles = (html: string) => (html.match(/<circle /g) ?? []).length

describe('Donut', () => {
  it('draws the track plus one ring per segment that has a value', () => {
    const html = renderToStaticMarkup(
      <Donut segments={[{ label: 'Running', value: 3, color: 'red' }, { label: 'Stopped', value: 0, color: 'blue' }, { label: 'Failed', value: 1, color: 'green' }]} />,
    )
    expect(circles(html)).toBe(3) // track + Running + Failed; the zero segment draws nothing
  })

  it('draws only the track when the total is zero, and never divides by zero', () => {
    const html = renderToStaticMarkup(<Donut segments={[{ label: 'Running', value: 0, color: 'red' }]} />)
    expect(circles(html)).toBe(1)
    expect(html).not.toContain('NaN')
  })

  it('lists every segment in a table for screen readers, zero ones included', () => {
    const html = renderToStaticMarkup(
      <Donut centerLabel="VMs" segments={[{ label: 'Running', value: 3, color: 'red' }, { label: 'Stopped', value: 0, color: 'blue' }]} />,
    )
    expect(html).toContain('class="sr-only"')
    expect(html).toContain('<th scope="row">Stopped</th><td>0</td>')
    expect(html).toContain('<caption>VMs</caption>')
  })

  it('leaves no gap when one segment is the whole ring', () => {
    const html = renderToStaticMarkup(<Donut size={100} thickness={10} segments={[{ label: 'All', value: 5, color: 'red' }]} />)
    const c = 2 * Math.PI * 45
    expect(html).toContain(`stroke-dasharray="${c} 0"`)
  })
})

describe('GlowCard', () => {
  it('carries its tone and whether it is interactive as attributes the CSS keys off', () => {
    const html = renderToStaticMarkup(<GlowCard tone="proof" interactive>x</GlowCard>)
    expect(html).toContain('data-tone="proof"')
    expect(html).toContain('data-interactive="true"')
    expect(renderToStaticMarkup(<GlowCard>x</GlowCard>)).toContain('data-tone="neutral"')
  })
})

describe('StatusPulse', () => {
  it('always names the state in words, so colour is never the only signal', () => {
    const html = renderToStaticMarkup(<StatusPulse tone="danger" live label="Failed" />)
    expect(html).toContain('aria-label="Failed"')
    expect(html).toContain('role="img"')
    expect(html).toContain('data-live="true"')
  })
})

describe('FlowLine', () => {
  it('moves only when animated; a still link keeps its dashes', () => {
    const moving = renderToStaticMarkup(<svg><FlowLine d="M0 0H10" /></svg>)
    const still = renderToStaticMarkup(<svg><FlowLine d="M0 0H10" animated={false} /></svg>)
    expect(moving).toContain('class="zf-flow"')
    expect(still).not.toContain('zf-flow')
    expect(still).toContain('stroke-dasharray="6 8"')
  })
})

describe('Illustration', () => {
  it.each<IllustrationName>(['no-vms', 'no-sessions', 'all-clear', 'sealed'])('%s is decorative and uses theme tokens only', (name) => {
    const html = renderToStaticMarkup(<Illustration name={name} />)
    expect(html).toContain('aria-hidden="true"')
    expect(html).not.toMatch(/#[0-9a-fA-F]{3,6}/) // no hard-coded colours: light and dark both follow the tokens
  })
})
