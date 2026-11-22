// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { PreflightView, type PreflightViewProps } from '../PreflightView'
import type { PreflightResult } from '../../lib/preflight'

const result = (findings: PreflightResult['findings']): PreflightResult => ({
  vm: 'web-1',
  backend: 'fluxvm',
  can_start: !findings.some((f) => f.severity === 'blocker'),
  summary: { blockers: 0, warnings: 0, infos: 0 },
  findings,
})

const base: PreflightViewProps = { result: null, loading: false, error: null, onRun: () => {} }
const render = (p: Partial<PreflightViewProps>) => renderToStaticMarkup(<PreflightView {...base} {...p} />)

describe('PreflightView', () => {
  it('offers to run the check before any result exists, with no verdict', () => {
    const html = render({})
    expect(html).toContain('Pre-flight check')
    expect(html).toContain('Run pre-flight check')
    expect(html).not.toContain('preflight-verdict')
  })

  it('shows a ready verdict for a clean result', () => {
    const html = render({ result: result([]) })
    expect(html).toContain('Ready to start: no problems found')
    expect(html).toContain('Run again')
  })

  it('lists blockers before warnings with message and hint text', () => {
    const html = render({
      result: result([
        { id: 'w', severity: 'warning', message: 'Quota nearly full', hint: 'Raise the quota.' },
        { id: 'backend.tpm', severity: 'blocker', message: 'No TPM here', hint: 'Remove the TPM option.' },
      ]),
    })
    expect(html).toContain('Cannot start: 1 blocker, 1 warning')
    expect(html.indexOf('No TPM here')).toBeLessThan(html.indexOf('Quota nearly full'))
    expect(html).toContain('Remove the TPM option.')
    expect(html).toContain('Blockers (1)')
    expect(html).toContain('Warnings (1)')
  })

  it('disables the button while loading and surfaces errors', () => {
    const html = render({ loading: true, error: 'Network down' })
    expect(html).toContain('Checking…')
    expect(html).toContain('disabled')
    expect(html).toContain('role="alert"')
    expect(html).toContain('Network down')
  })
})
