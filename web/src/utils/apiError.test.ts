// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { describe, expect, it } from 'vitest'
import { formatHttpErrorBody, formatUserError, sanitizeErrorText } from './apiError'

describe('sanitizeErrorText', () => {
  it('strips HTML error pages', () => {
    const msg = sanitizeErrorText('<!DOCTYPE html><html><body>down</body></html>')
    expect(msg).toContain('HTML error page')
  })
})

describe('formatHttpErrorBody', () => {
  it('parses JSON error_code', () => {
    const msg = formatHttpErrorBody(
      500,
      'Internal Server Error',
      JSON.stringify({ error: 'vm start failed', error_code: 'operation_failed' }),
    )
    expect(msg).toContain('operation failed')
  })
})

describe('formatUserError', () => {
  it('handles Error instances', () => {
    expect(formatUserError(new Error('not found'))).toBe('not found')
  })
})
