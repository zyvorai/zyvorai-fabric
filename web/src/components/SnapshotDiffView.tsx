// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import type { ConfigDiff, ConfigSnapshotMeta } from '../api/configSnapshots'
import { formatValue, groupChanges, isRedacted, sectionLabel, summarizeChanges } from '../lib/snapshotDiff'

export interface SnapshotDiffViewProps {
  snapshots: ConfigSnapshotMeta[]
  max: number
  from: string
  to: string
  onFromChange: (v: string) => void
  onToChange: (v: string) => void
  onCreate: () => void
  creating: boolean
  diff: ConfigDiff | null
  diffLoading: boolean
  error: string | null
}

const KIND_STYLE = {
  added: 'text-emerald-700 bg-emerald-50 border-emerald-200',
  removed: 'text-red-700 bg-red-50 border-red-200',
  changed: 'text-amber-800 bg-amber-50 border-amber-200',
} as const

function SnapshotSelect({
  label,
  value,
  onChange,
  snapshots,
}: {
  label: string
  value: string
  onChange: (v: string) => void
  snapshots: ConfigSnapshotMeta[]
}) {
  return (
    <label className="flex items-center gap-2 text-xs text-[var(--zf-muted)]">
      {label}
      <select
        aria-label={label}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="px-2 py-1.5 rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-canvas)] text-[var(--zf-ink)]"
      >
        <option value="current">Current configuration</option>
        {snapshots.map((s) => (
          <option key={s.id} value={s.id}>
            {new Date(s.created_at).toLocaleString()} ({s.id})
          </option>
        ))}
      </select>
    </label>
  )
}

export function SnapshotDiffView(p: SnapshotDiffViewProps) {
  const groups = p.diff ? groupChanges(p.diff.changes) : []
  const summary = p.diff ? summarizeChanges(p.diff.changes) : null

  return (
    <div className="space-y-4" data-testid="snapshot-diff-view">
      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={p.onCreate}
          disabled={p.creating}
          className="px-3 py-1.5 text-xs font-medium rounded-lg bg-[var(--zf-ink)] text-[var(--zf-canvas)] disabled:opacity-50"
        >
          {p.creating ? 'Saving…' : 'Take snapshot'}
        </button>
        <span className="text-xs text-[var(--zf-muted)]">
          {p.snapshots.length} of {p.max} kept
        </span>
      </div>

      {p.error && (
        <div role="alert" className="bg-red-50 rounded-lg border border-red-200 px-4 py-2 text-xs text-red-700">
          {p.error}
        </div>
      )}

      {p.snapshots.length === 0 ? (
        <p className="text-sm text-[var(--zf-muted)]">
          No snapshots yet. Take one to record the current configuration, then compare it later.
        </p>
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-4">
            <SnapshotSelect label="From" value={p.from} onChange={p.onFromChange} snapshots={p.snapshots} />
            <SnapshotSelect label="To" value={p.to} onChange={p.onToChange} snapshots={p.snapshots} />
          </div>

          {p.diffLoading && <p className="text-xs text-[var(--zf-muted)]">Comparing…</p>}

          {summary && !p.diffLoading && (
            <div>
              <p className="text-sm text-[var(--zf-ink)]" data-testid="diff-summary">
                {summary.total === 0
                  ? 'No differences.'
                  : `${summary.added} added, ${summary.removed} removed, ${summary.changed} field changes`}
              </p>
              <div className="mt-3 space-y-3">
                {groups.map((g) => (
                  <div
                    key={`${g.section}/${g.key}`}
                    className="rounded-lg border border-[var(--zf-hairline)] p-3"
                  >
                    <div className="flex items-center gap-2">
                      <span className={`text-xs px-1.5 py-0.5 rounded border ${KIND_STYLE[g.kind]}`}>{g.kind}</span>
                      <span className="text-xs text-[var(--zf-muted)]">{sectionLabel(g.section)}</span>
                      <span className="text-sm font-medium text-[var(--zf-ink)]">{g.key}</span>
                    </div>
                    {g.kind === 'changed' && (
                      <ul className="mt-2 space-y-1">
                        {g.changes.map((c) => (
                          <li key={c.field} className="text-xs font-mono text-[var(--zf-ink)]">
                            {c.field}:{' '}
                            <span className={isRedacted(c.old) ? 'italic text-[var(--zf-muted)]' : ''}>
                              {formatValue(c.old)}
                            </span>
                            {' → '}
                            <span className={isRedacted(c.new) ? 'italic text-[var(--zf-muted)]' : ''}>
                              {formatValue(c.new)}
                            </span>
                          </li>
                        ))}
                      </ul>
                    )}
                  </div>
                ))}
              </div>
            </div>
          )}
        </>
      )}
    </div>
  )
}
