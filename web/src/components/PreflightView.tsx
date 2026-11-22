// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { groupFindings, preflightVerdict, type PreflightResult, type PreflightSeverity, type PreflightTone } from '../lib/preflight'

export interface PreflightViewProps {
  result: PreflightResult | null
  loading: boolean
  error: string | null
  onRun: () => void
}

const TONE_STYLE: Record<PreflightTone, string> = {
  ok: 'text-emerald-700 bg-emerald-50 border-emerald-200',
  warn: 'text-amber-800 bg-amber-50 border-amber-200',
  blocked: 'text-red-700 bg-red-50 border-red-200',
}

const SEVERITY_STYLE: Record<PreflightSeverity, string> = {
  blocker: 'border-red-200 bg-red-50',
  warning: 'border-amber-200 bg-amber-50',
  info: 'border-[var(--zf-hairline)] bg-black/[0.02]',
}

/** Presentational pre-flight panel: pure of data fetching so it renders statically. */
export function PreflightView({ result, loading, error, onRun }: PreflightViewProps) {
  const verdict = result ? preflightVerdict(result.findings) : null
  const groups = result ? groupFindings(result.findings) : []

  return (
    <div className="zf-panel p-5 md:col-span-2" data-testid="preflight-panel">
      <div className="flex flex-wrap items-center justify-between gap-3 mb-3">
        <div>
          <h3 className="text-sm font-medium text-[var(--zf-muted)]">Pre-flight check</h3>
          <p className="text-xs text-[var(--zf-muted)]">Checks whether this VM can start here. Nothing is started or changed.</p>
        </div>
        <button
          type="button"
          onClick={onRun}
          disabled={loading}
          className="px-3 py-1.5 text-xs font-medium rounded-lg bg-[var(--zf-ink)] text-[var(--zf-canvas)] disabled:opacity-50"
        >
          {loading ? 'Checking…' : result ? 'Run again' : 'Run pre-flight check'}
        </button>
      </div>

      {error && (
        <div role="alert" className="bg-red-50 rounded-lg border border-red-200 px-4 py-2 text-xs text-red-700">
          {error}
        </div>
      )}

      {verdict && (
        <div role="status" data-testid="preflight-verdict" className={`rounded-lg border px-4 py-2 text-sm font-medium ${TONE_STYLE[verdict.tone]}`}>
          {verdict.text}
        </div>
      )}

      {groups.map((g) => (
        <section key={g.severity} className="mt-3" aria-label={g.label}>
          <h4 className="text-xs font-medium text-[var(--zf-muted)] mb-1.5">
            {g.label} ({g.findings.length})
          </h4>
          <ul className="space-y-1.5">
            {g.findings.map((f) => (
              <li key={`${f.id}:${f.message}`} className={`rounded-lg border px-3 py-2 text-sm ${SEVERITY_STYLE[g.severity]}`}>
                <p className="text-[var(--zf-ink)]">{f.message}</p>
                {f.hint && <p className="text-xs text-[var(--zf-muted)] mt-0.5">{f.hint}</p>}
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  )
}
