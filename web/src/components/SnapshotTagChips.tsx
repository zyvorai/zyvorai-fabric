// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { Lock } from 'lucide-react'
import { hasKeep, KEEP_TAG } from '../lib/snapshotPolicy'

export interface SnapshotTagChipsProps {
  tags?: string[] | null
  /** Called to toggle `keep`; omit for a read-only view. */
  onToggleKeep?: () => void
  busy?: boolean
}

/** Tag chips for a snapshot plus a keep toggle (keep = never pruned by a policy). */
export function SnapshotTagChips({ tags, onToggleKeep, busy }: SnapshotTagChipsProps) {
  const keep = hasKeep(tags)
  const others = (tags ?? []).filter((t) => t !== KEEP_TAG)
  return (
    <div className="flex flex-wrap items-center gap-1 mt-1" data-testid="snapshot-tags">
      {keep && (
        <span className="px-1.5 py-0.5 text-[11px] rounded border border-emerald-200 bg-emerald-50 text-emerald-700">
          keep
        </span>
      )}
      {others.map((t) => (
        <span key={t} className="px-1.5 py-0.5 text-[11px] rounded border border-[var(--zf-hairline)] text-[var(--zf-muted)]">
          {t}
        </span>
      ))}
      {onToggleKeep && (
        <button
          type="button"
          onClick={onToggleKeep}
          disabled={busy}
          aria-pressed={keep}
          title={keep ? 'Allow snapshot policies to prune this snapshot' : 'Protect this snapshot from snapshot-policy pruning'}
          className="inline-flex items-center gap-1 px-1.5 py-0.5 text-[11px] rounded border border-[var(--zf-hairline)] text-[var(--zf-muted)] hover:text-[var(--zf-ink)] disabled:opacity-50"
        >
          <Lock className="w-3 h-3" />
          {keep ? 'Unkeep' : 'Keep'}
        </button>
      )}
    </div>
  )
}
