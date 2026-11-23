// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import {
  goodLabel,
  riskText,
  type FieldChange,
  type RevisionSummary,
  type SinceGood,
  type VmDiff,
  type VmHistory,
} from '../api/configHistory'

export function RiskBadge({ risk }: { risk?: FieldChange['risk'] }) {
  if (!risk) return null
  const tone =
    risk.level === 'high'
      ? 'bg-red-100 text-red-900'
      : risk.level === 'medium'
        ? 'bg-amber-100 text-amber-900'
        : 'bg-slate-100 text-slate-800'
  return <span className={`mr-2 rounded px-2 py-0.5 text-xs font-medium ${tone}`}>{riskText(risk.level)}</span>
}

export function ChangeList({ changes }: { changes: FieldChange[] }) {
  if (changes.length === 0) return <p className="text-sm">No configuration differences.</p>
  return (
    <ul className="space-y-2" aria-label="Changed fields">
      {changes.map((c) => (
        <li key={c.path} className="rounded border border-[var(--zf-hairline)] px-3 py-2 text-sm">
          <RiskBadge risk={c.risk} />
          <span>{c.summary}</span>
          {c.risk && <p className="mt-1 text-[var(--zf-muted)]">Why it matters: {c.risk.reason}</p>}
          {c.hidden && <p className="mt-1 text-[var(--zf-muted)]">The value is not stored; only the fact that it changed.</p>}
        </li>
      ))}
    </ul>
  )
}

export function DiffView({ diff }: { diff: VmDiff }) {
  return (
    <section aria-label="Revision diff">
      <p className="mb-2 text-sm">
        {diff.summary} (revision {diff.from.rev} by {diff.from.actor}, revision {diff.to.rev} by {diff.to.actor})
      </p>
      <ChangeList changes={diff.changes} />
    </section>
  )
}

export function SinceGoodView({ data }: { data: SinceGood }) {
  if (data.status === 'no_known_good') {
    return (
      <section aria-label="Since last known good">
        <p className="text-sm">{data.message}</p>
      </section>
    )
  }
  if (data.status === 'unchanged') {
    return (
      <section aria-label="Since last known good">
        <p className="text-sm">{data.summary}</p>
      </section>
    )
  }
  return (
    <section aria-label="Since last known good">
      <p className="mb-3 text-sm">{data.summary}</p>
      <ol className="space-y-2" aria-label="Likely culprits, most likely first">
        {data.culprits.map((c) => (
          <li key={c.path} className="rounded border border-[var(--zf-hairline)] px-3 py-2 text-sm">
            <span className="mr-2 font-semibold">{c.rank}.</span>
            <RiskBadge risk={c.risk} />
            <span>{c.summary}</span>
            {c.risk && <p className="mt-1 text-[var(--zf-muted)]">Why it matters: {c.risk.reason}</p>}
            {c.changed_in.map((w) => (
              <p key={w.rev} className="mt-1 text-[var(--zf-muted)]">
                Revision {w.rev} by {w.actor} at {w.at} ({w.cause})
              </p>
            ))}
          </li>
        ))}
      </ol>
      <p className="mt-3 text-sm text-[var(--zf-muted)]">{data.note}</p>
    </section>
  )
}

export function RevisionTable({
  history,
  selected,
  onToggle,
  onMarkGood,
  canWrite,
}: {
  history: VmHistory
  selected: number[]
  onToggle: (rev: number) => void
  onMarkGood: (rev: RevisionSummary) => void
  canWrite: boolean
}) {
  return (
    <table className="w-full text-sm" aria-label="Configuration revisions">
      <thead>
        <tr className="text-left text-[var(--zf-muted)]">
          <th className="py-2 pr-3">Compare</th>
          <th className="py-2 pr-3">Revision</th>
          <th className="py-2 pr-3">When</th>
          <th className="py-2 pr-3">Who</th>
          <th className="py-2 pr-3">Cause</th>
          <th className="py-2 pr-3">Changes</th>
          <th className="py-2 pr-3">Known good</th>
          <th className="py-2">Audit link</th>
        </tr>
      </thead>
      <tbody>
        {history.revisions.map((r) => (
          <tr key={r.rev}>
            <td className="py-2 pr-3">
              <input
                type="checkbox"
                aria-label={`Compare revision ${r.rev}`}
                checked={selected.includes(r.rev)}
                onChange={() => onToggle(r.rev)}
              />
            </td>
            <td className="py-2 pr-3">{r.rev}</td>
            <td className="py-2 pr-3">{r.timestamp}</td>
            <td className="py-2 pr-3">{r.actor}</td>
            <td className="py-2 pr-3">{r.cause}</td>
            <td className="py-2 pr-3">
              {r.changed_fields}
              {r.high_risk_changes > 0 ? `, ${r.high_risk_changes} high risk` : ''}
            </td>
            <td className="py-2 pr-3">
              {r.good ? (
                goodLabel(r.good)
              ) : canWrite ? (
                <button type="button" className="btn-secondary" onClick={() => onMarkGood(r)}>
                  {`Mark revision ${r.rev} good`}
                </button>
              ) : (
                'Not marked'
              )}
            </td>
            <td className="py-2">{r.audit.link}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
