// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/** Pure helpers behind the Merion session visuals, kept apart from the components so they can be tested without a DOM. */

export type ViewTone = 'neutral' | 'proof' | 'warn' | 'danger'

/** Which state an egress count is in. `unknown` when the runtime did not report one: never shown as sealed. */
export function egressState(connects: number | undefined | null): 'sealed' | 'leaked' | 'unknown' {
  if (typeof connects !== 'number' || !Number.isFinite(connects) || connects < 0) return 'unknown'
  return connects === 0 ? 'sealed' : 'leaked'
}

/** Drop reasons as bars: `[{reason, count}]` sorted by count, widest first. Anything else (the runtime sends `unknown`)
    gives no bars rather than guessing. */
export function dropBars(drops: unknown): Array<{ reason: string; count: number; share: number }> {
  if (!Array.isArray(drops)) return []
  const rows = drops
    .filter((d): d is { reason: string; count: number } => {
      const r = d as { reason?: unknown; count?: unknown }
      return !!d && typeof r.reason === 'string' && typeof r.count === 'number' && r.count > 0
    })
    .map((d) => ({ reason: d.reason, count: d.count }))
    .sort((a, b) => b.count - a.count)
  const max = rows[0]?.count ?? 0
  return rows.map((r) => ({ ...r, share: max > 0 ? r.count / max : 0 }))
}

/** The tone of one journal or report entry, from its type, phase or action text. Failure wins over everything else, so a
    "changeset.apply Failed" is never shown as a success because it also contains "apply". */
export function timelineTone(label: string): ViewTone {
  const l = label.toLowerCase()
  if (/(fail|error|refused|reached|expired|could not|cannot|can't|unable|not applied)/.test(l)) return 'danger'
  if (/(deny|denied|reject|paused|warn)/.test(l)) return 'warn'
  if (/(approved|applied|passed|performed|verified|sealed|created|completed)/.test(l)) return 'proof'
  return 'neutral'
}

/** A fill share (0 to 1) for a budget bar. A cap with no limit set has no bar. */
export function barShare(used: number, max: number | null): number | null {
  if (max === null || !Number.isFinite(max)) return null
  if (max <= 0) return 1
  return Math.max(0, Math.min(1, used / max))
}

export type RunStage = 'idle' | 'cell' | 'extract' | 'done' | 'fail'
export type NodeState = 'todo' | 'active' | 'done' | 'fail'

/** What each of the four pipeline nodes (file, cell, extract, artifact) shows for a run stage. The one that is working is
    `active`; on failure everything reached stays done and the node it stopped at is `fail`. */
export function pipelineStates(stage: RunStage): [NodeState, NodeState, NodeState, NodeState] {
  switch (stage) {
    case 'idle':
      return ['todo', 'todo', 'todo', 'todo']
    case 'cell':
      return ['done', 'active', 'todo', 'todo']
    case 'extract':
      return ['done', 'done', 'active', 'todo']
    case 'done':
      return ['done', 'done', 'done', 'done']
    case 'fail':
      // A failure can happen at any point and the page does not say where, so it is shown at the cell, the first thing that runs.
      return ['done', 'fail', 'todo', 'todo']
  }
}
