// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { WhatIfView, type WhatIfViewProps } from '../WhatIfView'
import { EMPTY_ROW, fitVms, NO_OVERCOMMIT } from '../../lib/capacity'

const noop = () => {}
const base: WhatIfViewProps = {
  rows: [{ ...EMPTY_ROW, name: 'web' }],
  report: null,
  source: null,
  loading: false,
  error: null,
  onRowChange: noop,
  onAddRow: noop,
  onRemoveRow: noop,
  onRun: noop,
}
const render = (p: Partial<WhatIfViewProps>) => renderToStaticMarkup(<WhatIfView {...base} {...p} />)

describe('WhatIfView', () => {
  it('shows the form and no report before simulating', () => {
    const html = render({})
    expect(html).toContain('What if I add...')
    expect(html).toContain('Simulate')
    expect(html).toContain('Nothing is created or reserved')
    expect(html).not.toContain('whatif-report')
    expect(html).not.toContain('Remove')
  })

  it('offers row removal only with more than one row', () => {
    expect(render({ rows: [base.rows[0], base.rows[0]] })).toContain('Remove row 2')
  })

  it('renders a fitting report with headroom', () => {
    const report = fitVms(
      [{ name: 'h1', vcpus: 8, memory_mb: 8192, disk_gb: 100 }],
      [],
      [{ name: 'web', vcpus: 2, memory_mb: 2048, disk_gb: 10, count: 2 }],
      NO_OVERCOMMIT,
    )
    const html = render({ report, source: 'api' })
    expect(html).toContain('All 2 VMs fit')
    expect(html).toContain('4 of 8')
    expect(html).toContain('Calculated by the daemon')
    expect(html).not.toContain('whatif-unplaced')
  })

  it('lists VMs that do not fit with reasons, and flags local calculation', () => {
    const report = fitVms(
      [{ name: 'h1', vcpus: 2, memory_mb: 8192, disk_gb: null }],
      [],
      [{ name: 'big', vcpus: 4, memory_mb: 1024, disk_gb: 10, count: 1 }],
      NO_OVERCOMMIT,
    )
    const html = render({ report, source: 'local' })
    expect(html).toContain('0 of 1 VMs fit')
    expect(html).toContain('big #1: not enough vCPUs')
    expect(html).toContain('unknown')
    expect(html).toContain('in the browser')
  })

  it('shows errors and a busy button', () => {
    const html = render({ error: 'Row 1: enter a name', loading: true })
    expect(html).toContain('role="alert"')
    expect(html).toContain('Row 1: enter a name')
    expect(html).toContain('Simulating')
    expect(html).toContain('disabled')
  })
})
