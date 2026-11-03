// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { changeCount, changeShares, formatRemaining, secondsUntilExpiry, summarizeChangeset } from '../../lib/changeset'

const KINDS = [
  { key: 'added', sign: '+', word: 'added', color: 'var(--zf-proof)', text: 'var(--zf-proof-text)' },
  { key: 'modified', sign: '~', word: 'modified', color: 'var(--zf-warning)', text: 'var(--zf-warn-text)' },
  { key: 'deleted', sign: '−', word: 'deleted', color: 'var(--zf-danger)', text: 'var(--zf-danger-text)' },
] as const

/** What a person reviews before approving a changeset: the command, a proportional change bar with counts, the files
    by kind, what can and cannot be replayed, and the expiry. File contents stay masked on the host, and the card says so. */
export function ChangesetDiff({ planned }: { planned: unknown }) {
  const cs = summarizeChangeset(planned)
  if (!cs) return null
  const left = secondsUntilExpiry(cs, Date.now())
  const shares = changeShares(cs)
  const total = changeCount(cs)
  return (
    <div className="mt-3 space-y-3 text-xs" data-testid="changeset-review">
      <div className="rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-canvas)] px-3 py-2 font-mono text-[var(--zf-secondary)]">
        <span className="text-[var(--zf-brand-text)]">$</span> {cs.command || '(command not shown)'}
        {cs.exitCode !== null && (
          <span className={cs.exitCode === 0 ? 'text-[var(--zf-proof-text)]' : 'text-[var(--zf-danger-text)]'}>
            {' '}· exit {cs.exitCode}
          </span>
        )}
      </div>

      {total === 0 ? (
        <p className="text-[var(--zf-muted)]">No file changes.</p>
      ) : (
        <div>
          <div className="flex h-2 overflow-hidden rounded-full bg-[var(--zf-hairline)]" role="img" aria-label={`${cs.added.length} added, ${cs.modified.length} modified, ${cs.deleted.length} deleted`}>
            {KINDS.map((k) => (
              <span key={k.key} style={{ width: `${shares[k.key] * 100}%`, background: k.color }} />
            ))}
          </div>
          <div className="mt-1.5 flex gap-4 text-[var(--zf-secondary)]">
            {KINDS.map((k) => (
              <span key={k.key} className="tabular-nums">
                <span style={{ color: k.text }} className="font-semibold">{k.sign}{cs[k.key].length}</span> {k.word}
              </span>
            ))}
          </div>
          <ul className="mt-2 space-y-1 font-mono">
            {KINDS.flatMap((k) =>
              cs[k.key].map((p) => (
                <li key={k.key + p} className="flex items-center gap-2">
                  <span className="w-4 shrink-0 text-center font-semibold" style={{ color: k.text }} aria-label={k.word}>{k.sign}</span>
                  <span className="truncate text-[var(--zf-ink)]" title={p}>{p}</span>
                </li>
              )),
            )}
          </ul>
          <p className="mt-1 text-[var(--zf-muted)]">File contents are masked on the host, so only names and counts are shown.</p>
        </div>
      )}

      {cs.unstaged.length > 0 && (
        <p className="text-[var(--zf-warn-text)]">Not captured, so not applied: {cs.unstaged.join(', ')}</p>
      )}

      {(cs.replayable.length > 0 || cs.nonReplayable.length > 0) && (
        <div className="grid gap-2 sm:grid-cols-2" data-testid="changeset-risk">
          <div className="rounded-lg border border-[var(--zf-hairline)] p-2">
            <div className="font-semibold text-[var(--zf-proof-text)]">Replayed on approval</div>
            <ul className="mt-1 space-y-0.5 text-[var(--zf-secondary)]">
              {cs.replayable.length ? cs.replayable.map((r) => <li key={r}>{r}</li>) : <li>Nothing.</li>}
            </ul>
          </div>
          <div className="rounded-lg border border-[var(--zf-hairline)] p-2">
            <div className="font-semibold text-[var(--zf-warn-text)]">Recorded, never replayed</div>
            <ul className="mt-1 space-y-0.5 text-[var(--zf-secondary)]">
              {cs.nonReplayable.length ? cs.nonReplayable.map((r) => <li key={r}>{r}</li>) : <li>Nothing.</li>}
            </ul>
          </div>
        </div>
      )}

      <p className="text-[var(--zf-muted)]">
        Nothing reaches the sandbox until you approve.{cs.egress ? ` Network: ${cs.egress}.` : ''}
      </p>
      {left !== null && (
        <p className={left <= 0 ? 'text-[var(--zf-danger-text)]' : 'text-[var(--zf-muted)]'} data-testid="changeset-expiry">
          {left <= 0 ? 'Expired: approving will not apply it. Speculate again.' : `Expires: ${formatRemaining(left)}`}
        </p>
      )}
    </div>
  )
}
