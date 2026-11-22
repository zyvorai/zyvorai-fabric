// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import type { AiRouter, AutoScore, Trace, TracePolicy } from '../api/aiImprove'

export function formatScore(score: number | null | undefined): string {
  return score == null ? '—' : score.toFixed(2)
}

export function formatPercent(rate: number): string {
  return `${(rate * 100).toFixed(1)}%`
}

export function formatCost(usd: number): string {
  if (usd === 0) return '$0'
  return usd < 0.01 ? `$${usd.toFixed(6)}` : `$${usd.toFixed(2)}`
}

export function formatPValue(p: number): string {
  return p < 0.001 ? '<0.001' : p.toFixed(3)
}

/** "contains:foo" / "finish_stop" / "" -> AutoScore | null. Returns undefined when invalid. */
export function parseAutoScore(text: string): AutoScore | null | undefined {
  const t = text.trim()
  if (t === '') return null
  if (t === 'non_empty' || t === 'finish_stop' || t === 'json_valid') return { kind: t }
  if (t.startsWith('contains:') && t.length > 'contains:'.length) {
    return { kind: 'contains', needle: t.slice('contains:'.length) }
  }
  return undefined
}

export function autoScoreText(s: AutoScore | null | undefined): string {
  if (!s) return ''
  return s.kind === 'contains' ? `contains:${s.needle}` : s.kind
}

/** Validate the form values for a capture policy; returns an error message or the policy. */
export function buildPolicy(input: {
  endpoint: string
  sampleRate: string
  redact: boolean
  tokenIds: boolean
  autoScore: string
}): { error: string } | { policy: TracePolicy } {
  if (!input.endpoint) return { error: 'Choose an endpoint' }
  const rate = Number(input.sampleRate)
  if (!Number.isFinite(rate) || rate < 0 || rate > 1) {
    return { error: 'Sample rate must be between 0 and 1' }
  }
  const auto = parseAutoScore(input.autoScore)
  if (auto === undefined) {
    return { error: 'Scorer must be non_empty, finish_stop, json_valid or contains:<text>' }
  }
  return {
    policy: {
      endpoint: input.endpoint,
      sample_rate: rate,
      redact: input.redact,
      capture_token_ids: input.tokenIds,
      auto_score: auto,
    },
  }
}

export function parseRouterJson(text: string): { error: string } | { router: AiRouter } {
  let v: unknown
  try {
    v = JSON.parse(text)
  } catch (e) {
    return { error: `Not valid JSON: ${e instanceof Error ? e.message : String(e)}` }
  }
  const r = v as Partial<AiRouter> | null
  if (!r || typeof r !== 'object' || Array.isArray(r)) return { error: 'Router must be a JSON object' }
  if (!r.endpoint || typeof r.endpoint !== 'string') return { error: '"endpoint" is required' }
  if (r.strategy !== 'rules' && r.strategy !== 'cost' && r.strategy !== 'split') {
    return { error: '"strategy" must be rules, cost or split' }
  }
  if (!Array.isArray(r.candidates) || r.candidates.length === 0) {
    return { error: 'At least one candidate is required' }
  }
  return { router: r as AiRouter }
}

export function routerTemplate(endpoint: string, candidateEndpoints: string[]): string {
  const [a = 'endpoint-a', b = 'endpoint-b'] = candidateEndpoints
  return JSON.stringify(
    {
      endpoint,
      strategy: 'split',
      control: 'a',
      candidates: [
        { name: 'a', endpoint: a, weight: 1, quality: 0.8, cost_per_mtok_in: 1, cost_per_mtok_out: 3 },
        { name: 'b', endpoint: b, weight: 1, quality: 0.6, cost_per_mtok_in: 0.2, cost_per_mtok_out: 0.6 },
      ],
    },
    null,
    2,
  )
}

/** Mean logprob of the completion tokens, when the backend returned them. */
export function meanLogprob(t: Trace): number | null {
  const lp = t.logprobs
  if (!lp || lp.length === 0) return null
  return lp.reduce((a, b) => a + b, 0) / lp.length
}

export function lastUserMessage(t: Trace): string {
  const msgs = t.request?.messages ?? []
  for (let i = msgs.length - 1; i >= 0; i--) {
    if (msgs[i].role !== 'user') continue
    const c = msgs[i].content
    if (typeof c === 'string') return c
    if (Array.isArray(c)) {
      return c
        .map((p) => (p && typeof p === 'object' && 'text' in p ? String((p as { text: unknown }).text) : ''))
        .join('\n')
    }
  }
  return ''
}

export function traceTime(unix: number): string {
  return new Date(unix * 1000).toLocaleString()
}
