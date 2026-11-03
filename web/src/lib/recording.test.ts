// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest'
import { describeStep, offsetSeconds, playedSteps, type RecordingStep } from './recording'

const steps: RecordingStep[] = [
  { n: 0, source: 'journal', at: '2026-10-27T10:00:00Z', action: 'egress.http', phase: 'performed', detail: { host: 'api.example.com' } },
  { n: 1, source: 'event', at: '2026-10-27T10:00:05Z', kind: 'stdout', data_bytes: 42 },
  { n: 2, source: 'approval', at: '2026-10-27T10:01:00Z', kind: 'send', status: 'approved', decided_at: '2026-10-27T10:01:30Z' },
]

describe('recording', () => {
  it('describes each source without inventing content', () => {
    expect(describeStep(steps[0])).toBe('egress.http (performed) · api.example.com')
    expect(describeStep(steps[1])).toBe('stdout · 42 bytes, content not included')
    expect(describeStep(steps[2])).toBe('send approval: approved (decided)')
  })

  it('plays the run up to the scrubber and stays in range', () => {
    expect(playedSteps(steps, 0)).toHaveLength(1)
    expect(playedSteps(steps, 1)).toHaveLength(2)
    expect(playedSteps(steps, 99)).toHaveLength(3)
    expect(playedSteps(steps, -5)).toHaveLength(0)
  })

  it('labels a step by seconds from the start', () => {
    expect(offsetSeconds(steps, steps[2])).toBe(60)
    expect(offsetSeconds(steps, { ...steps[1], at: 'nonsense' })).toBe(0)
  })
})
