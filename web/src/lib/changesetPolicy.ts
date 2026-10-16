// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/** Operator-set auto-approval for one user's agent (docs/merion/design/auto-approve-changesets.md). */
export interface PolicyForm {
  paths: string
  kinds: string[]
  maxFiles: string
  maxBytes: string
  deny: string
}

export interface StoredPolicy {
  user_id: string
  agent: string
  updated_at: string
  policy: {
    id: string
    version: number
    paths: string[]
    kinds: string[]
    max_files: number
    max_bytes: number | null
    deny_globs: string[]
  }
}

export const POLICY_KINDS = ['added', 'modified', 'deleted'] as const

const list = (text: string): string[] =>
  text.split(',').map((x) => x.trim()).filter(Boolean)

/** The request body, or the first problem the operator should fix. The runtime validates again. */
export function policyBody(
  id: string,
  form: PolicyForm,
): { ok: true; body: Record<string, unknown> } | { ok: false; error: string } {
  const paths = list(form.paths)
  if (!id.trim()) return { ok: false, error: 'A policy needs an agent.' }
  if (paths.length === 0 || paths.some((p) => !p.startsWith('/') || p.includes('..'))) {
    return { ok: false, error: 'Paths must be absolute guest directories, for example /work.' }
  }
  if (form.kinds.length === 0) return { ok: false, error: 'Pick at least one change kind.' }
  const body: Record<string, unknown> = {
    id: id.trim(),
    paths,
    kinds: form.kinds,
    deny_globs: list(form.deny),
  }
  for (const [key, text] of [['max_files', form.maxFiles], ['max_bytes', form.maxBytes]] as const) {
    if (!text.trim()) continue
    const n = Number(text)
    if (!Number.isInteger(n) || n < 1) return { ok: false, error: `${key} must be a whole number above zero.` }
    body[key] = n
  }
  return { ok: true, body }
}

export function describePolicy(p: StoredPolicy): string {
  const size = p.policy.max_bytes == null ? 'any size' : `up to ${p.policy.max_bytes} bytes`
  return `${p.policy.kinds.join(', ')} under ${p.policy.paths.join(', ')}; at most ${p.policy.max_files} files, ${size}`
}

export const emptyForm: PolicyForm = { paths: '/work', kinds: ['added'], maxFiles: '', maxBytes: '', deny: '' }
