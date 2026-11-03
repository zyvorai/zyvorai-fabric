import { describe, expect, it } from 'vitest'
import { describePolicy, emptyForm, policyBody, type StoredPolicy } from './changesetPolicy'

describe('policyBody', () => {
  it('builds the body the runtime expects', () => {
    const r = policyBody('deploy-op', { ...emptyForm, paths: '/work, /data', maxFiles: '5', deny: '*.log' })
    expect(r).toEqual({
      ok: true,
      body: { id: 'deploy-op', paths: ['/work', '/data'], kinds: ['added'], deny_globs: ['*.log'], max_files: 5 },
    })
  })

  it('refuses what the runtime would refuse', () => {
    expect(policyBody('a', { ...emptyForm, paths: 'work' }).ok).toBe(false)
    expect(policyBody('a', { ...emptyForm, paths: '/work/..' }).ok).toBe(false)
    expect(policyBody('a', { ...emptyForm, kinds: [] }).ok).toBe(false)
    expect(policyBody('a', { ...emptyForm, maxBytes: '0' }).ok).toBe(false)
    expect(policyBody('a', { ...emptyForm, maxFiles: '1.5' }).ok).toBe(false)
    expect(policyBody(' ', emptyForm).ok).toBe(false)
  })
})

describe('describePolicy', () => {
  it('reads as a sentence', () => {
    const p: StoredPolicy = {
      user_id: 'u', agent: 'a', updated_at: '',
      policy: { id: 'a', version: 2, paths: ['/work'], kinds: ['added'], max_files: 20, max_bytes: null, deny_globs: [] },
    }
    expect(describePolicy(p)).toBe('added under /work; at most 20 files, any size')
  })
})
