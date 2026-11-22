// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, expect, it } from 'vitest'
import { traceQuery, type Trace } from '../api/aiImprove'
import {
  autoScoreText,
  buildPolicy,
  formatCost,
  formatPValue,
  formatPercent,
  formatScore,
  lastUserMessage,
  meanLogprob,
  parseAutoScore,
  parseRouterJson,
  routerTemplate,
} from './aiImprove'

const base: Trace = {
  id: 't',
  created_unix: 0,
  endpoint: 'e',
  target_endpoint: 'e',
  model: 'm',
  path: '/v1/chat/completions',
  status: 200,
  latency_ms: 1,
  streamed: false,
  prompt_tokens: 1,
  completion_tokens: 1,
  request_sha256: '',
}

describe('formatting', () => {
  it('formats scores, percents, costs and p-values', () => {
    expect(formatScore(null)).toBe('—')
    expect(formatScore(0.6234)).toBe('0.62')
    expect(formatPercent(0.0125)).toBe('1.3%')
    expect(formatCost(0)).toBe('$0')
    expect(formatCost(0.000366)).toBe('$0.000366')
    expect(formatCost(12.5)).toBe('$12.50')
    expect(formatPValue(0.0001)).toBe('<0.001')
    expect(formatPValue(0.0456)).toBe('0.046')
  })
})

describe('auto score', () => {
  it('parses every scorer and rejects junk', () => {
    expect(parseAutoScore('')).toBeNull()
    expect(parseAutoScore('finish_stop')).toEqual({ kind: 'finish_stop' })
    expect(parseAutoScore('contains:ok')).toEqual({ kind: 'contains', needle: 'ok' })
    expect(parseAutoScore('contains:')).toBeUndefined()
    expect(parseAutoScore('bogus')).toBeUndefined()
  })
  it('round-trips to text', () => {
    expect(autoScoreText({ kind: 'contains', needle: 'x:y' })).toBe('contains:x:y')
    expect(autoScoreText(null)).toBe('')
  })
})

describe('buildPolicy', () => {
  const ok = { endpoint: 'e', sampleRate: '0.5', redact: true, tokenIds: false, autoScore: 'json_valid' }
  it('builds a policy', () => {
    expect(buildPolicy(ok)).toEqual({
      policy: {
        endpoint: 'e',
        sample_rate: 0.5,
        redact: true,
        capture_token_ids: false,
        auto_score: { kind: 'json_valid' },
      },
    })
  })
  it('rejects bad input', () => {
    expect(buildPolicy({ ...ok, endpoint: '' })).toHaveProperty('error')
    expect(buildPolicy({ ...ok, sampleRate: '2' })).toHaveProperty('error')
    expect(buildPolicy({ ...ok, sampleRate: 'abc' })).toHaveProperty('error')
    expect(buildPolicy({ ...ok, autoScore: 'nope' })).toHaveProperty('error')
  })
})

describe('parseRouterJson', () => {
  it('accepts the template', () => {
    const r = parseRouterJson(routerTemplate('front', ['x', 'y']))
    expect('router' in r && r.router.candidates.map((c) => c.endpoint)).toEqual(['x', 'y'])
  })
  it('explains what is wrong', () => {
    expect(parseRouterJson('{')).toHaveProperty('error')
    expect(parseRouterJson('[]')).toHaveProperty('error')
    expect(parseRouterJson('{"strategy":"cost","candidates":[{}]}')).toHaveProperty('error')
    expect(parseRouterJson('{"endpoint":"e","strategy":"nope","candidates":[{}]}')).toHaveProperty('error')
    expect(parseRouterJson('{"endpoint":"e","strategy":"cost","candidates":[]}')).toHaveProperty('error')
  })
})

describe('trace helpers', () => {
  it('averages logprobs', () => {
    expect(meanLogprob(base)).toBeNull()
    expect(meanLogprob({ ...base, logprobs: [-1, -3] })).toBe(-2)
  })
  it('finds the last user message in string and part form', () => {
    const msgs = [
      { role: 'user', content: 'first' },
      { role: 'assistant', content: 'a' },
      { role: 'user', content: [{ type: 'text', text: 'second' }] },
    ]
    expect(lastUserMessage({ ...base, request: { messages: msgs } })).toBe('second')
    expect(lastUserMessage(base)).toBe('')
  })
  it('builds query strings without empty values', () => {
    expect(traceQuery({})).toBe('')
    expect(traceQuery({ endpoint: 'a b', variant: '', scored: false, limit: 5 })).toBe(
      '?endpoint=a+b&scored=false&limit=5',
    )
  })
})
