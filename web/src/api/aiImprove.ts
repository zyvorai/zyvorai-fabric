// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { apiDelete, apiFetch, apiGet, apiPost } from './client'

export type AutoScore =
  | { kind: 'non_empty' }
  | { kind: 'finish_stop' }
  | { kind: 'json_valid' }
  | { kind: 'contains'; needle: string }

export interface TracePolicy {
  endpoint: string
  enabled?: boolean
  sample_rate: number
  max_body_bytes?: number
  redact: boolean
  capture_token_ids: boolean
  auto_score?: AutoScore | null
  max_traces?: number
}

export interface Trace {
  id: string
  created_unix: number
  endpoint: string
  router?: string
  variant?: string
  target_endpoint: string
  model: string
  path: string
  status: number
  latency_ms: number
  streamed: boolean
  user?: string
  prompt_tokens: number
  completion_tokens: number
  finish_reason?: string
  request?: { messages?: { role: string; content: unknown }[] } & Record<string, unknown>
  completion?: string
  prompt_token_ids?: number[]
  completion_token_ids?: number[]
  logprobs?: number[]
  request_sha256: string
  redacted?: boolean
  score?: number
  score_source?: string
  labels?: string[]
}

export type RouteStrategy = 'rules' | 'cost' | 'split'

export interface RouterCandidate {
  name: string
  endpoint: string
  weight?: number
  cost_per_mtok_in?: number
  cost_per_mtok_out?: number
  quality?: number
  max_context?: number | null
}

export interface AiRouter {
  endpoint: string
  strategy: RouteStrategy
  candidates: RouterCandidate[]
  rules?: unknown[]
  default?: string | null
  control?: string | null
  sticky_header?: string
  min_quality?: number
  hard_quality?: number | null
  hard_prompt_tokens?: number
  allow_override?: boolean
  enabled?: boolean
}

export interface Comparison {
  delta: number
  z: number
  p_value: number
  significant: boolean
}

export interface VariantStats {
  variant: string
  requests: number
  errors: number
  error_rate: number
  scored: number
  mean_score: number | null
  latency_p50_ms: number
  latency_p95_ms: number
  prompt_tokens: number
  completion_tokens: number
  est_cost_usd: number
  vs_control?: Comparison
}

export interface RouterReport {
  router: string
  control: string
  min_samples: number
  variants: VariantStats[]
  recommendation: string
}

export interface RouteDecision {
  chosen: string
  order: string[]
  reason: string
  prompt_tokens_estimate: number
  hard: boolean
}

export interface TraceFilter {
  endpoint?: string
  router?: string
  variant?: string
  scored?: boolean
  min_score?: number
  limit?: number
  format?: 'raw' | 'sft'
}

export function traceQuery(f: TraceFilter): string {
  const p = new URLSearchParams()
  for (const [k, v] of Object.entries(f)) {
    if (v !== undefined && v !== '') p.set(k, String(v))
  }
  const s = p.toString()
  return s ? `?${s}` : ''
}

const enc = encodeURIComponent

export const listTraces = (f: TraceFilter = {}) => apiGet<Trace[]>(`/api/ai/traces${traceQuery(f)}`)
export const scoreTrace = (id: string, score: number, labels: string[] = []) =>
  apiPost<Trace>(`/api/ai/traces/${enc(id)}/feedback`, { score, labels })

/** Fetch an export and return its text (newline-delimited JSON). */
export async function exportTraces(f: TraceFilter): Promise<string> {
  const res = await apiFetch(`/api/ai/traces/export${traceQuery(f)}`)
  if (!res.ok) throw new Error(`Export failed (${res.status})`)
  return res.text()
}

export const listTracePolicies = () => apiGet<TracePolicy[]>('/api/ai/trace-policies')
export const saveTracePolicy = (p: TracePolicy) => apiPost<TracePolicy>('/api/ai/trace-policies', p)
export const deleteTracePolicy = (endpoint: string) =>
  apiDelete(`/api/ai/trace-policies/${enc(endpoint)}`)

export const listRouters = () => apiGet<AiRouter[]>('/api/ai/routers')
export const saveRouter = (r: AiRouter) => apiPost<AiRouter>('/api/ai/routers', r)
export const deleteRouter = (endpoint: string) => apiDelete(`/api/ai/routers/${enc(endpoint)}`)
export const routerReport = (endpoint: string, minSamples?: number) =>
  apiGet<RouterReport>(
    `/api/ai/routers/${enc(endpoint)}/report${minSamples ? `?min_samples=${minSamples}` : ''}`,
  )
export const promoteVariant = (endpoint: string, variant: string) =>
  apiPost<AiRouter>(`/api/ai/routers/${enc(endpoint)}/promote`, { variant })
export const decideRoute = (endpoint: string, body: unknown) =>
  apiPost<RouteDecision>(`/api/ai/routers/${enc(endpoint)}/decide`, body)
