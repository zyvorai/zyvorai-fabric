// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { StaticRouter } from 'react-router'
import { ChipList, CtaBand, FeatureRow, MarketingHero } from '../blocks'
import { HubVisual, MerionVisual, NetworkVisual, StorageVisual, VmVisual } from '../visuals'

const wrap = (node: React.ReactNode) => renderToStaticMarkup(<StaticRouter location="/">{node}</StaticRouter>)

describe('ChipList', () => {
  it('is a labelled list with one item per chip', () => {
    const html = wrap(<ChipList items={['Ceph', 'Backups']} label="Storage covers" />)
    expect(html).toContain('aria-label="Storage covers"')
    expect((html.match(/<li /g) ?? []).length).toBe(2)
    expect(html).toContain('Ceph')
  })
})

describe('FeatureRow', () => {
  const row = (flip: boolean) =>
    wrap(<FeatureRow title="Network fabric" body="Bridges and VLANs." chips={['Bridges', 'VLANs']} visual={<i data-art="v" />} flip={flip} />)

  it('puts the words first in reading order and hides the picture from assistive tech', () => {
    const html = row(false)
    expect(html.indexOf('Network fabric')).toBeLessThan(html.indexOf('data-art="v"'))
    expect(html).toContain('aria-hidden="true"')
    expect(html).toContain('aria-label="Network fabric covers"')
  })

  it('flips the picture to the left on wide screens only by visual order, not by reading order', () => {
    const flipped = row(true)
    expect(flipped).toContain('data-flip="true"')
    expect(flipped).toContain('md:order-2')
    expect(flipped).toContain('md:order-1')
    expect(flipped.indexOf('Network fabric')).toBeLessThan(flipped.indexOf('data-art="v"'))
    expect(row(false)).not.toContain('md:order-1')
  })

  it('lets both columns shrink on a phone, so a wide picture cannot push the page sideways', () => {
    const html = row(true)
    expect((html.match(/min-w-0/g) ?? []).length).toBeGreaterThanOrEqual(2)
    expect(html).toContain('p-4 sm:p-6')
  })

  it('omits the chip list when there are no chips', () => {
    expect(wrap(<FeatureRow title="T" body="B" visual={<i />} />)).not.toContain('<ul')
  })
})

describe('MarketingHero and CtaBand', () => {
  it('renders the headline, the lede and the aurora', () => {
    const html = wrap(<MarketingHero title="Four ways in." lede="One daemon." />)
    expect(html).toContain('<h1')
    expect(html).toContain('Four ways in.')
    expect(html).toContain('zf-aurora')
  })
  it('links its buttons to the given routes', () => {
    const html = wrap(<CtaBand title="T" body="B" primary={{ to: '/sign-in', label: 'Sign in' }} secondary={{ to: '/platform', label: 'More' }} />)
    expect(html).toContain('href="/sign-in"')
    expect(html).toContain('href="/platform"')
    expect(wrap(<CtaBand title="T" body="B" primary={{ to: '/x', label: 'X' }} />)).not.toContain('zf-btn-secondary')
  })
})

describe('visuals', () => {
  it('use theme tokens only, so light and dark both work', () => {
    for (const v of [<NetworkVisual key="n" />, <StorageVisual key="s" />, <HubVisual key="h" />]) {
      expect(renderToStaticMarkup(v)).not.toMatch(/#[0-9a-fA-F]{3,6}/)
    }
  })
  it('say what they show in words where they show it', () => {
    expect(renderToStaticMarkup(<HubVisual />)).toContain('zyvor-')
    for (const label of ['Web', 'CLI', 'Operator', 'Terraform']) expect(renderToStaticMarkup(<HubVisual />)).toContain(`>${label}<`)
    expect(renderToStaticMarkup(<NetworkVisual />)).toContain('floating IP')
    expect(renderToStaticMarkup(<StorageVisual />)).toContain('replica')
    expect(renderToStaticMarkup(<MerionVisual />)).toContain('0 CONNECT, sealed')
  })
  it('moves only what is live: the network picture has a still link for the one stopped VM', () => {
    const html = renderToStaticMarkup(<NetworkVisual />)
    expect(html).toContain('zf-flow')
    expect(html).toContain('stroke-dasharray="6 8"')
  })
  it('the VM picture is the decorative fleet map', () => {
    const html = renderToStaticMarkup(<VmVisual />)
    expect(html).toContain('aria-hidden="true"')
    expect(html).not.toContain('role="img"')
  })
})
