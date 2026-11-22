// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { insightsHeadline, type AuditInsight, type InsightSeverity } from '../lib/auditInsights'

export interface AuditInsightsPanelProps {
  insights: AuditInsight[]
  /** How many log entries were analysed. */
  analysed: number
}

const STYLE: Record<InsightSeverity, string> = {
  critical: 'border-red-200 bg-red-50 text-red-700',
  warning: 'border-amber-200 bg-amber-50 text-amber-800',
  info: 'border-[var(--zf-hairline)] bg-black/[0.02] text-[var(--zf-ink)]',
}

const LABEL: Record<InsightSeverity, string> = { critical: 'Critical', warning: 'Warning', info: 'Note' }

/** Presentational insights panel: pure of data fetching so it renders statically. */
export function AuditInsightsPanel({ insights, analysed }: AuditInsightsPanelProps) {
  return (
    <section className="zf-panel-muted p-4 mb-4" data-testid="audit-insights" aria-label="Audit insights">
      <div className="flex flex-wrap items-baseline justify-between gap-2 mb-2">
        <h3 className="text-sm font-medium text-[var(--zf-ink)]">Insights</h3>
        <span className="text-xs text-[var(--zf-muted)]">Based on the {analysed} entries currently loaded</span>
      </div>
      <p role="status" data-testid="audit-insights-headline" className="text-sm text-[var(--zf-muted)]">
        {analysed === 0 ? 'No entries to analyse' : insightsHeadline(insights)}
      </p>
      {insights.length > 0 && (
        <ul className="mt-3 space-y-2">
          {insights.map((i) => (
            <li key={`${i.kind}-${i.actor}-${i.firstAt}`} data-testid="audit-insight" className={`rounded-lg border px-3 py-2 text-xs ${STYLE[i.severity]}`}>
              <span className="font-semibold">{LABEL[i.severity]}: </span>
              {i.title}
              <span className="block opacity-80">{i.detail}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
