// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { DensityPanel } from '../DensityPanel'
import { densityReport, type CapacityResponseLike } from '../../lib/capacity'
import fx from '../../lib/fixtures/system-capacity.json'

const f = fx as Record<string, CapacityResponseLike>
const render = (c: CapacityResponseLike) => renderToStaticMarkup(<DensityPanel report={densityReport(c)} />)

describe('DensityPanel', () => {
  it('shows no warnings for a healthy host', () => {
    const html = render(f.plain)
    expect(html).toContain('VM density and overcommit')
    expect(html).toContain('disabled')
    expect(html).not.toContain('density-warnings')
    expect(html).toContain('0.63:1')
  })
  it('warns when commit exceeds the configured limit', () => {
    const html = render(f.overcommitted)
    expect(html).toContain('density-warnings')
    expect(html).toContain('CPU is committed 3.75:1, above the configured 2.00:1 limit')
    expect(html).toContain('Over limit')
  })
  it('renders an empty host', () => {
    const html = render(f.empty)
    expect(html).not.toContain('density-warnings')
    expect(html).not.toContain('room for about')
  })
})
