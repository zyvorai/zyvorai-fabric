// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/** The signed, masked recording a session exports (`GET /v1/sessions/{id}/recording`). */
export interface RecordingStep {
  n: number
  source: 'journal' | 'event' | 'approval'
  at: string
  action?: string
  phase?: string
  kind?: string
  status?: string
  decided_at?: string | null
  detail?: Record<string, unknown>
  masked_fields?: number
  data_bytes?: number
}

export interface SignedRecording {
  bundle: { format: string; session: { id: string; agent: string; status: string }; steps: RecordingStep[]; masking: string }
  digest: string
  signature: string
  signer: string
}

/** One plain line per step. Never shows masked content: only what the bundle carries. */
export function describeStep(s: RecordingStep): string {
  if (s.source === 'journal') {
    const host = typeof s.detail?.host === 'string' ? ` · ${s.detail.host}` : ''
    return `${s.action ?? 'journal'} (${s.phase ?? '?'})${host}`
  }
  if (s.source === 'event') return `${s.kind ?? 'event'}${s.data_bytes ? ` · ${s.data_bytes} bytes, content not included` : ''}`
  return `${s.kind ?? 'approval'} approval: ${s.status ?? 'unknown'}${s.decided_at ? ' (decided)' : ''}`
}

/** The steps up to and including `index`, so a scrubber shows the run as far as it has played. */
export function playedSteps(steps: RecordingStep[], index: number): RecordingStep[] {
  return steps.slice(0, Math.max(0, Math.min(index + 1, steps.length)))
}

/** Seconds from the first step to `s`, for the scrubber label; 0 when either time is unreadable. */
export function offsetSeconds(steps: RecordingStep[], s: RecordingStep): number {
  const a = Date.parse(steps[0]?.at ?? '')
  const b = Date.parse(s.at)
  return Number.isFinite(a) && Number.isFinite(b) ? Math.max(0, Math.round((b - a) / 1000)) : 0
}
