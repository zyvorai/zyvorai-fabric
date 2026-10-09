// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/** What a person reviews before approving a Keep `changeset` approval. */
export interface ChangesetSummary {
  command: string
  exitCode: number | null
  added: string[]
  modified: string[]
  deleted: string[]
  unstaged: string[]
  egress: string | null
  nonReplayable: string[]
}

const strings = (v: unknown): string[] =>
  Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : []

/**
 * Read a changeset approval's `planned_action` defensively: it comes from FluxVM through the
 * runtime, so every field may be missing. Returns null when it is not a changeset at all.
 */
export function summarizeChangeset(planned: unknown): ChangesetSummary | null {
  if (!planned || typeof planned !== 'object') return null
  const p = planned as Record<string, unknown>
  if (typeof p.changeset_id !== 'string') return null
  const changes = (p.changes && typeof p.changes === 'object' ? p.changes : {}) as Record<string, unknown>
  const effects = (p.side_effects && typeof p.side_effects === 'object' ? p.side_effects : {}) as Record<
    string,
    unknown
  >
  return {
    command: typeof p.command === 'string' ? p.command : '',
    exitCode: typeof p.exit_code === 'number' ? p.exit_code : null,
    added: strings(changes.added),
    modified: strings(changes.modified),
    deleted: strings(changes.deleted),
    unstaged: strings(p.unstaged),
    egress: typeof effects.egress === 'string' ? effects.egress : null,
    nonReplayable: strings(effects.non_replayable),
  }
}

export function changeCount(s: ChangesetSummary): number {
  return s.added.length + s.modified.length + s.deleted.length
}
