// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/**
 * A `.merionpack.json` made by `fabric-agent pack bundle` / `merionctl bundle`.
 * `deploy_json` is the exact string that was signed; the console uploads the
 * file untouched so the signature still verifies. The signer seed never
 * reaches the browser.
 */
export interface MerionPackSummary {
  name: string
  signed: boolean
  hasPolicy: boolean
  policySigned: boolean
  hasGoal: boolean
}

export function inspectPack(text: string): { summary?: MerionPackSummary; error?: string } {
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch (e) {
    return { error: `Not valid JSON: ${e instanceof Error ? e.message : String(e)}` }
  }
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) {
    return { error: 'Expected a .merionpack.json file.' }
  }
  const o = raw as Record<string, unknown>
  if (o.format !== 'merionpack/1') {
    return { error: 'This is not a merionpack/1 file. Make one with: fabric-agent pack bundle <dir>' }
  }
  if (typeof o.name !== 'string' || typeof o.deploy_json !== 'string') {
    return { error: 'The pack is missing "name" or "deploy_json".' }
  }
  let inner: unknown
  try {
    inner = JSON.parse(o.deploy_json)
  } catch {
    return { error: 'deploy_json inside the pack is not valid JSON.' }
  }
  if ((inner as { name?: unknown } | null)?.name !== o.name) {
    return { error: 'The pack name does not match the name inside deploy_json.' }
  }
  const nonEmpty = (v: unknown) => typeof v === 'string' && v.length > 0
  return {
    summary: {
      name: o.name,
      signed: nonEmpty(o.signature),
      hasPolicy: nonEmpty(o.policy),
      policySigned: nonEmpty(o.policy_signature),
      hasGoal: typeof o.goal === 'object' && o.goal !== null,
    },
  }
}
