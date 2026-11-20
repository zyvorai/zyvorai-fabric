// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { describe, expect, it } from 'vitest'
import {
  draftToSpec,
  emptyDraft,
  MAX_RULES,
  parseSpecJson,
  slugify,
  validateSpec,
} from './useCaseSpec'

describe('useCaseSpec', () => {
  it('slugifies titles into server-valid ids', () => {
    expect(slugify('Invoice check!')).toBe('invoice-check')
    expect(slugify('  --A  B--  ')).toBe('a-b')
    expect(slugify('x'.repeat(80)).length).toBeLessThanOrEqual(40)
    expect(slugify('***')).toBe('')
  })

  it('maps a form to the spec agent-runtime stores', () => {
    const d = emptyDraft()
    d.title = 'Invoice check'
    d.description = 'Totals'
    d.accepts = '.TXT, log'
    d.rules = [
      { kind: 'keyword_sections', title: 'Totals', items: 'total, amount due\nbalance', count: '3' },
      { kind: 'top_repeated_lines', title: 'Repeats', items: '', count: '' },
      { kind: 'stats', title: '', items: '', count: '' },
      { kind: 'csv_columns', title: 'Regions', items: 'region', count: '2' },
    ]
    d.sampleText = 'Total: 10'
    const spec = draftToSpec(d)
    expect(spec).toEqual({
      id: 'invoice-check',
      title: 'Invoice check',
      description: 'Totals',
      accepts: ['txt', 'log'],
      extract: 'text',
      summary: [
        { kind: 'keyword_sections', title: 'Totals', keywords: ['total', 'amount due', 'balance'], max_lines: 3 },
        { kind: 'top_repeated_lines', title: 'Repeats' },
        { kind: 'stats' },
        { kind: 'csv_columns', title: 'Regions', columns: ['region'], top: 2 },
      ],
      sample: { filename: 'sample.txt', text: 'Total: 10' },
    })
    expect(validateSpec(spec)).toEqual([])
  })

  it('forces pdf for the PDF extractor and never attaches a sample', () => {
    const d = emptyDraft()
    d.title = 'Contract'
    d.extract = 'pdftotext'
    d.accepts = 'txt'
    d.sampleText = 'ignored'
    d.rules[0].items = 'term'
    const spec = draftToSpec(d)
    expect(spec.accepts).toEqual(['pdf'])
    expect(spec.sample).toBeUndefined()
    expect(validateSpec(spec)).toEqual([])
  })

  it('reports mistakes before the request is sent', () => {
    const d = emptyDraft()
    expect(validateSpec(draftToSpec(d))).toEqual(
      expect.arrayContaining(['Give the use case a name.', 'Rule 1: add at least one keyword.']),
    )
    d.title = 'Bad'
    d.accepts = 'pdf'
    d.rules = []
    const errs = validateSpec(draftToSpec(d))
    expect(errs).toContain('PDF files need the PDF extractor.')
    expect(errs).toContain('Add at least one summary rule.')
    d.rules = Array.from({ length: MAX_RULES + 1 }, () => ({
      kind: 'stats' as const, title: '', items: '', count: '',
    }))
    expect(validateSpec(draftToSpec(d))).toContain(`At most ${MAX_RULES} rules.`)
  })

  it('parses pasted JSON and explains failures', () => {
    expect(parseSpecJson('{').error).toMatch(/Not valid JSON/)
    expect(parseSpecJson('[]').error).toMatch(/JSON object/)
    expect(parseSpecJson('{"id":"x"}').error).toMatch(/"id" and "title"/)
    expect(parseSpecJson('{"id":"x","title":"X"}').error).toMatch(/"accepts"/)
    const ok = parseSpecJson(
      '{"id":"x","title":"X","accepts":["txt"],"extract":"text","summary":[{"kind":"stats"}]}',
    )
    expect(ok.spec?.id).toBe('x')
  })

  it('fixes the extensions for each fixed-format extractor', () => {
    const d = emptyDraft()
    d.title = 'Sheets'
    for (const [extract, accepts] of [
      ['docx', ['docx']],
      ['xlsx', ['xlsx']],
      ['pptx', ['pptx']],
      ['ocr', ['png', 'jpg', 'jpeg', 'tif', 'tiff']],
      ['html', ['html', 'htm']],
      ['eml', ['eml', 'mbox']],
    ] as const) {
      d.extract = extract
      d.accepts = 'txt'
      expect(draftToSpec(d).accepts).toEqual(accepts)
    }
  })

  it('flags an extension the fixed extractor cannot read', () => {
    const spec = parseSpecJson(
      '{"id":"x","title":"X","accepts":["csv"],"extract":"xlsx","summary":[{"kind":"stats"}]}',
    ).spec!
    expect(validateSpec(spec).join(' ')).toContain('reads only .xlsx, not .csv')
  })
})
