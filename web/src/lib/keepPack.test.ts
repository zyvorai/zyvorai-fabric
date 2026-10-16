// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest'
import { inspectPack } from './keepPack'

const pack = (over: Record<string, unknown> = {}) =>
  JSON.stringify({
    format: 'merionpack/1',
    name: 'my-agent',
    deploy_json: JSON.stringify({ name: 'my-agent', bundle_base64: 'AA==' }),
    signature: 'ab',
    policy: 'version: 1\n',
    policy_signature: 'cd',
    goal: { title: 'x', description: 'y' },
    ...over,
  })

describe('inspectPack', () => {
  it('summarises a signed pack', () => {
    expect(inspectPack(pack()).summary).toEqual({
      name: 'my-agent',
      signed: true,
      hasPolicy: true,
      policySigned: true,
      hasGoal: true,
    })
  })

  it('reports an unsigned pack without a policy', () => {
    const { summary } = inspectPack(pack({ signature: null, policy: null, policy_signature: null, goal: null }))
    expect(summary).toMatchObject({ signed: false, hasPolicy: false, policySigned: false, hasGoal: false })
  })

  it('explains what is wrong', () => {
    expect(inspectPack('{').error).toMatch(/Not valid JSON/)
    expect(inspectPack('[]').error).toMatch(/merionpack/)
    expect(inspectPack('{"format":"x"}').error).toMatch(/not a merionpack\/1/)
    expect(inspectPack(pack({ deploy_json: undefined })).error).toMatch(/missing/)
    expect(inspectPack(pack({ deploy_json: '{' })).error).toMatch(/deploy_json/)
    expect(inspectPack(pack({ name: 'other' })).error).toMatch(/does not match/)
  })
})
